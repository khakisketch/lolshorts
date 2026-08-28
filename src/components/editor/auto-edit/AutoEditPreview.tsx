import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  CheckCircle2,
  Info,
  Sparkles,
  ArrowLeft,
  PlayCircle,
} from "lucide-react";
import {
  AutoEditConfig,
  AutoEditMetadata,
  GameSelection,
} from "@/types/autoEdit";

interface AutoEditPreviewProps {
  config: AutoEditConfig;
  metadata: AutoEditMetadata;
  availableGames: GameSelection[];
  onBack: () => void;
  onGenerate: () => void;
}

export function AutoEditPreview({
  config,
  metadata,
  availableGames,
  onBack,
  onGenerate,
}: AutoEditPreviewProps) {
  const { t } = useTranslation();

  const selectedGames = availableGames.filter((g) =>
    config.game_ids.includes(g.game_id),
  );

  return (
    <div className="max-w-2xl mx-auto space-y-6">
      <div className="gaming-panel p-6">
        <div className="mb-6">
          <h3 className="text-xl font-bold flex items-center gap-2">
            <Sparkles className="w-6 h-6 text-primary" />
            {t("autoEdit.previewTitle", "Review Your Short")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t(
              "autoEdit.previewSubtitle",
              "Review your configuration and planning metadata before generation.",
            )}
          </p>
        </div>

        <div className="space-y-6">
          {/* Configuration Summary */}
          <div className="grid grid-cols-2 gap-4">
            <div className="p-3 bg-black/40 rounded-lg border border-white/5">
              <div className="text-xs text-muted-foreground mb-1">
                {t("autoEdit.duration", "Duration")}
              </div>
              <div className="font-medium">{config.target_duration}s</div>
            </div>
            <div className="p-3 bg-black/40 rounded-lg border border-white/5">
              <div className="text-xs text-muted-foreground mb-1">
                {t("autoEdit.clips", "Estimated Clips")}
              </div>
              <div className="font-medium">
                ~{Math.ceil(config.target_duration / 5)} clips
              </div>
            </div>
          </div>

          {/* Selected Games */}
          <div className="space-y-2">
            <div className="text-sm font-medium flex items-center gap-2">
              <CheckCircle2 className="w-4 h-4 text-primary" />
              {t("autoEdit.selectedGames", "Selected Games")}
            </div>
            <div className="flex flex-wrap gap-2">
              {selectedGames.map((g) => (
                <Badge key={g.game_id} variant="secondary" className="text-xs">
                  {g.champion} ({g.clip_count} clips)
                </Badge>
              ))}
            </div>
          </div>

          {/* Metadata Summary */}
          <div className="space-y-3 p-4 bg-muted/30 rounded-lg border border-white/5">
            <div className="text-sm font-medium flex items-center gap-2">
              <Info className="w-4 h-4 text-primary" />
              {t("autoEdit.metadataSummary", "Shorts Planning")}
            </div>
            <div className="space-y-2 text-sm">
              <div className="flex justify-between">
                <span className="text-muted-foreground">
                  {t("autoEdit.metadataTitleLabel", "Title")}
                </span>
                <span className="font-medium">
                  {metadata.title ||
                    t("autoEdit.notSpecified", "Not specified")}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">
                  {t("autoEdit.metadataCaptionLabel", "Caption")}
                </span>
                <span className="font-medium truncate max-w-[200px] text-right">
                  {metadata.caption ||
                    t("autoEdit.notSpecified", "Not specified")}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">
                  {t("autoEdit.metadataTagsLabel", "Tags")}
                </span>
                <div className="flex gap-1">
                  {metadata.tags.length > 0 ? (
                    metadata.tags.map((tag) => (
                      <span
                        key={tag}
                        className="text-[10px] bg-primary/20 text-primary px-1 rounded"
                      >
                        #{tag}
                      </span>
                    ))
                  ) : (
                    <span className="text-muted-foreground">
                      {t("autoEdit.notSpecified", "Not specified")}
                    </span>
                  )}
                </div>
              </div>
            </div>
          </div>

          {/* Final Warning */}
          <div className="space-y-3">
            <div className="p-3 bg-blue-500/10 border border-blue-500/20 rounded-lg">
              <div className="flex gap-2 text-xs text-blue-600">
                <Info className="w-4 h-4 flex-shrink-0" />
                <p>
                  {t(
                    "autoEdit.previewNote",
                    "Your reviewed clip order and trim ranges will be used. Go back to the storyboard if you want to change them before rendering.",
                  )}
                </p>
              </div>
            </div>
            <div className="p-3 bg-yellow-500/10 border border-yellow-500/20 rounded-lg">
              <div className="flex gap-2 text-xs text-yellow-600">
                <Info className="w-4 h-4 flex-shrink-0" />
                <p>
                  {t(
                    "autoEdit.previewWarning",
                    "Generation will use your quota. You can cancel while the render is running, and a recoverable job will be offered after an unexpected interruption.",
                  )}
                </p>
              </div>
            </div>
          </div>
        </div>

        <div className="flex gap-3 mt-8">
          <Button variant="outline" onClick={onBack} className="flex-1">
            <ArrowLeft className="w-4 h-4 mr-2" />
            {t("autoEdit.backToSettings", "Back to Settings")}
          </Button>
          <Button onClick={onGenerate} className="flex-1">
            <PlayCircle className="w-4 h-4 mr-2" />
            {t("autoEdit.confirmGenerate", "Confirm & Generate")}
          </Button>
        </div>
      </div>
    </div>
  );
}
