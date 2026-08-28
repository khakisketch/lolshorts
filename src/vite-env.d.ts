/// <reference types="vite/client" />

interface ImportMetaEnv {
  // Vite built-in
  readonly MODE: string;
  readonly DEV: boolean;
  readonly PROD: boolean;
  readonly SSR: boolean;

  // Application
  readonly VITE_APP_TITLE: string;
  readonly VITE_VERSION: string;
  readonly VITE_API_URL?: string;
  readonly VITE_SENTRY_DSN?: string;

  // Supabase (Required for authentication)
  readonly VITE_SUPABASE_URL: string;
  readonly VITE_SUPABASE_ANON_KEY?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

declare const __APP_VERSION__: string;
declare const __VITE_SENTRY_DSN__: string;
