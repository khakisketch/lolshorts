import { useTranslation } from "react-i18next";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Progress } from "@/components/ui/progress";
import { Video, Clock, CheckCircle2, AlertCircle, Loader2 } from "lucide-react";
import { AutoEditProgress as AutoEditProgressType } from "@/types/autoEdit";

interface AutoEditProgressProps {
  progress: AutoEditProgressType;
}

function getStageProgress(stage: string): number {
  const stages: Record<string, number> = {
    SelectingClips: 10,
    PreparingClips: 30,
    Concatenating: 50,
    ApplyingCanvas: 70,
    MixingAudio: 90,
  };
  return stages[stage] || 0;
}

export function AutoEditProgressView({ progress }: AutoEditProgressProps) {
  const { t } = useTranslation();

  return (
    <div
      className="max-w-2xl mx-auto space-y-6"
      data-testid="progress-section"
      role="status"
      aria-live="polite"
    >
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <Loader2 className="w-5 h-5 animate-spin" />
            {t("autoEdit.generatingYourShort")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("autoEdit.pleaseWait")}
          </p>
        </div>
        <div className="space-y-6">
          {/* Progress Bar */}
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="font-medium">{progress.current_stage}</span>
              <span
                className="text-muted-foreground"
                data-testid="progress-percentage"
              >
                {Math.round(progress.progress_percentage)}%
              </span>
            </div>
            <Progress
              value={progress.progress_percentage}
              className="h-2"
              data-testid="progress-bar"
            />
          </div>

          {/* Status Details */}
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div className="flex items-center gap-2">
              <Video className="w-4 h-4 text-muted-foreground" />
              <span>
                {t("autoEdit.clipsProgress", {
                  selected: progress.clips_selected,
                  total: progress.total_clips,
                })}
              </span>
            </div>
            {progress.estimated_completion_seconds && (
              <div className="flex items-center gap-2">
                <Clock className="w-4 h-4 text-muted-foreground" />
                <span>
                  {t("autoEdit.timeRemaining", {
                    seconds: Math.ceil(progress.estimated_completion_seconds),
                  })}
                </span>
              </div>
            )}
          </div>

          {/* Stage Indicators */}
          <div className="space-y-2">
            {[
              {
                stage: "SelectingClips",
                label: t("autoEdit.stages.selectingClips"),
              },
              {
                stage: "PreparingClips",
                label: t("autoEdit.stages.preparingClips"),
              },
              {
                stage: "Concatenating",
                label: t("autoEdit.stages.concatenating"),
              },
              {
                stage: "ApplyingCanvas",
                label: t("autoEdit.stages.applyingCanvas"),
              },
              { stage: "MixingAudio", label: t("autoEdit.stages.mixingAudio") },
            ].map(({ stage, label }) => (
              <div
                key={stage}
                data-testid={`stage-${stage.toLowerCase()}`}
                className={`flex items-center gap-2 text-sm ${
                  progress.status === stage
                    ? "text-primary font-medium"
                    : progress.progress_percentage > getStageProgress(stage)
                      ? "text-green-600"
                      : "text-muted-foreground"
                }`}
              >
                {progress.status === stage ? (
                  <Loader2 className="w-4 h-4 animate-spin" />
                ) : progress.progress_percentage > getStageProgress(stage) ? (
                  <CheckCircle2 className="w-4 h-4" />
                ) : (
                  <div className="w-4 h-4 rounded-full border-2" />
                )}
                {label}
              </div>
            ))}
          </div>

          <Alert>
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>
              {t("autoEdit.generationWarning")}
            </AlertDescription>
          </Alert>
        </div>
      </div>
    </div>
  );
}
