export const UNCONFIGURED_SUPABASE_URL = "http://localhost:54321";
export const UNCONFIGURED_SUPABASE_ANON_KEY = "unconfigured-public-anon-key";

export interface FrontendSupabaseConfig {
  url: string;
  anonKey: string;
  configured: boolean;
}

export const isPrivilegedSupabaseKey = (value: string): boolean => {
  const normalized = value.trim();
  const lower = normalized.toLowerCase();
  if (
    !normalized ||
    lower.includes("service_role") ||
    lower.includes("service-role") ||
    lower.startsWith("sb_secret_") ||
    lower.includes("your-anon-key")
  ) {
    return true;
  }

  const payload = normalized.split(".")[1];
  if (!payload || typeof globalThis.atob !== "function") return false;
  try {
    const padded =
      payload.replace(/-/g, "+").replace(/_/g, "/") +
      "=".repeat((4 - (payload.length % 4)) % 4);
    const claims = JSON.parse(globalThis.atob(padded)) as { role?: unknown };
    return claims.role === "service_role";
  } catch {
    return false;
  }
};

export const resolveFrontendSupabaseConfig = (
  rawUrl: string | undefined,
  rawAnonKey: string | undefined,
  production: boolean,
): FrontendSupabaseConfig => {
  const url = rawUrl?.trim() ?? "";
  const anonKey = rawAnonKey?.trim() ?? "";

  if (production) {
    if (!url) {
      throw new Error("[Security] VITE_SUPABASE_URL is required in production");
    }
    if (!anonKey) {
      throw new Error(
        "[Security] VITE_SUPABASE_ANON_KEY is required in production",
      );
    }

    let parsed: URL;
    try {
      parsed = new URL(url);
    } catch {
      throw new Error(
        "[Security] VITE_SUPABASE_URL must use HTTPS in production",
      );
    }
    if (
      parsed.protocol !== "https:" ||
      !parsed.hostname ||
      parsed.username ||
      parsed.password ||
      parsed.port ||
      parsed.search ||
      parsed.hash
    ) {
      throw new Error(
        "[Security] VITE_SUPABASE_URL must use HTTPS in production",
      );
    }
    if (isPrivilegedSupabaseKey(anonKey)) {
      throw new Error(
        "[Security] VITE_SUPABASE_ANON_KEY must be a public anon/publishable key",
      );
    }
  }

  return {
    url: url || UNCONFIGURED_SUPABASE_URL,
    anonKey: anonKey || UNCONFIGURED_SUPABASE_ANON_KEY,
    configured: Boolean(url && anonKey),
  };
};
