import { create } from "zustand";
import { persist } from "zustand/middleware";
import { supabase } from "./supabase";
import { authApi, EntitlementInfo } from "@/api/auth";
import { getErrorKey, isNetworkError } from "./errorMapper";
import { logger } from "@/lib/logger";
import type { User as SupabaseUser, Session } from "@supabase/supabase-js";

export interface UserProfile {
  id: string;
  email: string;
  display_name: string | null;
  avatar_url: string | null;
}

export interface User {
  id: string;
  email: string;
  tier: "FREE" | "PRO";
  profile: UserProfile | null;
  supabaseUser: SupabaseUser;
}

export interface LoginCredentials {
  email: string;
  password: string;
}

export interface SignupCredentials {
  email: string;
  password: string;
  confirm_password: string;
}

export type SignupResult = "signed_in" | "confirmation_required";

export interface LicenseInfo {
  tier: "FREE" | "PRO";
  expires_at?: string;
  features: string[];
}

interface AuthState {
  user: User | null;
  entitlement: EntitlementInfo | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  error: string | null;

  login: (credentials: LoginCredentials) => Promise<void>;
  signup: (credentials: SignupCredentials) => Promise<SignupResult>;
  logout: () => Promise<void>;
  refreshToken: () => Promise<void>;
  syncSession: (session: Session) => Promise<void>;
  syncSignedOut: () => Promise<void>;
  refreshEntitlement: () => Promise<EntitlementInfo | null>;
  checkAuth: () => Promise<void>;
  getLicenseInfo: () => Promise<LicenseInfo | null>;
  clearError: () => void;
}

let backendLogoutInFlight: Promise<void> | null = null;

function tierFromEntitlement(
  entitlement: EntitlementInfo | null,
): "FREE" | "PRO" {
  return entitlement?.tier === "PRO" && entitlement.status === "active"
    ? "PRO"
    : "FREE";
}

function failClosedEntitlement(): EntitlementInfo {
  return {
    tier: "FREE",
    status: "none",
    expires_at: null,
    source: "supabase",
    checked_at: new Date().toISOString(),
    payment_available: false,
  };
}

function featuresForTier(tier: "FREE" | "PRO"): string[] {
  return tier === "PRO"
    ? [
        "unlimited_clips",
        "advanced_editor",
        "priority_support",
        "no_watermarks",
      ]
    : ["basic_clips", "basic_editor"];
}

function buildUser(
  supabaseUser: SupabaseUser,
  entitlement: EntitlementInfo | null,
  profile: UserProfile | null,
): User {
  return {
    id: supabaseUser.id,
    email: supabaseUser.email!,
    tier: tierFromEntitlement(entitlement),
    profile,
    supabaseUser,
  };
}

async function fetchUserProfile(
  user: SupabaseUser,
): Promise<UserProfile | null> {
  const { data, error } = await supabase
    .from("user_profiles")
    .select("id,email,display_name,avatar_url")
    .eq("id", user.id)
    .single();

  if (error) {
    logger.warn("[AuthStore] User profile lookup failed:", error);
    return null;
  }

  return (data as UserProfile) || null;
}

async function ensureUserProfile(user: SupabaseUser): Promise<void> {
  const { error } = await supabase.from("user_profiles").insert({
    id: user.id,
    email: user.email!,
  });

  if (error && error.code !== "23505") {
    logger.warn("[AuthStore] User profile insert failed:", error);
  }
}

async function fetchOrCreateUserProfile(
  user: SupabaseUser,
): Promise<UserProfile | null> {
  const existing = await fetchUserProfile(user);
  if (existing) return existing;

  await ensureUserProfile(user);
  return fetchUserProfile(user);
}

async function syncBackendSession(session: Session): Promise<EntitlementInfo> {
  const retryDelaysMs = [0, 500, 1_500] as const;
  let lastError: unknown;

  for (const delayMs of retryDelaysMs) {
    if (delayMs > 0) {
      await new Promise<void>((resolve) => setTimeout(resolve, delayMs));
    }
    try {
      const response = await authApi.setSession(
        session.access_token,
        session.refresh_token,
        session.user.id,
        session.user.email || "",
        session.expires_at ?? null,
      );
      return response.entitlement;
    } catch (error) {
      lastError = error;
      if (!isNetworkError(error)) throw error;
    }
  }

  throw lastError;
}

function requireSession(session: Session | null): Session {
  if (!session) {
    throw new Error("No authenticated Supabase session returned");
  }
  return session;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      user: null,
      entitlement: null,
      isAuthenticated: false,
      isLoading: false,
      error: null,

      login: async (credentials) => {
        set({ isLoading: true, error: null });
        try {
          const { data, error } = await supabase.auth.signInWithPassword({
            email: credentials.email,
            password: credentials.password,
          });

          if (error) throw error;
          if (!data.user) throw new Error("No user returned");

          const session = requireSession(data.session);
          const entitlement = await syncBackendSession(session);
          const profile = await fetchOrCreateUserProfile(data.user);
          const user = buildUser(data.user, entitlement, profile);

          set({
            user,
            entitlement,
            isAuthenticated: true,
            isLoading: false,
            error: null,
          });
        } catch (error: unknown) {
          const errorKey = getErrorKey(error);
          set({
            error: errorKey,
            isLoading: false,
          });
          throw new Error(errorKey);
        }
      },

      signup: async (credentials) => {
        if (credentials.password !== credentials.confirm_password) {
          const errorKey = "errors.passwordsDoNotMatch";
          set({ error: errorKey });
          throw new Error(errorKey);
        }

        set({ isLoading: true, error: null });
        try {
          const { data, error } = await supabase.auth.signUp({
            email: credentials.email,
            password: credentials.password,
          });

          if (error) throw error;
          if (!data.user) throw new Error("No user returned");

          // No session is Supabase's successful email-confirmation path. Do
          // not attempt authenticated profile or entitlement calls until the
          // user has confirmed the email address and signed in.
          if (!data.session) {
            set({
              user: null,
              entitlement: null,
              isAuthenticated: false,
              isLoading: false,
              error: null,
            });
            return "confirmation_required";
          }

          const session = data.session;
          await ensureUserProfile(data.user);
          const entitlement = await syncBackendSession(session);
          const profile = await fetchUserProfile(data.user);
          const user = buildUser(data.user, entitlement, profile);

          set({
            user,
            entitlement,
            isAuthenticated: true,
            isLoading: false,
            error: null,
          });
          return "signed_in";
        } catch (error: unknown) {
          const errorKey = getErrorKey(error);
          set({
            error: errorKey,
            isLoading: false,
          });
          throw new Error(errorKey);
        }
      },

      logout: async () => {
        set({ isLoading: true, error: null });
        try {
          const { error } = await supabase.auth.signOut();
          if (error) throw error;
          await get().syncSignedOut();
        } catch (error: unknown) {
          const errorKey = getErrorKey(error);
          set({
            error: errorKey,
            isLoading: false,
          });
          throw new Error(errorKey);
        }
      },

      refreshToken: async () => {
        try {
          const { data, error } = await supabase.auth.refreshSession();
          if (error) throw error;

          if (data.user && data.session) {
            await get().syncSession(data.session);
          }
        } catch (error) {
          logger.error("[AuthStore] Token refresh failed:", error);
          set({
            user: null,
            entitlement: null,
            isAuthenticated: false,
            error: "errors.sessionExpired",
          });
        }
      },

      // Supabase JS owns refresh scheduling. Every refreshed session must also
      // replace the Rust-side access/refresh tokens used by entitlement, quota,
      // and YouTube command authorization during long-running app sessions.
      syncSession: async (session) => {
        try {
          const entitlement = await syncBackendSession(session);
          const profile = await fetchOrCreateUserProfile(session.user);
          const user = buildUser(session.user, entitlement, profile);

          set({
            user,
            entitlement,
            isAuthenticated: true,
            isLoading: false,
            error: null,
          });
        } catch (error: unknown) {
          const errorKey = getErrorKey(error);
          set({ error: errorKey, isLoading: false });
          throw new Error(errorKey);
        }
      },

      // A SIGNED_OUT event can be raised by an explicit logout, a revoked
      // refresh token, or another device. Clear local authorization first and
      // coalesce the Rust-side logout so explicit and event-driven paths cannot
      // race or leave stale command authorization behind.
      syncSignedOut: async () => {
        const hadAuthenticatedSession =
          get().isAuthenticated || get().user !== null;

        set({
          user: null,
          entitlement: null,
          isAuthenticated: false,
          isLoading: false,
          error: null,
        });

        if (!backendLogoutInFlight && hadAuthenticatedSession) {
          backendLogoutInFlight = authApi.logout().finally(() => {
            backendLogoutInFlight = null;
          });
        }

        if (backendLogoutInFlight) {
          try {
            await backendLogoutInFlight;
          } catch (error) {
            // Local state remains signed out (fail closed). The backend session
            // is in-memory only and is also discarded when the app exits.
            logger.warn(
              "[AuthStore] Backend logout synchronization failed:",
              error,
            );
          }
        }
      },

      refreshEntitlement: async () => {
        try {
          const { user } = get();
          if (!user) return null;

          const entitlement = await authApi.getCurrentEntitlement(true);
          set({
            entitlement,
            user: {
              ...user,
              tier: tierFromEntitlement(entitlement),
            },
          });
          return entitlement;
        } catch (error) {
          logger.error("[AuthStore] Failed to refresh entitlement:", error);
          const { user } = get();
          const entitlement = failClosedEntitlement();
          set({
            entitlement,
            user: user
              ? {
                  ...user,
                  tier: "FREE",
                }
              : null,
          });
          return entitlement;
        }
      },

      checkAuth: async () => {
        set({ isLoading: true, error: null });
        try {
          const {
            data: { session },
            error,
          } = await supabase.auth.getSession();

          if (error) throw error;

          if (session?.user) {
            const entitlement = await syncBackendSession(session);
            const profile = await fetchOrCreateUserProfile(session.user);
            const user = buildUser(session.user, entitlement, profile);

            set({
              user,
              entitlement,
              isAuthenticated: true,
              isLoading: false,
            });
          } else {
            set({
              user: null,
              entitlement: null,
              isAuthenticated: false,
              isLoading: false,
            });
          }
        } catch (error: unknown) {
          set({
            error: getErrorKey(error),
            isLoading: false,
            user: null,
            entitlement: null,
            isAuthenticated: false,
          });
        }
      },

      getLicenseInfo: async () => {
        try {
          const entitlement = await get().refreshEntitlement();
          if (!entitlement) return null;

          const tier = tierFromEntitlement(entitlement);
          return {
            tier,
            expires_at: entitlement.expires_at || undefined,
            features: featuresForTier(tier),
          };
        } catch (error) {
          logger.error("[AuthStore] Failed to get license info:", error);
          return null;
        }
      },

      clearError: () => set({ error: null }),
    }),
    {
      name: "lolshorts-auth",
      partialize: () => ({}),
    },
  ),
);
