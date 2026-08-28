import { useState, useEffect, useCallback } from "react";
import { useSearch } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { useAutoEditStore, pinnedPathsFor } from "@/stores/autoEditStore";
import { useAutoEdit } from "@/hooks/useAutoEdit";
import { useStorage } from "@/hooks/useStorage";
import { useAutoEditQuota } from "@/hooks/useAutoEditQuota";
import { useAuthStore } from "@/lib/auth";
import { storageApi } from "@/api/storage";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { AutoEditQuotaBadge } from "../AutoEditQuotaBadge";
import { Sparkles } from "lucide-react";
import { GameSelection } from "@/types/autoEdit";
import { logger } from "@/lib/logger";
import { AutoEditSettings } from "./AutoEditSettings";
import { AutoEditProgressView } from "./AutoEditProgress";
import { AutoEditResult } from "./AutoEditResult";
import { AutoEditErrorView } from "./AutoEditError";
import { AutoEditPreview } from "./AutoEditPreview";
import { formatDuration } from "@/lib/utils";
import { AutoEditStoryboard } from "./AutoEditStoryboard";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { AutoEditConfig, MediaJobSnapshot } from "@/types/autoEdit";
import { videoApi } from "@/api/video";

export function AutoEditPanel() {
  const { t } = useTranslation();
  const { entitlement } = useAuthStore();
  const hasProEntitlement =
    entitlement?.tier === "PRO" && entitlement.status === "active";
  const searchParams = useSearch({ from: "/auto-edit" }) as { gameId?: string };
  const [localLoading, setLocalLoading] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [recoverableJobs, setRecoverableJobs] = useState<MediaJobSnapshot[]>(
    [],
  );
  const [clipDurations, setClipDurations] = useState<Map<string, number>>(
    () => new Map(),
  );

  const {
    currentStep,
    setCurrentStep,
    availableGames,
    selectedGameIds,
    pinnedClips,
    targetDuration,
    enableEventZoom,
    enableHookCaptions,
    currentTemplate,
    backgroundMusic,
    audioLevels,
    metadata,
    storyboard,
    storyboardPast,
    storyboardFuture,
    outputIntent,
    framingMode,
    platformPreset,
    progress,
    result,
    error,
    setAvailableGames,
    toggleGameSelection,
    setSelectedGameIds,
    setPinnedClips,
    setTargetDuration,
    setEnableEventZoom,
    setEnableHookCaptions,
    setCurrentTemplate,
    setBackgroundMusic,
    setAudioLevels,
    setMetadata,
    setStoryboard,
    moveStoryboardClip,
    updateStoryboardTrim,
    removeStoryboardClip,
    clearStoryboard,
    resetStoryboardToRecommendation,
    undoStoryboard,
    redoStoryboard,
    setOutputIntent,
    setFramingMode,
    setPlatformPreset,
    buildConfig,
    resetProgress,
  } = useAutoEditStore();

  const {
    startAutoEdit,
    planAutoEdit,
    cancelAutoEdit,
    resumeMediaJob,
    stopProgressPolling,
    isLoading: hookLoading,
  } = useAutoEdit();

  useEffect(() => {
    let mounted = true;
    void videoApi
      .listRecoverableMediaJobs()
      .then((jobs) => {
        if (mounted)
          setRecoverableJobs(jobs.filter((job) => job.kind === "auto_edit"));
      })
      .catch(() => {
        if (mounted) setRecoverableJobs([]);
      });
    return () => {
      mounted = false;
    };
  }, []);

  const handleResumeJob = useCallback(
    async (job: MediaJobSnapshot) => {
      try {
        const saved = JSON.parse(job.config_json) as AutoEditConfig;
        setSelectedGameIds(saved.game_ids ?? []);
        if (
          saved.target_duration === 60 ||
          saved.target_duration === 120 ||
          saved.target_duration === 180
        ) {
          setTargetDuration(saved.target_duration);
        }
        setOutputIntent(saved.output_intent ?? "single_short");
        setFramingMode(saved.framing_mode ?? "lol_focus_stack");
        setPlatformPreset(saved.platform_preset ?? "youtube_shorts");
        if (saved.publish_metadata) {
          setMetadata({
            title: saved.publish_metadata.title,
            caption: saved.publish_metadata.description,
            tags: saved.publish_metadata.tags,
          });
        }
        if (saved.storyboard) {
          setStoryboard(
            saved.storyboard.map((clip, index) => ({
              ...clip,
              source_duration_secs: clip.trim_end_secs,
              event_type: "",
              highlight_score: 0,
              recommended_order: index,
            })),
          );
        }
        setCurrentStep("generating");
        await resumeMediaJob(job.job_id);
        setRecoverableJobs((jobs) =>
          jobs.filter((item) => item.job_id !== job.job_id),
        );
      } catch (resumeError) {
        logger.error("Failed to resume media job:", resumeError);
        setCurrentStep("storyboard");
      }
    },
    [
      resumeMediaJob,
      setCurrentStep,
      setFramingMode,
      setMetadata,
      setOutputIntent,
      setPlatformPreset,
      setSelectedGameIds,
      setStoryboard,
      setTargetDuration,
    ],
  );

  const handleDiscardJob = useCallback(
    async (job: MediaJobSnapshot) => {
      if (!window.confirm(t("mediaJobs.discard"))) return;
      await videoApi.discardMediaJob(job.job_id);
      setRecoverableJobs((jobs) =>
        jobs.filter((item) => item.job_id !== job.job_id),
      );
    },
    [t],
  );

  // 홈에서 고른 클립이 이번 요청에 실제로 걸리는지 — `buildConfig` 와 같은
  // 함수로 판정한다. 안내와 실제 설정이 어긋나면 그게 또 하나의 거짓말이다.
  const pinnedPaths = pinnedPathsFor(pinnedClips, selectedGameIds);
  const pinnedDuration = (pinnedPaths ?? []).reduce(
    (total, path) => total + (clipDurations.get(path) ?? 0),
    0,
  );
  const pinnedClipCount = pinnedPaths?.length ?? 0;
  const pinnedGameCount = pinnedPaths ? (pinnedClips?.groups.length ?? 0) : 0;

  const { getAllGames, isLoading: gamesLoading } = useStorage();
  const { hasQuota, fetchQuota } = useAutoEditQuota();

  useEffect(() => {
    const loadGames = async () => {
      try {
        const games = await getAllGames();
        const preSelectedGameId = searchParams.gameId;
        const durations = new Map<string, number>();

        const gameSelectionsWithClips: GameSelection[] = await Promise.all(
          games.map(async (game) => {
            let clipCount = 0;
            try {
              const clips = await storageApi.listClips(game.game_id);
              clipCount = clips.length;
              for (const clip of clips) {
                durations.set(clip.file_path, Math.max(0, clip.duration));
              }
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
        setClipDurations(durations);

        if (preSelectedGameId) {
          // **토글이 아니라 대입**이다. 토글이면 이 효과가 한 번 더 도는 순간
          // (StrictMode 의 이중 호출, `getAllGames` 정체성 변화) 선택이 도로
          // 풀려서 "홈에서 만들기를 눌렀는데 게임이 안 골라져 있다" 가 된다.
          setPinnedClips(null);
          setSelectedGameIds([preSelectedGameId]);
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
    setSelectedGameIds,
    setPinnedClips,
  ]);

  useEffect(() => {
    if (progress?.status === "Complete") {
      stopProgressPolling();
      fetchQuota();
      setCurrentStep("complete");
    } else if (progress?.status === "Failed") {
      stopProgressPolling();
    } else if (progress?.status === "Cancelled") {
      stopProgressPolling();
      setCurrentStep("storyboard");
    }
  }, [progress, stopProgressPolling, setCurrentStep, fetchQuota]);

  const handleStartGeneration = useCallback(async () => {
    if (selectedGameIds.length === 0) {
      alert(t("errors.selectAtLeastOneGame"));
      return;
    }

    if (!hasQuota()) {
      alert(t("autoEdit.quotaExhaustedAlert"));
      return;
    }

    const storyboardDuration = storyboard.reduce(
      (sum, clip) => sum + clip.trim_end_secs - clip.trim_start_secs,
      0,
    );
    if (storyboardDuration > 180 && outputIntent === "single_short") {
      setOutputIntent("shorts_series");
    }

    setLocalLoading(true);
    setCurrentStep("generating");

    try {
      const config = buildConfig();
      if (storyboardDuration > 180 && config.output_intent === "single_short") {
        config.output_intent = "shorts_series";
      }
      await startAutoEdit(config);
    } catch (err) {
      logger.error("Failed to start auto-edit:", err);
      setCurrentStep("configure");
    } finally {
      setLocalLoading(false);
    }
  }, [
    selectedGameIds,
    storyboard,
    outputIntent,
    setOutputIntent,
    buildConfig,
    startAutoEdit,
    setCurrentStep,
    hasQuota,
    t,
  ]);

  const handleRegenerate = useCallback(async () => {
    resetProgress();
    setCurrentStep("storyboard");
    await handleStartGeneration();
  }, [resetProgress, setCurrentStep, handleStartGeneration]);

  const handleReviewClips = useCallback(async () => {
    if (selectedGameIds.length === 0) {
      alert(t("errors.selectAtLeastOneGame"));
      return;
    }
    setLocalLoading(true);
    try {
      clearStoryboard();
      const plan = await planAutoEdit(
        useAutoEditStore.getState().buildConfig(),
      );
      setStoryboard(plan.clips);
      setOutputIntent(plan.recommended_output_intent);
      setCurrentStep("storyboard");
    } catch (err) {
      logger.error("Failed to plan auto-edit:", err);
    } finally {
      setLocalLoading(false);
    }
  }, [
    selectedGameIds,
    t,
    clearStoryboard,
    planAutoEdit,
    setStoryboard,
    setOutputIntent,
    setCurrentStep,
  ]);

  const handleResetRecommendation = useCallback(() => {
    resetStoryboardToRecommendation();
  }, [resetStoryboardToRecommendation]);

  const handleStartNew = useCallback(() => {
    resetProgress();
    clearStoryboard();
    setCurrentStep("configure");
  }, [resetProgress, clearStoryboard, setCurrentStep]);

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
              {currentStep === "storyboard" &&
                t("autoEdit.storyboard.title", "Storyboard")}
              {currentStep === "preview" && t("autoEdit.steps.preview")}
              {currentStep === "generating" && t("autoEdit.steps.generating")}
              {currentStep === "complete" && t("autoEdit.steps.complete")}
            </Badge>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 overflow-y-auto p-6">
        {recoverableJobs.map((job) => (
          <Alert
            key={job.job_id}
            className="mx-auto mb-4 max-w-4xl"
            data-testid="recoverable-media-job"
          >
            <AlertTitle>{t("mediaJobs.recoverable")}</AlertTitle>
            <AlertDescription className="mt-2 flex flex-wrap items-center justify-between gap-3">
              <span>{t("mediaJobs.paused")}</span>
              <span className="flex gap-2">
                <Button
                  type="button"
                  size="sm"
                  onClick={() => void handleResumeJob(job)}
                >
                  {t("mediaJobs.resume")}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => void handleDiscardJob(job)}
                >
                  {t("mediaJobs.discard")}
                </Button>
              </span>
            </AlertDescription>
          </Alert>
        ))}
        {currentStep === "configure" && (
          <div className="mx-auto max-w-4xl space-y-4">
            <div
              className="gaming-panel space-y-5 p-6"
              data-testid="quick-create"
            >
              <div>
                <h3 className="text-xl font-semibold">
                  {t("autoEdit.quick.title", "Quick Create")}
                </h3>
                <p className="text-sm text-muted-foreground">
                  {t(
                    "autoEdit.quick.description",
                    "Choose the matches and length, then review every scene before rendering.",
                  )}
                </p>
              </div>

              {pinnedClipCount > 0 ? (
                <div className="rounded-md border border-primary/30 bg-primary/5 p-4">
                  <p className="font-medium">
                    {t("autoEdit.pinnedClips.summary", {
                      clips: pinnedClipCount,
                      games: pinnedGameCount,
                      duration: formatDuration(pinnedDuration),
                    })}
                  </p>
                  <p className="text-sm text-muted-foreground">
                    {t("autoEdit.pinnedClips.description")}
                  </p>
                  <Button
                    type="button"
                    variant="link"
                    className="h-auto px-0"
                    onClick={() => {
                      setPinnedClips(null);
                      clearStoryboard();
                    }}
                  >
                    {t("autoEdit.pinnedClips.useAutomatic")}
                  </Button>
                </div>
              ) : (
                <div className="space-y-2">
                  <p className="text-sm font-medium">
                    {t("autoEdit.selectGames")}
                  </p>
                  {!gamesLoading &&
                    availableGames.filter((game) => game.clip_count > 0)
                      .length === 0 && (
                      <div
                        role="alert"
                        className="rounded-md border border-dashed p-3 text-sm text-muted-foreground"
                      >
                        {t("autoEdit.noGamesAvailable")}
                      </div>
                    )}
                  <div className="grid max-h-56 gap-2 overflow-y-auto sm:grid-cols-2">
                    {availableGames
                      .filter((game) => game.clip_count > 0)
                      .map((game) => (
                        <Button
                          key={game.game_id}
                          type="button"
                          variant={
                            selectedGameIds.includes(game.game_id)
                              ? "default"
                              : "outline"
                          }
                          className="h-auto justify-start py-3 text-left"
                          aria-pressed={selectedGameIds.includes(game.game_id)}
                          onClick={() => toggleGameSelection(game.game_id)}
                        >
                          <span>
                            <strong>{game.champion}</strong>
                            <br />
                            <span className="text-xs opacity-80">
                              {game.date} · {game.clip_count}
                            </span>
                          </span>
                        </Button>
                      ))}
                  </div>
                </div>
              )}

              <div className="space-y-2">
                <p className="text-sm font-medium">
                  {t("autoEdit.targetDuration")}
                </p>
                <div className="grid grid-cols-3 gap-2">
                  {([60, 120, 180] as const).map((duration) => (
                    <Button
                      key={duration}
                      type="button"
                      aria-pressed={targetDuration === duration}
                      variant={
                        targetDuration === duration ? "default" : "outline"
                      }
                      onClick={() => setTargetDuration(duration)}
                    >
                      {formatDuration(duration)}
                    </Button>
                  ))}
                </div>
              </div>

              <div className="grid gap-4 sm:grid-cols-2">
                <label className="space-y-1 text-sm font-medium">
                  {t("autoEdit.quick.framing", "LoL framing")}
                  <select
                    className="mt-1 min-h-11 w-full rounded-md border bg-background px-3"
                    value={framingMode}
                    onChange={(event) =>
                      setFramingMode(event.target.value as typeof framingMode)
                    }
                  >
                    <option value="lol_focus_stack">
                      {t("autoEdit.framing.focus", "HUD + combat focus")}
                    </option>
                    <option value="safe_full_frame">
                      {t("autoEdit.framing.safe", "Full frame")}
                    </option>
                    <option value="center_crop">
                      {t("autoEdit.framing.crop", "Center crop")}
                    </option>
                  </select>
                </label>
                <label className="space-y-1 text-sm font-medium">
                  {t("autoEdit.quick.preset", "Export preset")}
                  <select
                    className="mt-1 min-h-11 w-full rounded-md border bg-background px-3"
                    value={platformPreset}
                    onChange={(event) =>
                      setPlatformPreset(
                        event.target.value as typeof platformPreset,
                      )
                    }
                  >
                    <option value="youtube_shorts">YouTube Shorts</option>
                    <option value="tiktok">TikTok</option>
                    <option value="instagram_reels">Instagram Reels</option>
                  </select>
                </label>
              </div>

              <div className="flex flex-wrap justify-between gap-3">
                <Button
                  type="button"
                  variant="ghost"
                  data-testid="advanced-settings-button"
                  onClick={() => setShowAdvanced(true)}
                >
                  {t("autoEdit.quick.advanced", "Advanced settings")}
                </Button>
                <Button
                  type="button"
                  disabled={isLoading || selectedGameIds.length === 0}
                  onClick={handleReviewClips}
                >
                  {t("autoEdit.quick.review", "Review clips")}
                </Button>
              </div>
            </div>

            {showAdvanced && (
              <div className="space-y-3">
                <div className="flex justify-end">
                  <Button
                    type="button"
                    variant="ghost"
                    onClick={() => setShowAdvanced(false)}
                  >
                    {t("common.close", "Close")}
                  </Button>
                </div>
                <AutoEditSettings
                  availableGames={availableGames}
                  selectedGameIds={selectedGameIds}
                  pinnedClipCount={pinnedClipCount}
                  pinnedGameCount={pinnedGameCount}
                  pinnedDuration={pinnedDuration}
                  targetDuration={targetDuration}
                  enableEventZoom={enableEventZoom}
                  enableHookCaptions={enableHookCaptions}
                  currentTemplate={currentTemplate}
                  backgroundMusic={backgroundMusic}
                  audioLevels={audioLevels}
                  metadata={metadata}
                  isLoading={isLoading}
                  gamesLoading={gamesLoading}
                  isFreeTier={!hasProEntitlement}
                  onToggleGame={toggleGameSelection}
                  onUseAutomaticSelection={() => {
                    setPinnedClips(null);
                    clearStoryboard();
                  }}
                  onSetDuration={setTargetDuration}
                  onToggleEventZoom={setEnableEventZoom}
                  onToggleHookCaptions={setEnableHookCaptions}
                  onTemplateChange={(tpl) => setCurrentTemplate(tpl)}
                  onBackgroundMusicChange={setBackgroundMusic}
                  onAudioLevelsChange={setAudioLevels}
                  onMetadataChange={setMetadata}
                  onGenerate={handleReviewClips}
                />
              </div>
            )}
          </div>
        )}

        {currentStep === "storyboard" && (
          <AutoEditStoryboard
            clips={storyboard}
            outputIntent={outputIntent}
            onOutputIntentChange={setOutputIntent}
            onMove={moveStoryboardClip}
            onTrim={updateStoryboardTrim}
            onRemove={removeStoryboardClip}
            onResetRecommendation={handleResetRecommendation}
            onUndo={undoStoryboard}
            onRedo={redoStoryboard}
            canUndo={storyboardPast.length > 0}
            canRedo={storyboardFuture.length > 0}
            onBack={() => setCurrentStep("configure")}
            onGenerate={handleStartGeneration}
            isLoading={isLoading}
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
          <div className="mx-auto max-w-2xl space-y-4">
            <AutoEditProgressView progress={progress} />
            <Button
              type="button"
              variant="destructive"
              className="w-full"
              onClick={() => cancelAutoEdit(progress.job_id)}
            >
              {t("autoEdit.cancelJob", "Cancel generation")}
            </Button>
          </div>
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
