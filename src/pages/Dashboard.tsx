import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { useAuthStore } from "@/lib/auth";
import { useRecordingStore } from "@/stores/recordingStore";
import { lcuApi, UnifiedGameStatus } from "@/api/lcu";
import { utilsApi } from "@/api/utils";
import { settingsApi } from "@/api/settings";
import { AuthModal } from "@/components/auth";
import { formatStorage } from "@/lib/utils";
import { useGameNotifications } from "@/hooks/useGameNotifications";
import { RecordingControls } from "@/components/RecordingControls";
import {
  getDDragonVersion,
  getChampionIconUrl,
  getChampionSplashUrl,
} from "@/lib/ddragon";
import { logger } from "@/lib/logger";
import {
  CheckCircle2,
  AlertTriangle,
  XCircle,
  ChevronDown,
  Gamepad2,
  Keyboard,
  Radio,
  Loader2,
} from "lucide-react";
import type { StorageStats } from "@/types/storage";
import type { HotkeySettings } from "@/types";

const DEFAULT_HOTKEYS: HotkeySettings = {
  toggle_recording: "F8",
  manual_save_clip: "F9",
  delete_last_clip: "F10",
};

type ReadinessComponentKey = "ffmpeg" | "audio" | "disk" | "lcu" | "gpu";

export function Dashboard() {
  const { t } = useTranslation();
  const { checkAuth } = useAuthStore();
  const {
    status: { state: recordingState },
    readiness,
    error: recordingError,
  } = useRecordingStore();

  const [showAuthModal, setShowAuthModal] = useState(false);
  const [gameStatus, setGameStatus] = useState<UnifiedGameStatus | null>(null);

  useGameNotifications(gameStatus);
  const [isConnecting, setIsConnecting] = useState<boolean>(false);
  const [ddragonVersion, setDdragonVersion] = useState<string | null>(null);

  useEffect(() => {
    getDDragonVersion().then(setDdragonVersion);
  }, []);
  const [stats, setStats] = useState<StorageStats | null>(null);
  const [hotkeys, setHotkeys] = useState<HotkeySettings>(DEFAULT_HOTKEYS);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdate, setLastUpdate] = useState<Date>(new Date());
  const [readinessOpen, setReadinessOpen] = useState<boolean>(false);
  const [guideOpen, setGuideOpen] = useState<boolean>(false);

  const lcuConnected = gameStatus?.lcu_connected ?? false;
  const inGame = gameStatus?.in_game ?? false;
  const isRecording = gameStatus?.is_recording ?? false;

  const updateGameStatus = useCallback(async () => {
    try {
      const status = await lcuApi.getUnifiedGameStatus();
      setGameStatus(status);
      setLastUpdate(new Date());
    } catch {
      setGameStatus((prev) =>
        prev
          ? {
              ...prev,
              in_game: false,
              summoner_name: null,
              champion_name: null,
              game_time: null,
              is_recording: false,
            }
          : null,
      );
    }
  }, []);

  const handleConnectLcu = useCallback(async () => {
    setIsConnecting(true);
    try {
      await lcuApi.connect();
      await updateGameStatus();
    } catch (error) {
      logger.error("LCU connection failed:", error);
    } finally {
      setIsConnecting(false);
    }
  }, [updateGameStatus]);

  useEffect(() => {
    let isMounted = true;

    const initializeDashboard = async () => {
      try {
        setIsLoading(true);
        setError(null);

        // Refresh the auth store for the sidebar, but never let it block the
        // dashboard's primary signals (connection / game / stats).
        checkAuth().catch(() => {
          /* auth store handles its own errors */
        });

        await handleConnectLcu();
        if (!isMounted) return;

        const statsResult = await utilsApi.getDashboardStats();
        if (!isMounted) return;
        setStats(statsResult);

        try {
          const settings = await settingsApi.getRecordingSettings();
          if (!isMounted) return;
          if (settings?.hotkeys) setHotkeys(settings.hotkeys);
        } catch {
          /* keep default hotkey labels */
        }
      } catch (err) {
        if (!isMounted) return;
        setError(
          err instanceof Error
            ? err.message
            : t("dashboard.errors.initialization"),
        );
      } finally {
        if (isMounted) {
          setIsLoading(false);
        }
      }
    };

    initializeDashboard();

    const interval = setInterval(() => {
      if (isMounted) {
        updateGameStatus();
      }
    }, 2000);

    return () => {
      isMounted = false;
      clearInterval(interval);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const formatGameTime = (seconds: number | null | undefined) => {
    if (seconds == null) return "--:--";
    return `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
  };

  // Readiness summary -------------------------------------------------------
  const blockerCount = readiness?.blockers.length ?? 0;
  const hasCritical =
    readiness?.blockers.some((b) => b.severity === "critical") ?? false;
  const readinessTone = !readiness
    ? "checking"
    : readiness.ready
      ? "ready"
      : hasCritical
        ? "blocked"
        : "warning";

  const readinessSummaryText = !readiness
    ? t("dashboard.readiness.summaryChecking")
    : readiness.ready
      ? t("dashboard.readiness.summaryReady")
      : t("dashboard.readiness.summaryActionNeeded", { n: blockerCount });

  const showGettingStarted = stats != null && stats.total_games === 0;

  // Hero state --------------------------------------------------------------
  const heroState = !lcuConnected
    ? "disconnected"
    : inGame && isRecording
      ? "recording"
      : inGame
        ? "ingame"
        : "waiting";

  const heroBadge = {
    disconnected: {
      label: t("dashboard.lcuStatus.disconnected"),
      cls: "bg-red-500/20 text-red-400 border-red-500/50",
      dot: "bg-red-400",
      pulse: false,
    },
    waiting: {
      label: t("dashboard.lcuStatus.connected"),
      cls: "bg-green-500/20 text-green-400 border-green-500/50",
      dot: "bg-green-400",
      pulse: true,
    },
    ingame: {
      label: t("dashboard.gameStatus.inGame"),
      cls: "bg-gaming-cyan/20 text-gaming-cyan border-gaming-cyan/50",
      dot: "bg-gaming-cyan",
      pulse: true,
    },
    recording: {
      label: `REC ${formatGameTime(gameStatus?.game_time)}`,
      cls: "bg-red-600 text-white border-red-500",
      dot: "bg-white",
      pulse: true,
    },
  }[heroState];

  const heroSubtitle = {
    disconnected: t("dashboard.hero.connectSubtitle"),
    waiting: t("dashboard.hero.waitingSubtitle"),
    ingame: t("dashboard.gameStatus.messages.monitoring"),
    recording: t("dashboard.hero.recordingSubtitle"),
  }[heroState];

  const gameMode = gameStatus?.game_mode;
  const gameModeLabel =
    gameMode === "TFT"
      ? "TFT"
      : typeof gameMode === "string"
        ? gameMode
        : "Replay";
  const isTft = gameMode === "TFT";
  const showChampion = inGame && !isTft && !!gameStatus?.champion_name;

  const hotkeyRows: Array<{ key: string; label: string }> = [
    {
      key: hotkeys.toggle_recording,
      label: t("dashboard.hotkeys.toggleRecording"),
    },
    { key: hotkeys.manual_save_clip, label: t("dashboard.hotkeys.manualSave") },
    { key: hotkeys.delete_last_clip, label: t("dashboard.hotkeys.deleteLast") },
  ];

  return (
    <div data-testid="dashboard" className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2
            className="text-xl font-bold focus:outline-none"
            data-autofocus
            tabIndex={-1}
          >
            {t("dashboard.title")}
          </h2>
          <p className="text-xs text-muted-foreground">
            {t("dashboard.lastUpdate", {
              time: lastUpdate.toLocaleTimeString(),
            })}
          </p>
        </div>
        {isLoading && <Spinner size="sm" />}
      </div>

      {/* Error Display */}
      {error && (
        <div
          className="gaming-panel p-4"
          style={{ borderColor: "rgba(255, 0, 60, 0.3)" }}
        >
          <div className="flex items-center gap-2 text-gaming-magenta">
            <span className="text-sm font-medium">
              {t("dashboard.error.title")}
            </span>
            <span className="text-sm">{error}</span>
            <button
              onClick={() => window.location.reload()}
              className="ml-auto text-xs underline hover:text-white"
            >
              {t("dashboard.error.retry")}
            </button>
          </div>
        </div>
      )}

      {recordingError && (
        <div
          className="gaming-panel p-4"
          style={{ borderColor: "rgba(255, 0, 60, 0.3)" }}
        >
          <div className="flex items-center gap-2 text-gaming-magenta">
            <span className="text-sm font-medium">
              {t("dashboard.recordingError.title", "Recording status error")}
            </span>
            <span className="text-sm">{recordingError}</span>
          </div>
        </div>
      )}

      {/* Loading Skeletons */}
      {isLoading && !gameStatus && (
        <div className="space-y-4">
          <div className="gaming-panel p-6">
            <Skeleton className="h-6 w-48 mb-4 bg-white/5" />
            <Skeleton className="h-24 w-full bg-white/5" />
          </div>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            {[1, 2, 3, 4].map((i) => (
              <div key={i} className="gaming-panel p-4">
                <Skeleton className="h-10 w-full bg-white/5" />
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Main Content */}
      {(!isLoading || gameStatus) && (
        <>
          {/* HERO: League + Current game + Recording, one signal */}
          <section
            className="gaming-panel relative overflow-hidden"
            style={
              heroState === "recording"
                ? { borderColor: "rgba(255, 0, 60, 0.35)" }
                : undefined
            }
          >
            {showChampion && !isTft && (
              <>
                <div
                  className="absolute inset-0 bg-cover bg-center bg-no-repeat opacity-25"
                  style={{
                    backgroundImage: `url(${getChampionSplashUrl(gameStatus!.champion_name!)})`,
                  }}
                  aria-hidden="true"
                />
                <div className="absolute inset-0 bg-gradient-to-r from-[hsl(240,18%,9%)] via-[hsl(240,18%,9%)]/90 to-transparent" />
              </>
            )}

            <div className="relative z-10 p-5 md:p-6">
              {/* Top row: title + primary badge */}
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <h3 className="flex items-center gap-2 text-lg font-bold leading-tight">
                    <Gamepad2 className="h-5 w-5 text-gaming-cyan shrink-0" />
                    <span className="truncate">
                      {t("dashboard.lcuStatus.title")}
                    </span>
                  </h3>
                  <p
                    className="mt-1 text-sm text-muted-foreground"
                    style={{ wordBreak: "keep-all" }}
                  >
                    {heroSubtitle}
                  </p>
                </div>
                <div
                  data-testid="lcu-status"
                  className={`gaming-status-badge inline-flex shrink-0 items-center gap-2 rounded border px-3 py-1 text-xs font-bold uppercase tracking-normal ${heroBadge.cls} ${
                    heroState === "recording" ? "animate-pulse-red" : ""
                  }`}
                >
                  <span
                    className={`h-2 w-2 rounded-full ${heroBadge.dot} ${
                      heroBadge.pulse ? "animate-pulse" : ""
                    }`}
                  />
                  {heroBadge.label}
                </div>
              </div>

              {/* Body */}
              {inGame && gameStatus ? (
                <div className="mt-4">
                  <div className="flex items-end gap-4">
                    {showChampion && (
                      <div className="h-16 w-16 shrink-0 overflow-hidden border-2 border-gaming-cyan shadow-[0_0_15px_rgba(0,240,255,0.4)]">
                        <img
                          src={
                            ddragonVersion
                              ? getChampionIconUrl(
                                  ddragonVersion,
                                  gameStatus.champion_name!,
                                )
                              : ""
                          }
                          alt={gameStatus.champion_name ?? ""}
                          className="h-full w-full object-cover"
                          onError={(e) => {
                            (e.target as HTMLImageElement).style.display =
                              "none";
                          }}
                        />
                      </div>
                    )}
                    <div className="min-w-0">
                      <h4 className="truncate text-xl font-black italic uppercase tracking-wide gaming-glow-cyan">
                        {isTft ? "TFT" : gameStatus.champion_name}
                      </h4>
                      <p className="text-xs font-bold uppercase text-gaming-cyan">
                        {gameModeLabel}
                      </p>
                    </div>
                  </div>

                  <div className="mt-4 grid grid-cols-3 gap-3">
                    <div className="flex flex-col items-center border border-white/10 bg-black/50 p-2.5">
                      <span className="mb-1 text-[10px] uppercase text-muted-foreground">
                        {t("dashboard.gameStatus.fields.gameMode")}
                      </span>
                      <span className="text-sm font-bold">{gameModeLabel}</span>
                    </div>
                    <div className="flex flex-col items-center border border-white/10 bg-black/50 p-2.5">
                      <span className="mb-1 text-[10px] uppercase text-muted-foreground">
                        {t("dashboard.gameStatus.fields.gameTime")}
                      </span>
                      <span className="font-mono text-sm font-bold">
                        {formatGameTime(gameStatus.game_time)}
                      </span>
                    </div>
                    <div className="flex flex-col items-center border border-white/10 bg-black/50 p-2.5">
                      <span className="mb-1 text-[10px] uppercase text-muted-foreground">
                        {t("dashboard.gameStatus.fields.summoner")}
                      </span>
                      <span className="max-w-full truncate text-sm font-bold">
                        {gameStatus.summoner_name ?? "-"}
                      </span>
                    </div>
                  </div>
                </div>
              ) : (
                <div className="mt-4 flex items-center gap-3">
                  {heroState === "disconnected" ? (
                    <>
                      {isConnecting ? (
                        <Spinner size="sm" className="text-gaming-cyan" />
                      ) : (
                        <Radio className="h-4 w-4 text-muted-foreground" />
                      )}
                      <p
                        className="text-xs text-muted-foreground"
                        style={{ wordBreak: "keep-all" }}
                      >
                        {isConnecting
                          ? t("dashboard.lcuStatus.connecting")
                          : t("dashboard.hero.connectHint")}
                      </p>
                    </>
                  ) : (
                    <>
                      <span className="h-2 w-2 animate-pulse rounded-full bg-green-400" />
                      <p className="text-sm text-muted-foreground">
                        {gameStatus?.summoner_name
                          ? gameStatus.summoner_name
                          : t("dashboard.gameStatus.messages.waiting")}
                      </p>
                    </>
                  )}
                </div>
              )}
            </div>
          </section>

          {/* Manual recording controls — start/stop auto-capture, save replay */}
          <RecordingControls />

          {/* Readiness summary — one line, expandable */}
          <section className="gaming-panel">
            <button
              type="button"
              onClick={() => setReadinessOpen((o) => !o)}
              aria-expanded={readinessOpen}
              className="flex w-full items-center gap-3 px-5 py-3 text-left min-h-[44px]"
            >
              {readinessTone === "ready" ? (
                <CheckCircle2 className="h-5 w-5 shrink-0 text-green-400" />
              ) : readinessTone === "blocked" ? (
                <XCircle className="h-5 w-5 shrink-0 text-red-400" />
              ) : readinessTone === "warning" ? (
                <AlertTriangle className="h-5 w-5 shrink-0 text-yellow-400" />
              ) : (
                <Loader2 className="h-5 w-5 shrink-0 animate-spin text-muted-foreground" />
              )}
              <span
                className={`flex-1 text-sm font-semibold ${
                  readinessTone === "ready"
                    ? "text-green-400"
                    : readinessTone === "blocked"
                      ? "text-red-400"
                      : readinessTone === "warning"
                        ? "text-yellow-400"
                        : "text-muted-foreground"
                }`}
                style={{ wordBreak: "keep-all" }}
              >
                {readinessSummaryText}
              </span>
              <span className="flex items-center gap-1 text-xs text-muted-foreground">
                {readinessOpen
                  ? t("dashboard.readiness.hideDetails")
                  : t("dashboard.readiness.showDetails")}
                <ChevronDown
                  className={`h-4 w-4 transition-transform ${
                    readinessOpen ? "rotate-180" : ""
                  }`}
                />
              </span>
            </button>

            {readinessOpen && (
              <div className="space-y-3 border-t border-white/5 px-5 py-4">
                {readiness ? (
                  <>
                    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-5">
                      {(
                        Object.entries(readiness.component_statuses) as Array<
                          [ReadinessComponentKey, { status: string }]
                        >
                      ).map(([comp, data]) => (
                        <div
                          key={comp}
                          className="flex items-center gap-2 rounded border border-white/5 bg-black/30 p-2 text-xs"
                        >
                          <span
                            className={`h-2 w-2 shrink-0 rounded-full ${
                              data.status === "ok"
                                ? "bg-green-500"
                                : data.status === "warning"
                                  ? "bg-yellow-500"
                                  : "bg-red-500"
                            }`}
                          />
                          <span
                            className="truncate text-muted-foreground"
                            style={{ wordBreak: "keep-all" }}
                          >
                            {t(`dashboard.readiness.componentNames.${comp}`)}
                          </span>
                          <span
                            className={`ml-auto font-medium ${
                              data.status === "ok"
                                ? "text-green-400"
                                : data.status === "warning"
                                  ? "text-yellow-400"
                                  : "text-red-400"
                            }`}
                          >
                            {t(
                              `dashboard.readiness.componentStatus.${
                                data.status === "ok"
                                  ? "ok"
                                  : data.status === "warning"
                                    ? "warning"
                                    : "error"
                              }`,
                            )}
                          </span>
                        </div>
                      ))}
                    </div>
                    {readiness.blockers.length > 0 && (
                      <ul className="space-y-1 rounded border border-red-500/20 bg-red-500/10 p-3 text-xs text-red-400">
                        {readiness.blockers.map((b, i) => (
                          <li
                            key={i}
                            className="flex items-start gap-1.5"
                            style={{ wordBreak: "keep-all" }}
                          >
                            <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" />
                            <span>
                              {t(`recordingControls.readiness.codes.${b.id}`, {
                                defaultValue: b.message,
                              })}
                            </span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </>
                ) : (
                  <div className="flex items-center gap-2">
                    <Spinner size="sm" className="text-gaming-cyan" />
                    <p className="text-sm text-muted-foreground">
                      {t("dashboard.readiness.checking")}
                    </p>
                  </div>
                )}
              </div>
            )}
          </section>

          {/* Quick stats */}
          <section className="grid grid-cols-2 gap-4 md:grid-cols-4">
            <div className="gaming-panel flex flex-col justify-center p-4 text-center">
              <div className="text-lg font-black text-gaming-purple">
                {stats ? formatStorage(stats.total_size_bytes) : "-"}
              </div>
              <div
                className="mt-1 text-[11px] uppercase text-muted-foreground"
                style={{ wordBreak: "keep-all" }}
              >
                {stats?.total_disk_usage_bytes != null
                  ? t("dashboard.stats.clipLibrary")
                  : t("dashboard.stats.storageUsed")}
              </div>
            </div>

            {stats?.total_disk_usage_bytes != null && (
              <div className="gaming-panel flex flex-col justify-center p-4 text-center">
                <div className="text-lg font-black text-gaming-cyan">
                  {formatStorage(stats.total_disk_usage_bytes)}
                </div>
                <div
                  className="mt-1 text-[11px] uppercase text-muted-foreground"
                  style={{ wordBreak: "keep-all" }}
                >
                  {t("dashboard.stats.totalDiskUsage")}
                </div>
              </div>
            )}

            <div className="gaming-panel flex flex-col justify-center p-4 text-center">
              <div className="text-2xl font-black text-white">
                {stats ? stats.total_games : "-"}
              </div>
              <div
                className="mt-1 text-[11px] uppercase text-muted-foreground"
                style={{ wordBreak: "keep-all" }}
              >
                {t("dashboard.stats.totalGames")}
              </div>
            </div>

            <div className="gaming-panel flex flex-col justify-center p-4 text-center">
              <div className="text-2xl font-black text-white">
                {stats ? stats.total_clips : "-"}
              </div>
              <div
                className="mt-1 text-[11px] uppercase text-muted-foreground"
                style={{ wordBreak: "keep-all" }}
              >
                {t("dashboard.stats.totalClips")}
              </div>
            </div>
          </section>

          {/* Hotkey reference — single row */}
          <section className="gaming-panel px-5 py-3">
            <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
              <span className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-muted-foreground">
                <Keyboard className="h-4 w-4 text-gaming-cyan" />
                {t("dashboard.hotkeys.title")}
              </span>
              {hotkeyRows.map((row) => (
                <span
                  key={row.label}
                  className="flex items-center gap-2 text-xs"
                  style={{ wordBreak: "keep-all" }}
                >
                  <kbd className="rounded bg-muted px-2 py-1 font-mono text-[11px]">
                    {row.key}
                  </kbd>
                  <span className="text-muted-foreground">{row.label}</span>
                </span>
              ))}
            </div>
          </section>

          {/* Getting started — first run only, collapsed by default */}
          {showGettingStarted && (
            <section className="gaming-panel">
              <button
                type="button"
                onClick={() => setGuideOpen((o) => !o)}
                aria-expanded={guideOpen}
                className="flex w-full items-center gap-3 px-5 py-3 text-left min-h-[44px]"
              >
                <span className="text-yellow-400">&#9889;</span>
                <span className="flex-1 text-sm font-semibold">
                  {t("dashboard.gettingStarted.title")}
                </span>
                <span className="flex items-center gap-1 text-xs text-muted-foreground">
                  {guideOpen
                    ? t("dashboard.gettingStarted.hide")
                    : t("dashboard.gettingStarted.show")}
                  <ChevronDown
                    className={`h-4 w-4 transition-transform ${
                      guideOpen ? "rotate-180" : ""
                    }`}
                  />
                </span>
              </button>

              {guideOpen && (
                <div className="space-y-2 border-t border-white/5 px-5 py-4">
                  {[
                    {
                      text: t("dashboard.gettingStarted.steps.startLeague"),
                      done: lcuConnected,
                      active: !lcuConnected,
                    },
                    {
                      text: t("dashboard.gettingStarted.steps.enterGame"),
                      done: inGame,
                      active: lcuConnected && !inGame,
                    },
                    {
                      text: t("dashboard.gettingStarted.steps.autoRecord"),
                      done: recordingState === "recording",
                      active: inGame && recordingState !== "recording",
                    },
                    {
                      text: t("dashboard.gettingStarted.steps.playNormal"),
                      done: false,
                      active: false,
                    },
                    {
                      text: t("dashboard.gettingStarted.steps.afterGame"),
                      done: false,
                      active: false,
                    },
                  ].map((step, i) => (
                    <div
                      key={i}
                      className={`flex items-center gap-3 rounded p-2 transition-all ${
                        step.done
                          ? "opacity-50"
                          : step.active
                            ? "border border-gaming-cyan/20 bg-gaming-cyan/5"
                            : ""
                      }`}
                    >
                      <div
                        className={`flex h-6 w-6 shrink-0 items-center justify-center text-xs font-bold ${
                          step.done
                            ? "bg-green-500/20 text-green-400"
                            : step.active
                              ? "animate-pulse-cyan bg-gaming-cyan/20 text-gaming-cyan"
                              : "bg-white/5 text-muted-foreground"
                        }`}
                      >
                        {step.done ? "✓" : i + 1}
                      </div>
                      <span
                        className={`text-sm ${
                          step.done
                            ? "text-muted-foreground line-through"
                            : step.active
                              ? "font-medium text-gaming-cyan"
                              : "text-muted-foreground"
                        }`}
                        style={{ wordBreak: "keep-all" }}
                      >
                        {step.text}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </section>
          )}
        </>
      )}

      {/* Auth Modal */}
      {showAuthModal && (
        <AuthModal
          open={showAuthModal}
          onClose={() => setShowAuthModal(false)}
        />
      )}
    </div>
  );
}
