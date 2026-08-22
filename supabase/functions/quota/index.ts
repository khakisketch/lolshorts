import "@supabase/functions-js/edge-runtime.d.ts";
import {
  buildCheckResponse,
  currentMonth,
  FREE_MONTHLY_AUTO_EDIT_LIMIT,
  type LicenseRecord,
  parseAction,
  parseJobId,
  userIsProFromLicenses,
} from "./logic.ts";

// Server-authoritative Auto-Edit quota boundary.
//
// Verifies the caller's Supabase JWT, determines their tier from the
// authoritative user_licenses table with the service role, and either reports
// or atomically consumes one unit of their FREE monthly auto-edit quota. PRO
// users are unlimited and never touch the counter. The desktop client's local
// SQLite counter is only a cache / offline fallback; this function is the
// source of truth. See supabase/migrations for the table + consume RPC.

type JsonObject = Record<string, unknown>;

interface AuthUser {
  id: string;
  email?: string;
}

interface ConsumeRow {
  allowed: boolean;
  used: number;
  limit: number;
}

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
  "Access-Control-Allow-Methods": "POST,OPTIONS",
};

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: CORS_HEADERS });
  }

  try {
    if (req.method !== "POST") {
      return errorResponse(405, "METHOD_NOT_ALLOWED", "Use POST for quota actions.");
    }

    const user = await requireUser(req);
    const body = await readJson(req);
    const action = parseAction(body.action);
    if (!action) {
      return errorResponse(400, "INVALID_ACTION", "action must be 'check' or 'consume'.");
    }

    // PRO is unlimited and never reads or writes the counter.
    if (await userIsPro(user.id)) {
      return jsonResponse({ allowed: true, used: 0, limit: null, tier: "PRO", unlimited: true });
    }

    const month = currentMonth();

    if (action === "check") {
      const used = await readCurrentUsage(user.id, month);
      return jsonResponse({
        ...buildCheckResponse(used, FREE_MONTHLY_AUTO_EDIT_LIMIT),
        tier: "FREE",
      });
    }

    // consume: atomic increment-under-limit via the SECURITY DEFINER RPC.
    const jobId = parseJobId(body.job_id);
    if (!jobId) {
      return errorResponse(400, "INVALID_JOB_ID", "consume requires a non-empty job_id.");
    }
    const consumed = await consumeQuota(user.id, month, FREE_MONTHLY_AUTO_EDIT_LIMIT, jobId);
    return jsonResponse({ ...consumed, tier: "FREE" });
  } catch (error) {
    const status = error instanceof HttpError ? error.status : 500;
    const code = error instanceof HttpError ? error.code : "QUOTA_ERROR";
    const message = error instanceof HttpError
      ? error.message
      : "Quota request could not be completed.";
    return errorResponse(status, code, message);
  }
});

/** Read the authoritative PRO status from user_licenses via the service role. */
async function userIsPro(userId: string): Promise<boolean> {
  const licenses = await restArray<LicenseRecord>(
    `user_licenses?user_id=eq.${encodeURIComponent(userId)}` +
      `&select=id,user_id,tier,status,expires_at&order=created_at.desc`,
  );
  return userIsProFromLicenses(licenses);
}

/** Current month's consumed count for the user (0 when no row exists yet). */
async function readCurrentUsage(userId: string, month: string): Promise<number> {
  const rows = await restArray<{ used: number }>(
    `auto_edit_usage?user_id=eq.${encodeURIComponent(userId)}` +
      `&month=eq.${encodeURIComponent(month)}&select=used&limit=1`,
  );
  return rows[0]?.used ?? 0;
}

/** Atomically consume one unit; the RPC enforces the limit server-side. */
async function consumeQuota(
  userId: string,
  month: string,
  limit: number,
  jobId: string,
): Promise<ConsumeRow> {
  const rows = await restRpc<ConsumeRow>("consume_auto_edit_quota", {
    p_user_id: userId,
    p_month: month,
    p_limit: limit,
    p_job_id: jobId,
  });
  const row = rows[0];
  if (!row) {
    // The RPC always returns exactly one row; a missing row is unexpected, so
    // fail closed (deny) rather than silently granting a free consume.
    return { allowed: false, used: limit, limit };
  }
  return {
    allowed: Boolean(row.allowed),
    used: Number(row.used),
    limit: Number(row.limit),
  };
}

async function requireUser(req: Request): Promise<AuthUser> {
  const token = bearerToken(req);
  if (!token) {
    throw new HttpError(401, "AUTH_REQUIRED", "A Supabase access token is required.");
  }

  const response = await fetch(`${requiredEnv("SUPABASE_URL")}/auth/v1/user`, {
    headers: {
      apikey: requiredEnv("SUPABASE_ANON_KEY"),
      authorization: `Bearer ${token}`,
    },
  });

  if (!response.ok) {
    throw new HttpError(401, "AUTH_INVALID", "Supabase access token could not be verified.");
  }

  return await response.json() as AuthUser;
}

async function restArray<T>(pathAndQuery: string): Promise<T[]> {
  const response = await supabaseRest(pathAndQuery, { method: "GET" });
  return await response.json() as T[];
}

async function restRpc<T>(fn: string, args: JsonObject): Promise<T[]> {
  const response = await supabaseRest(`rpc/${fn}`, {
    method: "POST",
    body: JSON.stringify(args),
  });
  const text = await response.text();
  return text ? JSON.parse(text) as T[] : [];
}

async function supabaseRest(pathAndQuery: string, init: RequestInit): Promise<Response> {
  const base = requiredEnv("SUPABASE_URL").replace(/\/$/, "");
  const serviceRoleKey = requiredEnv("SUPABASE_SERVICE_ROLE_KEY");
  const response = await fetch(`${base}/rest/v1/${pathAndQuery}`, {
    ...init,
    headers: {
      apikey: serviceRoleKey,
      authorization: `Bearer ${serviceRoleKey}`,
      "content-type": "application/json",
      ...(init.headers ?? {}),
    },
  });

  if (!response.ok) {
    const detail = await response.text();
    throw new HttpError(response.status, "SUPABASE_REST_ERROR", detail || "Supabase REST request failed.");
  }

  return response;
}

function bearerToken(req: Request): string | null {
  const header = req.headers.get("authorization");
  if (!header?.toLowerCase().startsWith("bearer ")) {
    return null;
  }
  return header.slice("bearer ".length).trim();
}

async function readJson(req: Request): Promise<JsonObject> {
  const text = await req.text();
  if (!text) {
    return {};
  }
  const parsed = JSON.parse(text);
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new HttpError(400, "INVALID_JSON", "Request body must be a JSON object.");
  }
  return parsed as JsonObject;
}

function requiredEnv(name: string): string {
  const value = Deno.env.get(name);
  if (!value) {
    throw new HttpError(500, "QUOTA_CONFIG_MISSING", `${name} is not configured.`);
  }
  return value;
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...CORS_HEADERS, "content-type": "application/json" },
  });
}

function errorResponse(status: number, code: string, message: string): Response {
  return jsonResponse({ error: code, message }, status);
}

class HttpError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
  ) {
    super(message);
  }
}
