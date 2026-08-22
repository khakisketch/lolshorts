// Pure, dependency-free helpers for the `quota` Edge Function.
//
// Kept in a separate module from index.ts so they can be unit-tested with
// `deno test` WITHOUT booting the HTTP server -- index.ts calls `Deno.serve`
// at module load time, so importing it in a test would start a listener.

/** FREE tier: number of auto-edits allowed per calendar month. */
export const FREE_MONTHLY_AUTO_EDIT_LIMIT = 5;

export interface LicenseRecord {
  id?: string;
  user_id?: string;
  tier: string;
  status: string;
  expires_at: string | null;
}

export type QuotaAction = "check" | "consume";

/** Validate the request `action`; returns null for anything unsupported. */
export function parseAction(value: unknown): QuotaAction | null {
  return value === "check" || value === "consume" ? value : null;
}

export function parseJobId(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 && trimmed.length <= 160 ? trimmed : null;
}

/** UTC calendar month key, 'YYYY-MM'. */
export function currentMonth(date: Date = new Date()): string {
  const year = date.getUTCFullYear();
  const month = String(date.getUTCMonth() + 1).padStart(2, "0");
  return `${year}-${month}`;
}

export interface CheckResponse {
  allowed: boolean;
  used: number;
  limit: number;
}

/** Shape a `check` response: allowed iff strictly under the limit. */
export function buildCheckResponse(used: number, limit: number): CheckResponse {
  const safeUsed = Number.isFinite(used) && used > 0 ? Math.floor(used) : 0;
  return { allowed: safeUsed < limit, used: safeUsed, limit };
}

/** A license is unexpired when it has no expiry, or expires in the future. */
export function licenseNotExpired(
  expiresAt: string | null,
  now: number = Date.now(),
): boolean {
  if (!expiresAt) return true;
  const expires = Date.parse(expiresAt);
  return Number.isFinite(expires) && expires > now;
}

/**
 * Server-side PRO determination: the user is PRO (and therefore unlimited)
 * only if they hold at least one active, unexpired PRO license. Everything
 * else fails closed to FREE. Mirrors the desktop client's license selection.
 */
export function userIsProFromLicenses(
  licenses: LicenseRecord[],
  now: number = Date.now(),
): boolean {
  return licenses.some(
    (license) =>
      license.tier === "PRO" &&
      license.status === "active" &&
      licenseNotExpired(license.expires_at, now),
  );
}
