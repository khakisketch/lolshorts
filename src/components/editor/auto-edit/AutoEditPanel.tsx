import { useState, useEffect, useCallback } from "react";
import { useSearch } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { useAutoEditStore } from "@/stores/autoEditStore";
import { useAutoEdit } from "@/hooks/useAutoEdit";
import { useStorage } from "@/hooks/useStorage";
import { useAutoEditQuota } from "@/hooks/useAutoEditQuota";
import { useAuthStore } from "@/lib/auth";
import { storageApi } from "@/api/storage";
import { Badge } from "@/components/ui/badge";
import { AutoEditQuotaBadge } from "../AutoEditQuotaBadge";
import { Sparkles } from "lucide-react";
import { GameSelection } from "@/types/autoEdit";
import { logger } from "@/lib/logger";
import { AutoEditSettings } from "./AutoEditSettings";
import { AutoEditProgressView } from "./AutoEditProgress";
import { AutoEditResult } from "./AutoEditResult";
import { AutoEditErrorView } from "./AutoEditError";
import { AutoEditPreview } from "./AutoEditPreview";

export function AutoEditPanel() {
  const { t } = useTranslation();
  const { entitlement } = useAuthStore();
  const hasProEntitlement =
    entitlement?.tier === "PRO" && entitlement.status === "active";
  const searchParams = useSearch({ from: "/auto-edit" }) as { gameId?: string };
  const [localLoading, setLocalLoading] = useState(false);

  const {
    currentStep,
    setCurrentStep,
    availableGames,
    selectedGameIds,
    targetDuration,
    currentTemplate,
    backgroundMusic,
    audioLevels,
    metadata,
    progress,
    result,
    error,
    setAvailableGames,
    toggleGameSelection,
    setTargetDuration,
    setCurrentTemplate,
    setBackgroundMusic,
    setAudioLevels,
    setMetadata,
    buildConfig,
    resetProgress,
  } = useAutoEditStore();

  const {
    startAutoEdit,
    startProgressPolling,
    stopProgressPolling,
    isLoading: hookLoading,
  } = useAutoEdit();

  const { getAllGames, isLoading: gamesLoading } = useStorage();
  const { hasQuota, fetchQuota } = useAutoEditQuota();

  useEffect(() => {
    const loadGames = async () => {
      try {
        const games = await getAllGames();
        const preSelectedGameId = searchParams.gameId;

        const gameSelectionsWithClips: GameSelection[] = await Promise.all(
          games.map(async (game) => {
            let clipCount = 0;
            try {
              const clips = await storageApi.listClips(game.game_id);
              clipCount = clips.length;
            } catch {
              logger.warn(`Failed to fetch clips for game ${game.game_id}`);
            }

            return {
              game_id: game.game_id,
              champion: game.champion,
              game_mode: game.game_mode,
              date: new Date(game.start_time).toLocaleDateString(),
              clip_count: clipCount,
              selected: preSelectedGameId === game.game_id,
            };
          }),
        );

        setAvailableGames(gameSelectionsWithClips);

        if (preSelectedGameId) {
          toggleGameSelection(preSelectedGameId);
        }
      } catch (err) {
        logger.error("Failed to load games:", err);
      }
    };

    loadGames();
  }, [
    getAllGames,
    setAvailableGames,
    searchParams.gameId,
    toggleGameSelection,
  ]);

  useEffect(() => {
    if (progress?.status === "Complete") {
      stopProgressPolling();
      setCurrentStep("complete");
    } else if (progress?.status === "Failed") {
      stopProgressPolling();
      setCurrentStep("configure");
    }
  }, [progress, stopProgressPolling, setCurrentStep]);

  const handleStartGeneration = useCallback(async () => {
    if (selectedGameIds.length === 0) {
      alert(t("errors.selectAtLeastOneGame"));
      return;
    }

    if (!hasQuota()) {
      alert(t("autoEdit.quotaExhaustedAlert"));
      return;
    }

    setLocalLoading(true);
    setCurrentStep("generating");

    try {
      const config = buildConfig();
      await startAutoEdit(config);
      fetchQuota();
      startProgressPolling(1000);
    } catch (err) {
      logger.error("Failed to start auto-edit:", err);
      setCurrentStep("configure");
    } finally {
      setLocalLoading(false);
    }
  }, [
    selectedGameIds,
    buildConfig,
    startAutoEdit,
    startProgressPolling,
    setCurrentStep,
    hasQuota,
    fetchQuota,
    t,
  ]);

  const handleRegenerate = useCallback(async () => {
    resetProgress();
    // We need to wait a tick or just call the generation logic
    // Since resetProgress is synchronous in zustand, we can call handleStartGeneration
    await handleStartGeneration();
  }, [resetProgress, handleStartGeneration]);

  const handleGoToPreview = useCallback(() => {
    if (selectedGameIds.length === 0) {
      alert(t("errors.selectAtLeastOneGame"));
      return;
    }
    setCurrentStep("preview");
  }, [selectedGameIds, setCurrentStep, t]);

  const handleStartNew = useCallback(() => {
    resetProgress();
    setCurrentStep("configure");
  }, [resetProgress, setCurrentStep]);

  const isLoading = hookLoading || localLoading || gamesLoading;

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="border-b p-4 bg-card">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Sparkles className="w-6 h-6 text-primary" />
            <div>
              <h2 className="text-lg font-semibold">{t("autoEdit.title")}</h2>
              <p className="text-sm text-muted-foreground">
                {t("autoEdit.subtitle")}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <AutoEditQuotaBadge />
            <Badge
              variant={currentStep === "generating" ? "default" : "secondary"}
            >
              {currentStep === "configure" && t("autoEdit.steps.configure")}
              {currentStep === "preview" && t("autoEdit.steps.preview")}
              {currentStep === "generating" && t("autoEdit.steps.generating")}
              {currentStep === "complete" && t("autoEdit.steps.complete")}
            </Badge>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 overflow-y-auto p-6">
        {currentStep === "configure" && (
          <AutoEditSettings
            availableGames={availableGames}
            selectedGameIds={selectedGameIds}
            targetDuration={targetDuration}
            currentTemplate={currentTemplate}
            backgroundMusic={backgroundMusic}
            audioLevels={audioLevels}
            metadata={metadata}
            isLoading={isLoading}
            gamesLoading={gamesLoading}
            isFreeTier={!hasProEntitlement}
            onToggleGame={toggleGameSelection}
            onSetDuration={setTargetDuration}
            onTemplateChange={(tpl) => setCurrentTemplate(tpl)}
            onBackgroundMusicChange={setBackgroundMusic}
            onAudioLevelsChange={setAudioLevels}
            onMetadataChange={setMetadata}
            onGenerate={handleGoToPreview}
          />
        )}

        {currentStep === "preview" && (
          <AutoEditPreview
            config={buildConfig()}
            metadata={metadata}
            availableGames={availableGames}
            onBack={() => setCurrentStep("configure")}
            onGenerate={handleStartGeneration}
          />
        )}

        {currentStep === "generating" && progress && (
          <AutoEditProgressView progress={progress} />
        )}

        {currentStep === "complete" && result && (
          <AutoEditResult
            result={result}
            onStartNew={handleStartNew}
            onRegenerate={handleRegenerate}
          />
        )}

        {error && (
          <AutoEditErrorView
            error={error}
            onRetry={handleStartGeneration}
            onStartNew={handleStartNew}
          />
        )}
      </div>
    </div>
  );
}
