import { useEffect, useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useYouTube } from "@/hooks/useYouTube";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { History, ExternalLink, Eye } from "lucide-react";
import { UploadHistoryEntry } from "@/types/youtube";
import { open } from "@tauri-apps/plugin-shell";
import { logger } from "@/lib/logger";

export function YouTubeHistory() {
  const { t } = useTranslation();
  const { authStatus, isLoading, error, getUploadHistory } = useYouTube();
  const [history, setHistory] = useState<UploadHistoryEntry[]>([]);

  const loadHistory = useCallback(async () => {
    try {
      const data = await getUploadHistory();
      setHistory(data);
    } catch (err) {
      logger.error("Failed to load history:", err);
    }
  }, [getUploadHistory]);

  useEffect(() => {
    if (authStatus.authenticated) {
      loadHistory();
    }
  }, [authStatus.authenticated, loadHistory]);

  const formatDate = (timestamp: number) => {
    const date = new Date(timestamp * 1000);
    return date.toLocaleDateString() + " " + date.toLocaleTimeString();
  };

  const formatNumber = (num: number | null): string => {
    if (num === null) return "0";
    if (num >= 1_000_000) {
      return (num / 1_000_000).toFixed(1) + "M";
    } else if (num >= 1_000) {
      return (num / 1_000).toFixed(1) + "K";
    }
    return num.toString();
  };

  const handleOpenVideo = (videoId: string) => {
    open(`https://www.youtube.com/watch?v=${videoId}`);
  };

  if (!authStatus.authenticated) {
    return (
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("youtube.history.uploadHistory")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("youtube.history.connectAccountFirst")}
          </p>
        </div>
        <div>
          <Alert>
            <AlertDescription>
              {t("youtube.history.needToConnect")}
            </AlertDescription>
          </Alert>
        </div>
      </div>
    );
  }

  return (
    <div className="gaming-panel p-6">
      <div className="mb-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <History className="h-6 w-6" />
            <div>
              <h3 className="text-lg font-semibold">
                {t("youtube.history.uploadHistory")}
              </h3>
              <p className="text-sm text-muted-foreground">
                {t("youtube.history.recentVideos")}
              </p>
            </div>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={loadHistory}
            disabled={isLoading}
          >
            {t("youtube.history.refresh")}
          </Button>
        </div>
      </div>

      <div className="space-y-4">
        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        {isLoading ? (
          <div className="text-center py-8 text-muted-foreground">
            {t("youtube.history.loadingHistory")}
          </div>
        ) : history.length === 0 ? (
          <EmptyState
            title={t("youtube.history.noUploads")}
            description={t(
              "youtube.history.uploadFirstVideo",
              "You haven't uploaded any videos yet. Start creating and sharing your highlights!",
            )}
            className="py-12"
          />
        ) : (
          <div className="space-y-3">
            {history.map((entry) => (
              <div
                key={entry.video_id}
                className="flex items-start gap-4 p-4 border rounded-lg hover:bg-muted/50 transition-colors"
              >
                {/* Thumbnail */}
                <div className="flex-shrink-0">
                  {entry.thumbnail_url ? (
                    <img
                      src={entry.thumbnail_url}
                      alt={entry.title}
                      className="w-32 h-18 object-cover rounded"
                    />
                  ) : (
                    <div className="w-32 h-18 bg-muted rounded flex items-center justify-center">
                      <History className="h-8 w-8 opacity-30" />
                    </div>
                  )}
                </div>

                {/* Video Info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex-1 min-w-0">
                      <h3 className="font-medium truncate">{entry.title}</h3>
                    </div>
                    <Badge variant="secondary" className="flex-shrink-0">
                      {entry.privacy_status}
                    </Badge>
                  </div>

                  <div className="flex items-center gap-4 mt-3 text-xs text-muted-foreground">
                    <span className="flex items-center gap-1">
                      <Eye className="h-3 w-3" />
                      {formatNumber(entry.view_count)}
                    </span>
                    <span className="ml-auto">
                      {t("youtube.history.uploaded")}:{" "}
                      {formatDate(entry.uploaded_at)}
                    </span>
                  </div>

                  <div className="flex items-center gap-2 mt-3">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => handleOpenVideo(entry.video_id)}
                    >
                      <ExternalLink className="h-3 w-3 mr-1" />
                      {t("youtube.history.viewOnYouTube")}
                    </Button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
