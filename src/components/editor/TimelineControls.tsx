import { useTranslation } from "react-i18next";
import { useEditorStore } from "@/stores/editorStore";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { useConfirmDialog } from "@/components/ui/confirm-dialog";
import { formatDuration } from "@/lib/utils";
import { ZoomIn, ZoomOut, RotateCcw, Trash2 } from "lucide-react";

export function TimelineControls() {
  const { t } = useTranslation();
  const {
    timelineClips,
    totalDuration,
    zoom,
    zoomIn,
    zoomOut,
    resetZoom,
    clearTimeline,
  } = useEditorStore();
  const { confirm, ConfirmDialog } = useConfirmDialog();

  const handleClearTimeline = async () => {
    const confirmed = await confirm({
      title: t("confirmations.clearTimelineTitle"),
      description: t("confirmations.clearTimelineDescription"),
      confirmText: t("common.clear"),
      cancelText: t("common.cancel"),
      variant: "warning",
    });

    if (confirmed) {
      clearTimeline();
    }
  };

  return (
    <div className="flex items-center justify-between">
      {/* Left: Timeline Info */}
      <div className="flex items-center gap-3">
        <h3 className="font-semibold">{t("editor.timeline.title")}</h3>
        <Badge variant="secondary">
          {timelineClips.length} {t("editor.timeline.clips")}
        </Badge>
        <Badge variant="outline">{formatDuration(totalDuration)}</Badge>
      </div>

      {/* Right: Controls */}
      <div className="flex items-center gap-2">
        {/* Zoom Controls */}
        <div className="flex items-center gap-1 border rounded-lg p-1">
          <Button
            size="sm"
            variant="ghost"
            onClick={zoomOut}
            disabled={zoom <= 0.5}
            className="h-7 px-2"
            aria-label={t("editor.timeline.zoomOut")}
          >
            <ZoomOut className="w-4 h-4" />
          </Button>

          <span className="text-xs font-medium min-w-[3rem] text-center">
            {Math.round(zoom * 100)}%
          </span>

          <Button
            size="sm"
            variant="ghost"
            onClick={zoomIn}
            disabled={zoom >= 4.0}
            className="h-7 px-2"
            aria-label={t("editor.timeline.zoomIn")}
          >
            <ZoomIn className="w-4 h-4" />
          </Button>

          <Button
            size="sm"
            variant="ghost"
            onClick={resetZoom}
            disabled={zoom === 1.0}
            className="h-7 px-2"
            aria-label={t("editor.timeline.resetZoom")}
          >
            <RotateCcw className="w-4 h-4" />
          </Button>
        </div>

        {/* Clear Timeline */}
        <Button
          size="sm"
          variant="outline"
          onClick={handleClearTimeline}
          disabled={timelineClips.length === 0}
        >
          <Trash2 className="w-4 h-4 mr-1" />
          {t("editor.timeline.clear")}
        </Button>
      </div>

      <ConfirmDialog />
    </div>
  );
}
