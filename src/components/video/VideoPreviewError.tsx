import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { utilsApi } from "@/api/utils";
import { logger } from "@/lib/logger";

interface VideoPreviewErrorProps {
  /**
   * Absolute path to the clip on disk, used for the "open file" affordance.
   * When absent, only the retry action is offered.
   */
  filePath?: string | null;
  /** Re-attempt loading the same source. */
  onRetry: () => void;
  className?: string;
}

/**
 * Overlay shown when a preview `<video>` fails to load — the WebView has no
 * HEVC (H.265) decoder, so clips recorded with that codec render a black frame
 * with live controls and zero feedback. A missing or out-of-scope file fails
 * the same way. This gives the failure a name and an escape hatch.
 */
export function VideoPreviewError({
  filePath,
  onRetry,
  className = "",
}: VideoPreviewErrorProps) {
  const { t } = useTranslation();

  const handleOpenFile = useCallback(async () => {
    if (!filePath) return;
    try {
      await utilsApi.openFileWithDefaultApp(filePath);
    } catch (err) {
      logger.error("Failed to open clip from preview error overlay:", err);
    }
  }, [filePath]);

  return (
    <div
      role="alert"
      data-testid="video-preview-error"
      className={`absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 bg-black/85 p-6 text-center ${className}`}
    >
      <AlertTriangle className="h-8 w-8 text-amber-400" aria-hidden="true" />
      <p
        className="max-w-md text-sm text-white"
        style={{ wordBreak: "keep-all" }}
      >
        {t("video.errors.previewUnavailable")}
      </p>
      <div className="flex flex-wrap items-center justify-center gap-2">
        <Button type="button" size="sm" variant="outline" onClick={onRetry}>
          {t("common.retry")}
        </Button>
        {filePath ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => void handleOpenFile()}
          >
            {t("video.errors.openInSystemPlayer")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}

/**
 * Local error state for a preview `<video>`. `handleError` is wired to the
 * element's `onError`; `clearError` is called on retry and whenever the source
 * changes so a previously failed clip does not keep the overlay pinned.
 */
export function useVideoPreviewError() {
  const [hasError, setHasError] = useState(false);
  const handleError = useCallback(() => setHasError(true), []);
  const clearError = useCallback(() => setHasError(false), []);
  return { hasError, handleError, clearError };
}
