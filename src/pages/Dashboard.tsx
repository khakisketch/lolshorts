import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { useAuthStore } from "@/lib/auth";
import { useRecordingStore } from "@/stores/recordingStore";
import { lcuApi, UnifiedGameStatus } from "@/api/lcu";
import { utilsApi } from "@/api/utils";
import { cmd } from "@/api/client";
import { youtubeApi } from "@/api/youtube";
import { AuthModal } from "@/components/auth";
import { StatusDashboard } from "@/components/StatusDashboard";
import { HealthHub } from "@/components/HealthHub";
import { SupportSummary } from "@/components/SupportSummary";
import { formatStorage } from "@/lib/utils";
import { useGameNotifications } from "@/hooks/useGameNotifications";
import {
  getDDragonVersion,
  getChampionIconUrl,
  getChampionSplashUrl,
} from "@/lib/ddragon";
import { logger } from "@/lib/logger";
import { CheckCircle2, AlertTriangle, XCircle } from "lucide-react";
import type { AuthStatus } from "@/types/youtube";
import type { DiagnosticsStatus, SystemMetrics } from "@/api/utils";

interface StorageStats {
  total_games: number;
  total_clips: number;
  total_size_bytes: number;
}

const disconnectedYouTubeAuthStatus: AuthStatus = {
  authenticated: false,
  expires_at: null,
  has_refresh_token: false,
};

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
  const [diagnosticsStatus, setDiagnosticsStatus] =
    useState<DiagnosticsStatus | null>(null);
  const [systemMetrics, setSystemMetrics] = useState<SystemMetrics | null>(
    null,
  );
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdate, setLastUpdate] = useState<Date>(new Date());
  const [youtubeAuthStatus, setYoutubeAuthStatus] = useState<AuthStatus>(
    disconnectedYouTubeAuthStatus,
  );

  const lcuConnected = gameStatus?.lcu_connected ?? false;
  const inGame = gameStatus?.in_game ?? false;

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
        await checkAuth();
        if (!isMounted) return;
        await handleConnectLcu();
        if (!isMounted) return;
        try {
          const authStatus = await youtubeApi.getAuthStatus();
          if (!isMounted) return;
          setYoutubeAuthStatus(authStatus ?? disconnectedYouTubeAuthStatus);
        } catch {
          if (!isMounted) return;
          setYoutubeAuthStatus(disconnectedYouTubeAuthStatus);
        }
        if (!isMounted) return;
        const statsResult = await utilsApi.getDashboardStats();
        if (!isMounted) return;
        setStats(statsResult);

        const diagResult = await utilsApi.getDiagnosticsStatus();
        if (!isMounted) return;
        setDiagnosticsStatus(diagResult);

        const sysResult = await cmd<SystemMetrics>("get_system_metrics");
        if (!isMounted) return;
        setSystemMetrics(sysResult);
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

  const availableDiskGb = systemMetrics?.available_disk_gb;
  const storageStatus =
    availableDiskGb == null
      ? "checking"
      : availableDiskGb > 10
        ? "ok"
        : availableDiskGb > 2
          ? "warning"
          : "blocked";
  const storageMessage =
    availableDiskGb == null
      ? "Checking storage"
      : availableDiskGb > 10
        ? "Plenty of space"
        : availableDiskGb > 2
          ? "Space running low"
          : "Disk almost full";

  const healthData = {
    capture: {
      status: (readiness?.component_statuses.ffmpeg.status === "ok" &&
      readiness?.component_statuses.audio.status === "ok" &&
      readiness?.component_statuses.gpu.status === "ok"
        ? "ok"
        : readiness?.component_statuses.ffmpeg.status === "error" ||
            readiness?.component_statuses.audio.status === "error" ||
            readiness?.component_statuses.gpu.status === "error"
          ? "blocked"
          : "warning") as "ok" | "warning" | "blocked" | "checking",
      label: "Capture",
      message:
        readiness?.component_statuses.ffmpeg.status === "ok" &&
        readiness?.component_statuses.audio.status === "ok" &&
        readiness?.component_statuses.gpu.status === "ok"
          ? "Ready to record"
          : "Check recording components",
    },
    replay: {
      status: (readiness?.component_statuses.lcu.status === "ok"
        ? "ok"
        : readiness?.component_statuses.lcu.status === "error"
          ? "blocked"
          : "warning") as "ok" | "warning" | "blocked" | "checking",
      label: "Replay",
      message:
        readiness?.component_statuses.lcu.status === "ok"
          ? "Replay system ready"
          : "Check LCU connection",
    },
    autoEdit: {
      status: (readiness?.component_statuses.ffmpeg.status === "ok" &&
      readiness?.component_statuses.disk.status === "ok"
        ? "ok"
        : "warning") as "ok" | "warning" | "blocked" | "checking",
      label: "AutoEdit",
      message:
        readiness?.component_statuses.ffmpeg.status === "ok" &&
        readiness?.component_statuses.disk.status === "ok"
          ? "AI Editor ready"
          : "Check system requirements",
    },
    publish: {
      status: (youtubeAuthStatus.authenticated ? "ok" : "warning") as
        | "ok"
        | "warning"
        | "blocked"
        | "checking",
      label: "Publish",
      message: youtubeAuthStatus.authenticated
        ? "YouTube connected"
        : "Connect account to publish",
    },
    storage: {
      status: storageStatus as "ok" | "warning" | "blocked" | "checking",
      label: "Storage",
      message: storageMessage,
    },
    service: {
      status: (diagnosticsStatus?.overall_status === "ok"
        ? "ok"
        : diagnosticsStatus?.overall_status === "warning"
          ? "warning"
          : "blocked") as "ok" | "warning" | "blocked" | "checking",
      label: "Service",
      message:
        diagnosticsStatus?.overall_status === "ok"
          ? "All services healthy"
          : diagnosticsStatus?.overall_status === "warning"
            ? "Some warnings detected"
            : "Service issues found",
    },
  };

  return (
    <div data-testid="dashboard" className="space-y-6">
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
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {[1, 2, 3].map((i) => (
            <div key={i} className="gaming-panel p-6">
              <Skeleton className="h-6 w-40 mb-4 bg-white/5" />
              <Skeleton className="h-20 w-full bg-white/5" />
            </div>
          ))}
        </div>
      )}

      {/* Main Content */}
      {(!isLoading || gameStatus) && (
        <>
          {/* Home/Health Hub */}
          <HealthHub healthData={healthData} />

          {/* Top Grid: LCU & Active Game */}
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            {/* Recording Readiness Panel */}
            <div className="gaming-panel p-6">
              <div className="flex justify-between items-start mb-4">
                <h3 className="text-lg font-bold flex items-center gap-2">
                  <span className="text-gaming-cyan">&#9889;</span>
                  {t("dashboard.readiness.title", "Recording Readiness")}
                </h3>
                <div
                  className={`gaming-status-badge inline-flex max-w-[9rem] items-center gap-1.5 px-2 py-1 text-[11px] font-bold uppercase leading-none tracking-normal ${
                    !readiness
                      ? "bg-white/5 text-muted-foreground border border-white/10"
                      : readiness.ready
                        ? "bg-green-500/20 text-green-400 border border-green-500/50"
                        : readiness.blockers.some(
                              (b) => b.severity === "critical",
                            )
                          ? "bg-red-500/20 text-red-400 border border-red-500/50"
                          : "bg-yellow-500/20 text-yellow-400 border border-yellow-500/50"
                  }`}
                >
                  {readiness ? (
                    <>
                      {readiness.ready ? (
                        <CheckCircle2 className="w-3 h-3" />
                      ) : readiness.blockers.some(
                          (b) => b.severity === "critical",
                        ) ? (
                        <XCircle className="w-3 h-3" />
                      ) : (
                        <AlertTriangle className="w-3 h-3" />
                      )}
                      <span className="whitespace-normal break-words">
                        {readiness.ready
                          ? t("dashboard.readiness.status.ready")
                          : readiness.blockers.some(
                                (b) => b.severity === "critical",
                              )
                            ? t("dashboard.readiness.status.blocked")
                            : t("dashboard.readiness.status.degraded")}
                      </span>
                    </>
                  ) : (
                    t("dashboard.readiness.status.checking")
                  )}
                </div>
              </div>

              {readiness ? (
                <div className="space-y-3">
                  <div className="grid grid-cols-2 gap-2">
                    {Object.entries(readiness.component_statuses).map(
                      ([comp, data]) => (
                        <div
                          key={comp}
                          className="flex items-center gap-2 text-xs p-2 bg-black/30 border border-white/5 rounded"
                        >
                          <div
                            className={`w-2 h-2 rounded-full ${
                              data.status === "ok"
                                ? "bg-green-500"
                                : data.status === "warning"
                                  ? "bg-yellow-500"
                                  : "bg-red-500"
                            }`}
                          />
                          <span className="capitalize text-muted-foreground">
                            {comp}
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
                            {data.status}
                          </span>
                        </div>
                      ),
                    )}
                  </div>
                  {readiness.blockers.length > 0 && (
                    <div className="p-3 bg-red-500/10 border border-red-500/20 rounded text-xs text-red-400 space-y-1">
                      <p className="font-bold flex items-center gap-1">
                        <AlertTriangle className="w-3 h-3" />{" "}
                        {t("dashboard.readiness.blockersTitle")}
                      </p>
                      <ul className="list-disc list-inside opacity-80">
                        {readiness.blockers.map((b, i) => (
                          <li key={i}>{b.message}</li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              ) : (
                <div className="flex items-center gap-2 p-2">
                  <Spinner size="sm" className="text-gaming-cyan" />
                  <p className="text-sm text-muted-foreground">
                    {t("dashboard.readiness.checking")}
                  </p>
                </div>
              )}
            </div>

            {/* LCU Status Panel */}
            <div className="gaming-panel p-6">
              <div className="mb-4 flex flex-col gap-2 2xl:flex-row 2xl:items-start 2xl:justify-between">
                <h3 className="flex items-center gap-2 text-lg font-bold leading-tight">
                  <span className="text-gaming-cyan">&#9889;</span>
                  <span>{t("dashboard.lcuStatus.title")}</span>
                </h3>
                <div
                  data-testid="lcu-status"
                  className={`gaming-status-badge inline-flex w-fit items-center gap-2 px-3 py-1 text-xs font-bold uppercase tracking-normal ${
                    lcuConnected
                      ? "bg-green-500/20 text-green-400 border border-green-500/50"
                      : "bg-red-500/20 text-red-400 border border-red-500/50"
                  }`}
                >
                  <div
                    className={`w-2 h-2 rounded-full ${lcuConnected ? "bg-green-400 animate-pulse" : "bg-red-400"}`}
                  />
                  {lcuConnected
                    ? t("dashboard.lcuStatus.connected")
                    : t("dashboard.lcuStatus.disconnected")}
                </div>
              </div>

              <p className="text-sm text-muted-foreground mb-4">
                {lcuConnected
                  ? t("dashboard.lcuStatus.messages.connected")
                  : t("dashboard.lcuStatus.messages.disconnected")}
              </p>

              {lcuConnected ? (
                <div className="bg-black/40 border border-white/10 p-4 font-mono text-xs text-green-400 rounded space-y-1">
                  <p>&gt; LCU_STATUS: ONLINE</p>
                  {gameStatus?.summoner_name && (
                    <p>&gt; SUMMONER: {gameStatus.summoner_name}</p>
                  )}
                  <p>
                    &gt; {inGame ? "MATCH_DETECTED" : "WAITING_FOR_MATCH..."}
                  </p>
                </div>
              ) : (
                <div className="space-y-3">
                  {isConnecting && (
                    <div className="flex items-center gap-2 p-2">
                      <Spinner size="sm" className="text-gaming-cyan" />
                      <p className="text-sm text-gaming-cyan font-medium">
                        {t("dashboard.lcuStatus.connecting")}
                      </p>
                    </div>
                  )}
                  <div className="bg-black/40 border border-white/10 p-4 font-mono text-xs text-yellow-400 rounded space-y-1">
                    <p>&gt; {t("dashboard.lcuStatus.messages.startLeague")}</p>
                    <p>
                      &gt;{" "}
                      {t("dashboard.lcuStatus.messages.autoReconnect", {
                        seconds: 3,
                      })}
                    </p>
                    <p>
                      &gt; {t("dashboard.lcuStatus.messages.adminRequired")}
                    </p>
                  </div>
                </div>
              )}
            </div>

            {/* Active Game Panel */}
            <div
              className="gaming-panel relative overflow-hidden"
              style={
                inGame ? { borderColor: "rgba(255, 0, 60, 0.3)" } : undefined
              }
            >
              {/* Champion splash background when in game */}
              {inGame &&
                gameStatus?.champion_name &&
                gameStatus.game_mode !== "TFT" && (
                  <>
                    <div
                      className="absolute inset-0 bg-cover bg-center bg-no-repeat opacity-30"
                      style={{
                        backgroundImage: `url(${getChampionSplashUrl(gameStatus.champion_name)})`,
                      }}
                    />
                    <div className="absolute inset-0 bg-gradient-to-r from-[hsl(240,18%,9%)] via-[hsl(240,18%,9%)]/90 to-transparent" />
                  </>
                )}

              <div className="relative z-10 p-6">
                <div className="flex justify-between items-start mb-4">
                  <h3 className="text-lg font-bold flex items-center gap-2">
                    <span className="text-gaming-magenta">&#127918;</span>
                    {t("dashboard.gameStatus.title")}
                  </h3>

                  {inGame && gameStatus?.is_recording ? (
                    <div className="animate-pulse-red bg-red-600 text-white gaming-status-badge px-4 py-1.5 text-xs font-bold uppercase tracking-wider flex items-center gap-2">
                      <div className="w-2 h-2 rounded-full bg-white" />
                      REC {formatGameTime(gameStatus.game_time)}
                    </div>
                  ) : (
                    <div className="gaming-status-badge bg-white/5 text-muted-foreground px-3 py-1 text-xs font-bold uppercase tracking-wider">
                      {inGame
                        ? t("dashboard.gameStatus.inGame")
                        : t("dashboard.gameStatus.noGame")}
                    </div>
                  )}
                </div>

                {inGame && gameStatus ? (
                  <div className="mt-2">
                    {/* Champion display for non-TFT */}
                    {gameStatus.game_mode !== "TFT" &&
                      gameStatus.champion_name && (
                        <div className="flex items-end gap-4 mb-6">
                          <div className="w-16 h-16 border-2 border-gaming-cyan shadow-[0_0_15px_rgba(0,240,255,0.4)] overflow-hidden flex-shrink-0">
                            <img
                              src={
                                ddragonVersion
                                  ? getChampionIconUrl(
                                      ddragonVersion,
                                      gameStatus.champion_name,
                                    )
                                  : ""
                              }
                              alt={gameStatus.champion_name}
                              className="w-full h-full object-cover"
                              onError={(e) => {
                                (e.target as HTMLImageElement).style.display =
                                  "none";
                              }}
                            />
                          </div>
                          <div>
                            <h4 className="text-2xl font-black italic tracking-wide gaming-glow-cyan uppercase">
                              {gameStatus.champion_name}
                            </h4>
                            <p className="text-sm text-gaming-cyan font-bold uppercase">
                              {typeof gameStatus.game_mode === "string"
                                ? gameStatus.game_mode
                                : "Replay"}
                            </p>
                          </div>
                        </div>
                      )}

                    {/* TFT mode */}
                    {gameStatus.game_mode === "TFT" && (
                      <div className="mb-6">
                        <h4 className="text-2xl font-black italic tracking-wide text-purple-400">
                          TFT
                        </h4>
                        <p className="text-sm text-purple-400 font-bold uppercase">
                          {t(
                            "dashboard.gameStatus.messages.tftMonitoring",
                            "TFT",
                          )}
                        </p>
                      </div>
                    )}

                    {/* Game Info Stats */}
                    <div className="grid grid-cols-3 gap-3">
                      <div className="bg-black/50 border border-white/10 p-3 flex flex-col items-center">
                        <span className="text-muted-foreground text-xs mb-1">
                          {t("dashboard.gameStatus.fields.gameMode")}
                        </span>
                        <span className="text-lg font-bold">
                          {gameStatus.game_mode === "TFT"
                            ? "TFT"
                            : typeof gameStatus.game_mode === "string"
                              ? gameStatus.game_mode
                              : "Replay"}
                        </span>
                      </div>
                      <div className="bg-black/50 border border-white/10 p-3 flex flex-col items-center">
                        <span className="text-muted-foreground text-xs mb-1">
                          {t("dashboard.gameStatus.fields.gameTime")}
                        </span>
                        <span className="text-lg font-bold font-mono">
                          {formatGameTime(gameStatus.game_time)}
                        </span>
                      </div>
                      <div className="bg-black/50 border border-white/10 p-3 flex flex-col items-center">
                        <span className="text-muted-foreground text-xs mb-1">
                          {t("dashboard.gameStatus.fields.summoner")}
                        </span>
                        <span className="text-sm font-bold truncate max-w-full">
                          {gameStatus.summoner_name ?? "Unknown"}
                        </span>
                      </div>
                    </div>

                    {/* Monitoring indicator */}
                    <div
                      className={`flex items-center gap-2 mt-4 text-xs ${
                        gameStatus.game_mode === "TFT"
                          ? "text-purple-400"
                          : "text-gaming-cyan"
                      }`}
                    >
                      <div
                        className={`w-1.5 h-1.5 rounded-full animate-pulse ${
                          gameStatus.game_mode === "TFT"
                            ? "bg-purple-500"
                            : "bg-gaming-cyan"
                        }`}
                      />
                      <span>
                        {gameStatus.game_mode === "TFT"
                          ? t(
                              "dashboard.gameStatus.messages.tftMonitoring",
                              "TFT 게임 감지됨",
                            )
                          : gameStatus.is_recording
                            ? t(
                                "dashboard.gameStatus.messages.activelyRecording",
                              )
                            : t("dashboard.gameStatus.messages.monitoring")}
                      </span>
                    </div>
                  </div>
                ) : (
                  <div className="bg-black/30 border border-white/10 p-4 rounded mt-2">
                    <span className="text-sm text-muted-foreground flex items-center gap-2">
                      {lcuConnected ? (
                        <>
                          <span className="w-2 h-2 bg-muted-foreground rounded-full animate-pulse" />
                          {t("dashboard.gameStatus.messages.waiting")}
                        </>
                      ) : (
                        <>
                          <span className="w-2 h-2 bg-muted-foreground rounded-full" />
                          {t("dashboard.gameStatus.messages.connectFirst")}
                        </>
                      )}
                    </span>
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Bottom Grid: Stats & Getting Started */}
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            {/* Storage Stats */}
            <div className="gaming-panel p-6">
              <h3 className="text-lg font-bold mb-6 flex items-center gap-2">
                <span className="text-gaming-purple">&#128190;</span>
                {t("dashboard.stats.title")}
              </h3>

              <div className="space-y-6">
                {stats && (
                  <div>
                    <div className="flex justify-between text-sm mb-2">
                      <span className="text-muted-foreground">
                        {t("dashboard.stats.storageUsed")}
                      </span>
                      <span className="font-bold text-gaming-purple">
                        {formatStorage(stats.total_size_bytes)}
                      </span>
                    </div>
                    <div className="w-full h-2 bg-black rounded-full overflow-hidden">
                      <div
                        className="h-full bg-gradient-to-r from-gaming-cyan to-gaming-purple transition-all duration-500"
                        style={{
                          width: `${Math.min(Math.max((stats.total_size_bytes / (500 * 1024 * 1024 * 1024)) * 100, 2), 100)}%`,
                        }}
                      />
                    </div>
                  </div>
                )}

                <div className="grid grid-cols-2 gap-4">
                  <div className="bg-black/30 p-4 border border-white/5 text-center">
                    <div className="text-2xl font-black text-white mb-1">
                      {stats ? stats.total_games : "-"}
                    </div>
                    <div className="text-xs text-muted-foreground uppercase">
                      {t("dashboard.stats.totalGames")}
                    </div>
                  </div>
                  <div className="bg-black/30 p-4 border border-white/5 text-center">
                    <div className="text-2xl font-black text-white mb-1">
                      {stats ? stats.total_clips : "-"}
                    </div>
                    <div className="text-xs text-muted-foreground uppercase">
                      {t("dashboard.stats.totalClips")}
                    </div>
                  </div>
                </div>
              </div>
            </div>

            {/* Getting Started */}
            <div className="gaming-panel p-6 lg:col-span-2">
              <h3 className="text-lg font-bold mb-2 flex items-center gap-2">
                <span className="text-yellow-400">&#9889;</span>
                {t("dashboard.gettingStarted.title")}
              </h3>
              <p className="text-sm text-muted-foreground mb-4">
                {t("dashboard.gettingStarted.subtitle")}
              </p>

              <div className="space-y-2">
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
                    className={`flex items-center gap-3 p-2.5 rounded transition-all ${
                      step.done
                        ? "opacity-50"
                        : step.active
                          ? "bg-gaming-cyan/5 border border-gaming-cyan/20"
                          : ""
                    }`}
                  >
                    <div
                      className={`w-6 h-6 flex items-center justify-center text-xs font-bold flex-shrink-0 ${
                        step.done
                          ? "bg-green-500/20 text-green-400"
                          : step.active
                            ? "bg-gaming-cyan/20 text-gaming-cyan animate-pulse-cyan"
                            : "bg-white/5 text-muted-foreground"
                      }`}
                    >
                      {step.done ? "\u2713" : i + 1}
                    </div>
                    <span
                      className={`text-sm ${
                        step.done
                          ? "line-through text-muted-foreground"
                          : step.active
                            ? "font-medium text-gaming-cyan"
                            : "text-muted-foreground"
                      }`}
                    >
                      {step.text}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </div>

          <SupportSummary />
          <StatusDashboard />
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
