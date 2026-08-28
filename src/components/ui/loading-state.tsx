import { cn } from "@/lib/utils";
import { Spinner, SpinnerCenter } from "./spinner";
import { Skeleton } from "./skeleton";
import { ReactNode } from "react";

interface LoadingStateProps {
  isLoading: boolean;
  children: ReactNode;
  /**
   * What to show while loading:
   * - "spinner": Show centered spinner
   * - "skeleton": Show skeleton placeholder
   * - "overlay": Show content with spinner overlay
   * - "inline": Show inline spinner before content
   * - ReactNode: Custom loading content
   */
  loadingContent?: "spinner" | "skeleton" | "overlay" | "inline" | ReactNode;
  /**
   * Custom skeleton to show when loadingContent is "skeleton"
   */
  skeleton?: ReactNode;
  /**
   * Loading message
   */
  loadingLabel?: string;
  /**
   * Additional class for the wrapper
   */
  className?: string;
  /**
   * Minimum height when loading (for layout stability)
   */
  minHeight?: string;
}

export function LoadingState({
  isLoading,
  children,
  loadingContent = "spinner",
  skeleton,
  loadingLabel,
  className,
  minHeight = "h-32",
}: LoadingStateProps) {
  if (!isLoading) {
    return <>{children}</>;
  }

  // Custom loading content
  if (typeof loadingContent !== "string") {
    return <>{loadingContent}</>;
  }

  switch (loadingContent) {
    case "spinner":
      return (
        <div
          className={cn(
            "flex items-center justify-center",
            minHeight,
            className,
          )}
        >
          <SpinnerCenter label={loadingLabel} />
        </div>
      );

    case "skeleton":
      return (
        <div className={className}>
          {skeleton || (
            <div className="space-y-4">
              <Skeleton className="h-8 w-1/3" />
              <Skeleton className="h-24 w-full" />
              <Skeleton className="h-24 w-full" />
            </div>
          )}
        </div>
      );

    case "overlay":
      return (
        <div className={cn("relative", className)}>
          <div className="opacity-50 pointer-events-none">{children}</div>
          <div className="absolute inset-0 flex items-center justify-center bg-background/50">
            <Spinner size="lg" label={loadingLabel} />
          </div>
        </div>
      );

    case "inline":
      return (
        <div className={cn("flex items-center gap-2", className)}>
          <Spinner size="sm" />
          <span className="text-muted-foreground">
            {loadingLabel || "로딩 중..."}
          </span>
        </div>
      );

    default:
      return (
        <div
          className={cn(
            "flex items-center justify-center",
            minHeight,
            className,
          )}
        >
          <SpinnerCenter label={loadingLabel} />
        </div>
      );
  }
}

// Full-page loading overlay
interface LoadingOverlayProps {
  isLoading: boolean;
  label?: string;
  blur?: boolean;
  className?: string;
}

export function LoadingOverlay({
  isLoading,
  label,
  blur = true,
  className,
}: LoadingOverlayProps) {
  if (!isLoading) return null;

  return (
    <div
      className={cn(
        "fixed inset-0 z-50 flex items-center justify-center",
        "bg-background/80",
        blur && "backdrop-blur-sm",
        className,
      )}
    >
      <div className="flex flex-col items-center gap-4">
        <Spinner size="xl" />
        {label && (
          <p className="text-lg font-medium text-muted-foreground">{label}</p>
        )}
      </div>
    </div>
  );
}

// Button loading state helper
interface ButtonLoadingProps {
  isLoading: boolean;
  children: ReactNode;
  loadingText?: string;
}

export function ButtonLoading({
  isLoading,
  children,
  loadingText,
}: ButtonLoadingProps) {
  if (isLoading) {
    return (
      <>
        <Spinner size="sm" className="mr-2" />
        {loadingText || children}
      </>
    );
  }
  return <>{children}</>;
}
