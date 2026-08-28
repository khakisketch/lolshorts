import {
  UNCONFIGURED_SUPABASE_ANON_KEY,
  UNCONFIGURED_SUPABASE_URL,
  resolveFrontendSupabaseConfig,
} from "./supabaseConfig";

describe("resolveFrontendSupabaseConfig", () => {
  it("keeps a clean development checkout renderable without pretending auth is configured", () => {
    expect(resolveFrontendSupabaseConfig(undefined, undefined, false)).toEqual({
      url: UNCONFIGURED_SUPABASE_URL,
      anonKey: UNCONFIGURED_SUPABASE_ANON_KEY,
      configured: false,
    });
  });

  it("fails closed when production configuration is missing", () => {
    expect(() =>
      resolveFrontendSupabaseConfig(undefined, undefined, true),
    ).toThrow("VITE_SUPABASE_URL is required");
    expect(() =>
      resolveFrontendSupabaseConfig("https://project.supabase.co", "", true),
    ).toThrow("VITE_SUPABASE_ANON_KEY is required");
  });

  it("requires an HTTPS production URL", () => {
    expect(() =>
      resolveFrontendSupabaseConfig(
        "http://project.supabase.co",
        "public-key",
        true,
      ),
    ).toThrow("must use HTTPS");
    expect(() =>
      resolveFrontendSupabaseConfig("not a url", "public-key", true),
    ).toThrow("must use HTTPS");
  });

  it("rejects privileged production keys", () => {
    expect(() =>
      resolveFrontendSupabaseConfig(
        "https://project.supabase.co",
        "sb_secret_never-embed-this",
        true,
      ),
    ).toThrow("public anon/publishable key");
  });

  it("rejects credentials, ports, and query/fragment components", () => {
    for (const url of [
      "https://user:password@project.supabase.co",
      "https://project.supabase.co:8443",
      "https://project.supabase.co?tenant=other",
      "https://project.supabase.co#fragment",
    ]) {
      expect(() =>
        resolveFrontendSupabaseConfig(url, "public-key", true),
      ).toThrow("must use HTTPS");
    }
  });

  it("preserves a valid public production configuration", () => {
    expect(
      resolveFrontendSupabaseConfig(
        " https://project.supabase.co ",
        " sb_publishable_public-key ",
        true,
      ),
    ).toEqual({
      url: "https://project.supabase.co",
      anonKey: "sb_publishable_public-key",
      configured: true,
    });
  });
});
