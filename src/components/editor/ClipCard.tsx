import { useTranslation } from "react-i18next";
import { ClipMetadata } from "@/types/storage";
import { Button } from "@/components/ui/button";
import { Play, Plus } from "lucide-react";
import { useEditorStore } from "@/stores/editorStore";
import { convertFileSrc } from "@tauri-apps/api/core";
import { formatDuration } from "@/lib/utils";
import { clipLabel } from "@/lib/clipLabel";

interface ClipCardProps {
  clip: ClipMetadata;
  top?: boolean;
}

/**
 * 편집기 클립 보관함의 카드.
 *
 * 홈의 `components/clips/ClipCard` 와 **합치지 않는다** — 이쪽은 미리보기·추가
 * 버튼을 단 카드고 저쪽은 고르는 카드다. 다만 **라벨은 같은 곳에서 온다**:
 * 예전에는 여기가 파일 이름(`merged_1703_1703.mp4`)을 제목으로, `priority`
 * 숫자(`우선순위 3`)를 배지로 내보냈다. 사용자에게 아무 뜻이 없는 값이고, 같은
 * 클립이 화면마다 다른 이름으로 보였다.
 */
export function ClipCard({ clip, top = false }: ClipCardProps) {
  const { t } = useTranslation();
  const { addToTimeline, setSelectedClipId } = useEditorStore();
  const { title, reasons } = clipLabel(clip);

  const handleAddToTimeline = () => {
    addToTimeline(clip);
  };

  const handlePreview = () => {
    setSelectedClipId(clip.file_path);
  };

  // Convert file path for Tauri
  const thumbnailSrc = clip.thumbnail_path
    ? convertFileSrc(clip.thumbnail_path)
    : undefined;

  return (
    <div className="bg-black/40 rounded-lg border border-white/5 overflow-hidden hover:border-primary transition-colors cursor-pointer group">
      {/* Thumbnail */}
      <div className="relative aspect-video bg-black">
        {thumbnailSrc ? (
          <img
            src={thumbnailSrc}
            alt="Clip thumbnail"
            className="w-full h-full object-contain"
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center">
            <Play className="w-12 h-12 text-muted-foreground" />
          </div>
        )}

        {/* Duration overlay */}
        <div className="absolute bottom-2 right-2 bg-black/80 text-white text-xs px-2 py-1 rounded">
          {formatDuration(clip.duration)}
        </div>
      </div>

      {/* Clip Info */}
      <div className="p-3 space-y-2">
        <div>
          <div className="flex items-center justify-between gap-2">
            <p
              className="min-w-0 truncate text-sm font-medium"
              style={{ wordBreak: "keep-all" }}
            >
              {t(title.key, title.params)}
            </p>
            {top && (
              <span className="shrink-0 text-[10px] font-bold uppercase tracking-wider text-gaming-cyan">
                {t("home.clips.topMoment")}
              </span>
            )}
          </div>
          {reasons.length > 0 && (
            <p
              className="mt-0.5 truncate text-xs text-gaming-cyan/80"
              style={{ wordBreak: "keep-all" }}
              data-testid={`editor-clip-reasons-${clip.file_path}`}
            >
              {reasons.map((r) => t(r.key, r.params)).join(" · ")}
            </p>
          )}
        </div>

        {/* Actions */}
        <div className="flex gap-2">
          <Button
            size="sm"
            variant="outline"
            className="flex-1"
            onClick={handlePreview}
          >
            <Play className="w-3 h-3 mr-1" />
            {t("editor.clip.preview")}
          </Button>
          <Button
            size="sm"
            variant="default"
            className="flex-1"
            onClick={handleAddToTimeline}
          >
            <Plus className="w-3 h-3 mr-1" />
            {t("editor.clip.add")}
          </Button>
        </div>
      </div>
    </div>
  );
}
