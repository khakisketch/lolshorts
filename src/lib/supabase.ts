import { createClient } from "@supabase/supabase-js";
import { resolveFrontendSupabaseConfig } from "./supabaseConfig";

// Environment helpers - works in both Vite and Jest
const getEnvVar = (key: string): string | undefined => {
  try {
    // Vite environment
    return (import.meta.env as Record<string, string>)?.[key];
  } catch {
    // Jest/Node environment
    return process.env[key];
  }
};

const isProd = (): boolean => {
  try {
    return import.meta.env?.PROD ?? false;
  } catch {
    return process.env.NODE_ENV === "production";
  }
};

const isDev = (): boolean => {
  try {
    return import.meta.env?.DEV ?? false;
  } catch {
    return process.env.NODE_ENV !== "production";
  }
};

// Supabase configuration - requires environment variables
const supabaseUrl = getEnvVar("VITE_SUPABASE_URL");
const supabaseAnonKey = getEnvVar("VITE_SUPABASE_ANON_KEY");
const resolvedConfig = resolveFrontendSupabaseConfig(
  supabaseUrl,
  supabaseAnonKey,
  isProd(),
);

// Log warning in development if using fallbacks
if (isDev() && (!supabaseUrl || !supabaseAnonKey)) {
  // eslint-disable-next-line no-console
  console.info(
    "[Dev] VITE_SUPABASE_URL or VITE_SUPABASE_ANON_KEY not set. Auth features will be unavailable without a valid anon key.",
  );
}

export const supabase = createClient(
  resolvedConfig.url,
  resolvedConfig.anonKey,
  {
    auth: {
      autoRefreshToken: true,
      persistSession: true,
      // The desktop release intentionally exposes no browser OAuth/deep-link
      // callback. Do not interpret arbitrary WebView URL fragments as sessions.
      detectSessionInUrl: false,
      flowType: "pkce",
    },
  },
);

export type Database = {
  public: {
    Tables: {
      user_profiles: {
        Row: {
          id: string;
          email: string;
          display_name: string | null;
          avatar_url: string | null;
          created_at: string;
          updated_at: string;
        };
        Insert: {
          id: string;
          email: string;
          display_name?: string | null;
          avatar_url?: string | null;
        };
        Update: {
          display_name?: string | null;
          avatar_url?: string | null;
        };
      };
      user_licenses: {
        Row: {
          id: string;
          user_id: string;
          tier: "FREE" | "PRO";
          status: "active" | "inactive" | "expired" | "cancelled" | "none";
          started_at: string | null;
          expires_at: string | null;
          cancelled_at: string | null;
          created_at: string;
          updated_at: string;
        };
        Insert: never;
        Update: never;
      };
      games: {
        Row: {
          game_id: number;
          user_id: string;
          game_start_time: string;
          game_end_time: string | null;
          champion_name: string | null;
          game_mode: string | null;
          game_result: "Victory" | "Defeat" | "Remake" | null;
          kills: number;
          deaths: number;
          assists: number;
          metadata: Record<string, unknown>;
          created_at: string;
          updated_at: string;
        };
        Insert: {
          game_id: number;
          user_id: string;
          game_start_time: string;
          game_end_time?: string | null;
          champion_name?: string | null;
          game_mode?: string | null;
          game_result?: "Victory" | "Defeat" | "Remake" | null;
          kills?: number;
          deaths?: number;
          assists?: number;
          metadata?: Record<string, unknown>;
        };
        Update: {
          game_id?: number;
          user_id?: string;
          game_start_time?: string;
          game_end_time?: string | null;
          champion_name?: string | null;
          game_mode?: string | null;
          game_result?: "Victory" | "Defeat" | "Remake" | null;
          kills?: number;
          deaths?: number;
          assists?: number;
          metadata?: Record<string, unknown>;
        };
      };
      clips: {
        Row: {
          id: number;
          game_id: number;
          user_id: string;
          file_path: string;
          event_type: string;
          event_time: number;
          priority: number;
          duration_secs: number;
          thumbnail_path: string | null;
          metadata: Record<string, unknown>;
          created_at: string;
        };
        Insert: {
          game_id: number;
          user_id: string;
          file_path: string;
          event_type: string;
          event_time: number;
          priority: number;
          duration_secs?: number;
          thumbnail_path?: string | null;
          metadata?: Record<string, unknown>;
        };
        Update: {
          game_id?: number;
          user_id?: string;
          file_path?: string;
          event_type?: string;
          event_time?: number;
          priority?: number;
          duration_secs?: number;
          thumbnail_path?: string | null;
          metadata?: Record<string, unknown>;
        };
      };
    };
  };
};
