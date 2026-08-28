type SentryModule = typeof import("@sentry/react");

let initialized = false;
let userEnabled = false;
let sentry: SentryModule | null = null;
let loadPromise: Promise<SentryModule | null> | null = null;

function configuredDsn(): string | undefined {
  const dsn = (
    typeof __VITE_SENTRY_DSN__ === "string"
      ? __VITE_SENTRY_DSN__
      : typeof process !== "undefined"
        ? process.env.VITE_SENTRY_DSN
        : undefined
  )?.trim();
  return dsn || undefined;
}

/**
 * Apply the user's crash-reporting preference to the optional Sentry client.
 *
 * The DSN is a public client configuration value, but the preference remains
 * opt-in. Closing the client when the preference is switched off prevents the
 * browser SDK from retaining a live transport for the rest of the session.
 */
export function configureErrorTelemetry(enabled: boolean): void {
  userEnabled = enabled;

  if (!enabled) {
    if (initialized && sentry) {
      void sentry.close();
      initialized = false;
    }
    // A preference change can happen while the optional SDK is loading. The
    // completion handler checks `userEnabled` before initializing it.
    return;
  }

  if (initialized || loadPromise) return;

  const dsn = configuredDsn();
  if (!dsn) return;

  const release = typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "dev";

  // Keep the optional SDK out of the initial renderer path. This avoids
  // loading Sentry for the default opt-out case while preserving the existing
  // synchronous preference API used by settings and application bootstrap.
  loadPromise = import("@sentry/react")
    .then((module) => {
      if (!userEnabled) return module;
      module.init({
        dsn,
        sendDefaultPii: false,
        enabled: true,
        debug: false,
        release,
        environment:
          typeof process !== "undefined" ? process.env.NODE_ENV : "production",
      });
      sentry = module;
      initialized = true;
      return module;
    })
    .catch(() => {
      // Telemetry is optional; a failed dynamic import must never affect the
      // recording, library, or export paths.
      return null;
    })
    .finally(() => {
      loadPromise = null;
    });
}

export function captureError(
  error: unknown,
  componentStack?: string | null,
): void {
  if (!userEnabled || !initialized || !sentry) return;

  sentry.captureException(error, {
    extra: componentStack ? { componentStack } : undefined,
  });
}
