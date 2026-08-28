import React, { Component, ErrorInfo, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { AlertTriangle, RefreshCw } from "lucide-react";
import { logger } from "@/lib/logger";

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
}

/**
 * Error Boundary Component
 *
 * Catches JavaScript errors in child component tree,
 * logs error information, and displays fallback UI
 *
 * Security: Does not expose sensitive error details in production
 * Accessibility: Provides clear error messaging and recovery options
 */
export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  private retryCount = 0;
  private maxRetries = 3;

  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = {
      hasError: false,
      error: null,
      errorInfo: null,
    };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return {
      hasError: true,
      error,
      errorInfo: null,
    };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    this.setState({ error, errorInfo });
    logger.error("Error Boundary caught an error:", error, errorInfo);

    if (this.props.onError) {
      this.props.onError(error, errorInfo);
    }
  }

  handleRetry = () => {
    if (this.retryCount < this.maxRetries) {
      this.retryCount++;
      this.setState({ hasError: false, error: null, errorInfo: null });
    }
  };

  handleReload = () => {
    window.location.reload();
  };

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return <>{this.props.fallback}</>;
      }

      return (
        <ErrorFallbackUI
          error={this.state.error}
          errorInfo={this.state.errorInfo}
          retryCount={this.retryCount}
          maxRetries={this.maxRetries}
          onRetry={this.handleRetry}
          onReload={this.handleReload}
        />
      );
    }

    return this.props.children;
  }
}

interface ErrorFallbackUIProps {
  error: Error | null;
  errorInfo: ErrorInfo | null;
  retryCount: number;
  maxRetries: number;
  onRetry: () => void;
  onReload: () => void;
}

function ErrorFallbackUI({
  error,
  errorInfo,
  retryCount,
  maxRetries,
  onRetry,
  onReload,
}: ErrorFallbackUIProps) {
  const { t } = useTranslation();

  return (
    <div className="min-h-screen flex items-center justify-center p-4 bg-background">
      <div className="gaming-panel p-6 w-full max-w-md border-destructive">
        <div className="mb-4 text-center">
          <div className="mx-auto w-12 h-12 bg-destructive/10 rounded-full flex items-center justify-center mb-4">
            <AlertTriangle className="h-6 w-6 text-destructive" />
          </div>
          <h3 className="text-lg font-semibold text-destructive">
            {t("errorBoundary.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("errorBoundary.description")}
          </p>
        </div>
        <div className="space-y-4">
          {process.env.NODE_ENV !== "production" && error && (
            <div className="bg-muted p-3 rounded-lg text-xs overflow-auto max-h-32">
              <p className="font-mono text-destructive mb-2">
                {error.name}: {error.message}
              </p>
              {errorInfo && (
                <p className="text-muted-foreground whitespace-pre-wrap">
                  {errorInfo.componentStack}
                </p>
              )}
            </div>
          )}

          <div className="space-y-2">
            {retryCount < maxRetries && (
              <Button
                onClick={onRetry}
                className="w-full"
                variant="default"
                aria-label={t("errorBoundary.retry")}
              >
                <RefreshCw className="h-4 w-4 mr-2" />
                {t("errorBoundary.retry")}{" "}
                {t("errorBoundary.retryCount", {
                  count: maxRetries - retryCount,
                })}
              </Button>
            )}

            <Button
              onClick={onReload}
              className="w-full"
              variant="outline"
              aria-label={t("errorBoundary.reload")}
            >
              {t("errorBoundary.reload")}
            </Button>
          </div>

          <p className="text-xs text-muted-foreground text-center">
            {t("errorBoundary.contactSupport")}
          </p>
        </div>
      </div>
    </div>
  );
}

// Hook for functional components
export function useErrorHandler() {
  const handleError = React.useCallback((error: Error, errorInfo?: string) => {
    logger.error("Caught error:", error, errorInfo);
  }, []);

  return { handleError };
}

// Specialized Error Boundaries for different contexts

export function VideoErrorBoundary({ children }: { children: ReactNode }) {
  const handleError = React.useCallback((error: Error) => {
    logger.error("Video playback error:", error);
  }, []);

  return (
    <ErrorBoundary onError={handleError} fallback={<VideoErrorFallback />}>
      {children}
    </ErrorBoundary>
  );
}

function VideoErrorFallback() {
  const { t } = useTranslation();
  return (
    <div className="gaming-panel p-6 m-4">
      <div className="p-6 text-center">
        <AlertTriangle className="h-8 w-8 text-muted-foreground mx-auto mb-2" />
        <h3 className="font-medium mb-2">{t("errorBoundary.videoError")}</h3>
        <p className="text-sm text-muted-foreground mb-4">
          {t("errorBoundary.videoErrorDesc")}
        </p>
        <Button size="sm" onClick={() => window.location.reload()}>
          {t("errorBoundary.reload")}
        </Button>
      </div>
    </div>
  );
}

export function FormErrorBoundary({ children }: { children: ReactNode }) {
  const handleError = React.useCallback((error: Error) => {
    logger.error("Form error:", error);
  }, []);

  return (
    <ErrorBoundary onError={handleError} fallback={<FormErrorFallback />}>
      {children}
    </ErrorBoundary>
  );
}

function FormErrorFallback() {
  const { t } = useTranslation();
  return (
    <div className="gaming-panel p-6 m-4 border-destructive">
      <div className="p-4">
        <div className="flex items-center gap-2 text-destructive">
          <AlertTriangle className="h-4 w-4" />
          <span className="text-sm font-medium">
            {t("errorBoundary.formError")}
          </span>
        </div>
        <p className="text-xs text-muted-foreground mt-1">
          {t("errorBoundary.formErrorDesc")}
        </p>
      </div>
    </div>
  );
}
