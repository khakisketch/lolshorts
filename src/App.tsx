import { useEffect, lazy, Suspense, useState, type ReactNode } from "react";
import {
  Router,
  Route,
  RootRoute,
  RouterProvider,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { AppShell } from "@/components/layout/AppShell";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { Card, CardContent } from "@/components/ui/card";
import { Loader2 } from "lucide-react";
import Overlay from "@/pages/Overlay";
import { Home } from "@/pages/Home";
import { supabase } from "@/lib/supabase";
import { useAuthStore } from "@/lib/auth";
import "./i18n"; // Initialize i18n with auto language detection
import { ReplayTargetModal } from "@/components/overlay/ReplayTargetModal";
import { OnboardingModal } from "@/components/onboarding/OnboardingModal";
import { logger } from "@/lib/logger";
import { settingsApi } from "@/api/settings";
import { captureError, configureErrorTelemetry } from "@/lib/telemetry";
import { AppUpdateDialog } from "@/components/updater/AppUpdateDialog";
import { ProtectedFeature } from "@/components/auth/ProtectedFeature";

// Lazy load secondary pages for better performance. The dashboard is eager-loaded
// because it is the first route and must not leave the app in a blank Suspense state.
const Editor = lazy(() =>
  import("@/pages/Editor").then((m) => ({ default: m.Editor })),
);
const Results = lazy(() =>
  import("@/pages/Results").then((m) => ({ default: m.Results })),
);
const Settings = lazy(() =>
  import("@/pages/Settings").then((m) => ({ default: m.Settings })),
);
// Not in the sidebar, reached from 결과 — same pattern as /editor. It was briefly a
// redirect, which cut the only path to `start_auto_edit` and left the app unable to
// produce a highlight at all.
const AutoEdit = lazy(() =>
  import("@/pages/AutoEdit").then((m) => ({ default: m.AutoEdit })),
);
const PaymentSuccess = lazy(() =>
  import("@/pages/PaymentSuccess").then((m) => ({ default: m.PaymentSuccess })),
);
const PaymentFail = lazy(() =>
  import("@/pages/PaymentFail").then((m) => ({ default: m.PaymentFail })),
);

// Loading component for lazy loaded pages
const LoadingSpinner = () => (
  <Card className="w-full h-96 flex items-center justify-center">
    <CardContent>
      <Loader2 className="h-8 w-8 animate-spin" />
      <p className="mt-2 text-sm text-muted-foreground">로딩 중...</p>
    </CardContent>
  </Card>
);

const FeatureRoute = ({ children }: { children: ReactNode }) => (
  <ErrorBoundary>
    <Suspense fallback={<LoadingSpinner />}>{children}</Suspense>
  </ErrorBoundary>
);

// Define root route
const rootRoute = new RootRoute({
  component: () => (
    <AppShell>
      <Outlet />
    </AppShell>
  ),
});

// Define individual routes
const indexRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => (
    <FeatureRoute>
      <Home />
    </FeatureRoute>
  ),
});

// Legacy top-level screens. They are no longer entry points of their own —
// everything a user owns now lives on /results — but the paths stay alive so
// old links, bookmarks and deep links land on the right tab instead of a 404.
const gamesRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/games",
  beforeLoad: () => {
    throw redirect({ to: "/results", search: { tab: "clips" } });
  },
});

const replaysRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/replays",
  beforeLoad: () => {
    throw redirect({ to: "/results", search: { tab: "replays" } });
  },
});

const editorRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/editor",
  component: () => (
    <FeatureRoute>
      <ProtectedFeature requiresPro={false} featureName="Editor & export">
        <Editor />
      </ProtectedFeature>
    </FeatureRoute>
  ),
});

const autoEditRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/auto-edit",
  component: () => (
    <FeatureRoute>
      <AutoEdit />
    </FeatureRoute>
  ),
});

const RESULTS_TABS = ["clips", "highlights", "games", "replays"] as const;
type ResultsTab = (typeof RESULTS_TABS)[number];

const resultsRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/results",
  validateSearch: (search: Record<string, unknown>): { tab?: ResultsTab } => {
    const tab = search.tab;
    return typeof tab === "string" &&
      (RESULTS_TABS as readonly string[]).includes(tab)
      ? { tab: tab as ResultsTab }
      : {};
  },
  component: () => (
    <FeatureRoute>
      <Results />
    </FeatureRoute>
  ),
});

const youtubeRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/youtube",
  beforeLoad: () => {
    throw redirect({ to: "/results", search: { tab: "highlights" } });
  },
});

const settingsRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: () => (
    <FeatureRoute>
      <Settings />
    </FeatureRoute>
  ),
});

const paymentSuccessRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/payment/success",
  component: () => (
    <FeatureRoute>
      <PaymentSuccess />
    </FeatureRoute>
  ),
});

const paymentFailRoute = new Route({
  getParentRoute: () => rootRoute,
  path: "/payment/fail",
  component: () => (
    <FeatureRoute>
      <PaymentFail />
    </FeatureRoute>
  ),
});

// Create route tree
const routeTree = rootRoute.addChildren([
  indexRoute,
  gamesRoute,
  replaysRoute,
  editorRoute,
  autoEditRoute,
  resultsRoute,
  youtubeRoute,
  settingsRoute,
  paymentSuccessRoute,
  paymentFailRoute,
]);

// Create router instance
const router = new Router({ routeTree });

// Type augmentation for TypeScript
declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

import { useRecordingStore } from "@/stores/recordingStore"; // Add import
import { Toaster } from "@/components/ui/toaster";

export default function App() {
  // Detect if this is the overlay window (loads /overlay URL without AppShell)
  const isOverlay = window.location.pathname === "/overlay";
  if (isOverlay) {
    // The overlay window is `transparent: true`, but the global `body` rule paints
    // an opaque dark background — which rendered the in-game REC badge as a black
    // box sitting on top of the game. This class turns the window's background off
    // (see the `.overlay-window` rule in styles/globals.css). Set synchronously
    // rather than in an effect so the window never paints opaque for a frame.
    document.documentElement.classList.add("overlay-window");
    return <Overlay />;
  }

  return <MainApp />;
}

function MainApp() {
  const { checkAuth, syncSession, syncSignedOut } = useAuthStore();
  const { startStatusPolling, stopStatusPolling } = useRecordingStore(); // Use hook
  const [isReplayModalOpen, setIsReplayModalOpen] = useState(false);

  useEffect(() => {
    let isMounted = true;

    // Check for existing session on mount
    const initAuth = async () => {
      if (isMounted) {
        await checkAuth();
      }
    };
    initAuth();

    // Start polling recording status (sync frontend with backend)
    startStatusPolling();

    // Crash reporting is optional and follows the persisted user preference.
    // A missing DSN or a disabled preference keeps the renderer fully local.
    void settingsApi
      .getRecordingSettings()
      .then((settings) =>
        configureErrorTelemetry(settings.crash_reporting_enabled),
      )
      .catch(() => configureErrorTelemetry(false));

    // Keep Rust's command-authorization session aligned with Supabase JS. The
    // callback stays synchronous to avoid blocking Supabase's auth event lock;
    // network synchronization runs in a detached promise.
    const {
      data: { subscription },
    } = supabase.auth.onAuthStateChange((event, session) => {
      if (!isMounted) return;

      if (
        (event === "TOKEN_REFRESHED" || event === "USER_UPDATED") &&
        session
      ) {
        void syncSession(session).catch((error) => {
          logger.error("Failed to synchronize refreshed auth session:", error);
        });
      } else if (event === "SIGNED_OUT") {
        void syncSignedOut();
      }
    });

    return () => {
      isMounted = false;
      subscription.unsubscribe();
      stopStatusPolling();
    };
    // Note: These functions are stable from Zustand store, but we include them for eslint
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <ErrorBoundary
      onError={(error, errorInfo) => {
        logger.error("App-level error caught:", error, errorInfo);
        captureError(error, errorInfo.componentStack);
      }}
    >
      <RouterProvider router={router} />
      <ReplayTargetModal
        isOpen={isReplayModalOpen}
        onClose={() => setIsReplayModalOpen(false)}
      />
      <OnboardingModal />
      <AppUpdateDialog />
      <Toaster />
    </ErrorBoundary>
  );
}
