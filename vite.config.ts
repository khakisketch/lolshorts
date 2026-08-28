import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

const appVersion = process.env.npm_package_version ?? "0.0.0-dev";
const sentryDsn = process.env.VITE_SENTRY_DSN ?? "";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],

  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
    __VITE_SENTRY_DSN__: JSON.stringify(sentryDsn),
  },

  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // Vite options tailored for Tauri development
  clearScreen: false,
  server: {
    port: 5181, // Tauri와 동일한 포트
    strictPort: true, // 정확히 이 포트 사용
    watch: {
      // This repository deliberately keeps Rust and field-QA artefacts outside
      // `src/`. On Windows, watching those trees can consume tens of thousands
      // of file handles and leave Vite accepting TCP connections without ever
      // answering HTTP requests. Keep the dev/E2E server scoped to source-like
      // inputs so a dirty developer checkout behaves like a clean checkout.
      ignored: [
        "**/.git/**",
        "**/node_modules/**",
        "**/src-tauri/**",
        "**/target/**",
        "**/target-*/**",
        "**/dist/**",
        "**/qa-evidence/**",
        "**/field-qa/**",
        "**/playwright-report/**",
        "**/test-results*/**",
      ],
    },
  },

  // Production build optimizations
  build: {
    // Target modern browsers for better tree-shaking
    target: "esnext",

    // Minification settings
    minify: "terser",
    terserOptions: {
      compress: {
        drop_console: true, // Remove console.log in production
        drop_debugger: true, // Remove debugger statements
        pure_funcs: ["console.log", "console.info", "console.debug"],
      },
    },

    // Chunk size warnings
    chunkSizeWarningLimit: 600, // Increase to 600 KB (from 500 KB default)

    // Manual chunk splitting for better caching
    rollupOptions: {
      output: {
        manualChunks: {
          // React core
          "react-vendor": [
            "react",
            "react/jsx-runtime",
            "react-dom",
            "react-dom/client",
          ],

          // React Router
          router: ["@tanstack/react-router"],

          // UI components library
          "ui-vendor": [
            "@radix-ui/react-dialog",
            "@radix-ui/react-select",
            "@radix-ui/react-tabs",
            "@radix-ui/react-toast",
            "@radix-ui/react-progress",
            "@radix-ui/react-slider",
            "@radix-ui/react-switch",
          ],

          // Drag and drop
          "dnd-vendor": [
            "@dnd-kit/core",
            "@dnd-kit/sortable",
            "@dnd-kit/utilities",
          ],

          // State management
          "state-vendor": ["zustand"],

          // i18n
          "i18n-vendor": [
            "i18next",
            "react-i18next",
            "i18next-browser-languagedetector",
          ],

          // Icons
          "icons-vendor": ["lucide-react", "@radix-ui/react-icons"],

          // Tauri
          "tauri-vendor": [
            "@tauri-apps/api",
            "@tauri-apps/plugin-dialog",
            "@tauri-apps/plugin-shell",
          ],

          // Optional crash reporting. `src/lib/telemetry.ts` loads this only
          // after the user enables reporting and a DSN is configured; keeping a
          // dedicated chunk prevents the SDK from inflating the initial app
          // renderer bundle.
          sentry: ["@sentry/react"],
        },
      },
    },

    // Source map for production debugging (optional)
    sourcemap: false, // Set to true if you need source maps in production

    // Optimize dependencies
    commonjsOptions: {
      include: [/node_modules/],
      transformMixedEsModules: true,
    },
  },

  // Dependency optimization
  optimizeDeps: {
    include: [
      "react",
      "react-dom",
      "@tanstack/react-router",
      "zustand",
      "i18next",
      "react-i18next",
    ],
    exclude: [
      "@tauri-apps/api",
      "@tauri-apps/plugin-shell",
      "@tauri-apps/plugin-dialog",
    ],
  },
});
