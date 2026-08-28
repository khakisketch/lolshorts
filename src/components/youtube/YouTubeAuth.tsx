import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useYouTube } from "@/hooks/useYouTube";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Youtube, LogOut, CheckCircle, AlertCircle } from "lucide-react";
import { open } from "@tauri-apps/plugin-shell";
import { logger } from "@/lib/logger";

// The backend's local OAuth callback server times out after 120s
// (callback_server.rs); give the UI a matching fallback so it never waits
// forever if the 'youtube-auth-completed'/'youtube-auth-failed' events are
// somehow missed.
const AUTH_WAIT_TIMEOUT_MS = 2 * 60 * 1000;

export function YouTubeAuth() {
  const { t } = useTranslation();
  const {
    authStatus,
    isLoading,
    error,
    startAuthWithServer,
    logout,
    checkAuthStatus,
    authEventError,
    clearAuthEventError,
  } = useYouTube();

  const [authInProgress, setAuthInProgress] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const authTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearAuthTimeout = () => {
    if (authTimeoutRef.current) {
      clearTimeout(authTimeoutRef.current);
      authTimeoutRef.current = null;
    }
  };

  useEffect(() => {
    checkAuthStatus();
    return () => clearAuthTimeout();
  }, [checkAuthStatus]);

  // 'youtube-auth-completed' triggers checkAuthStatus() inside useYouTube,
  // so authStatus.authenticated flipping to true while we're waiting is our
  // completion signal.
  useEffect(() => {
    if (authInProgress && authStatus.authenticated) {
      setAuthInProgress(false);
      clearAuthTimeout();
    }
  }, [authInProgress, authStatus.authenticated]);

  // 'youtube-auth-failed' surfaces here as authEventError.
  useEffect(() => {
    if (authInProgress && authEventError) {
      setAuthInProgress(false);
      setActionError(authEventError);
      clearAuthEventError();
      clearAuthTimeout();
    }
  }, [authInProgress, authEventError, clearAuthEventError]);

  const handleStartAuth = async () => {
    try {
      setAuthInProgress(true);
      setActionError(null);
      clearAuthEventError();
      const authUrl = await startAuthWithServer();

      // Open auth URL in system browser
      await open(authUrl);

      // Keep authInProgress true - resolved by the 'youtube-auth-completed'/
      // 'youtube-auth-failed' effects above once the background callback
      // server (port 9090) finishes. Fall back to a timeout so the UI never
      // waits forever if the event is missed.
      clearAuthTimeout();
      authTimeoutRef.current = setTimeout(() => {
        setAuthInProgress(false);
        setActionError(
          t(
            "youtube.auth.authTimeout",
            "Authorization timed out. Please try again.",
          ),
        );
      }, AUTH_WAIT_TIMEOUT_MS);
    } catch (err) {
      logger.error("Auth error:", err);
      setAuthInProgress(false);
      setActionError(
        err instanceof Error
          ? err.message
          : t(
              "youtube.auth.authStartFailed",
              "Failed to start YouTube authentication.",
            ),
      );
    }
  };

  const handleLogout = async () => {
    try {
      setActionError(null);
      await logout();
    } catch (err) {
      logger.error("Logout error:", err);
      setActionError(
        err instanceof Error
          ? err.message
          : t(
              "youtube.auth.logoutFailed",
              "Failed to disconnect YouTube account.",
            ),
      );
    }
  };

  const formatExpiryDate = (timestamp: number) => {
    const date = new Date(timestamp * 1000);
    return date.toLocaleDateString() + " " + date.toLocaleTimeString();
  };

  return (
    <div className="gaming-panel p-6">
      <div className="mb-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Youtube className="h-6 w-6 text-red-500" />
            <div>
              <h3 className="text-lg font-semibold">
                {t("youtube.auth.youtubeAccount")}
              </h3>
              <p className="text-sm text-muted-foreground">
                {t("youtube.auth.connectDescription")}
              </p>
            </div>
          </div>
          {authStatus.authenticated ? (
            <Badge variant="default" className="gap-1">
              <CheckCircle className="h-3 w-3" />
              {t("youtube.auth.connected")}
            </Badge>
          ) : (
            <Badge variant="secondary" className="gap-1">
              <AlertCircle className="h-3 w-3" />
              {t("youtube.auth.disconnected")}
            </Badge>
          )}
        </div>
      </div>

      <div className="space-y-4">
        {(actionError || error) && (
          <Alert variant="destructive">
            <AlertDescription>{actionError || error}</AlertDescription>
          </Alert>
        )}

        {authStatus.authenticated ? (
          <div className="space-y-4">
            <div className="flex items-center justify-between p-4 bg-muted rounded-lg">
              <div>
                <p className="text-sm font-medium">
                  {t("youtube.auth.signedInAs")}
                </p>
                <p className="text-sm text-muted-foreground">
                  {t("youtube.auth.youtubeAccount")}
                </p>
                {authStatus.expires_at && (
                  <p className="text-xs text-muted-foreground mt-1">
                    {t("youtube.auth.tokenExpires")}{" "}
                    {formatExpiryDate(authStatus.expires_at)}
                  </p>
                )}
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={handleLogout}
                disabled={isLoading}
              >
                <LogOut className="h-4 w-4 mr-2" />
                {t("youtube.auth.signOut")}
              </Button>
            </div>

            <Alert>
              <AlertDescription>
                {t("youtube.auth.accountConnected")}
              </AlertDescription>
            </Alert>
          </div>
        ) : (
          <div className="space-y-4">
            <Alert>
              <AlertDescription>
                {t("youtube.auth.connectPrompt")}
              </AlertDescription>
            </Alert>

            {authInProgress && (
              <Alert>
                <CheckCircle className="h-4 w-4" />
                <AlertDescription>
                  {t("youtube.auth.waitingForAuthorization")}
                </AlertDescription>
              </Alert>
            )}

            <Button
              onClick={handleStartAuth}
              disabled={isLoading || authInProgress}
              className="w-full"
            >
              <Youtube className="h-4 w-4 mr-2" />
              {authInProgress
                ? t("youtube.auth.connectingAutomatically")
                : t("youtube.auth.connectYouTubeAccount")}
            </Button>

            <p className="text-xs text-muted-foreground">
              {t("youtube.auth.automaticAuth")}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
