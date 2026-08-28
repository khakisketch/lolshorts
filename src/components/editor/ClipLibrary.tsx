import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useEditorStore } from "@/stores/editorStore";
import { ClipCard } from "./ClipCard";
import { rankClips } from "@/lib/clipRanking";
import { Film } from "lucide-react";

export function ClipLibrary() {
  const { t } = useTranslation();
  const { availableClips } = useEditorStore();

  // 홈과 같은 순서 — 점수 높은 것부터. 홈에서 1위로 본 클립이 편집기에서
  // 목록 한복판에 있으면 같은 판을 두 번 훑게 된다.
  const clips = useMemo(() => rankClips(availableClips), [availableClips]);

  if (availableClips.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center p-6 text-center">
        <Film className="w-16 h-16 text-muted-foreground mb-4" />
        <h3 className="text-lg font-semibold mb-2">
          {t("editor.clipLibrary.noClipsAvailable")}
        </h3>
        <p className="text-sm text-muted-foreground">
          {t("editor.clipLibrary.noClipsDescription")}
        </p>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="p-4 border-b">
        <h3 className="font-semibold">{t("editor.clipLibrary.title")}</h3>
        <p className="text-sm text-muted-foreground">
          {t("editor.clipLibrary.clipsCount", { count: availableClips.length })}
        </p>
        <p className="mt-1 text-xs text-gaming-cyan/80">
          {t("editor.clipLibrary.rankedDescription")}
        </p>
      </div>

      {/* Clip Grid */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="grid grid-cols-1 gap-4">
          {clips.map((clip, index) => (
            <ClipCard key={clip.file_path} clip={clip} top={index === 0} />
          ))}
        </div>
      </div>
    </div>
  );
}
