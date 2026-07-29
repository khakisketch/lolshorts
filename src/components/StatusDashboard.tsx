/**
 * Production Status Dashboard
 *
 * Real-time monitoring of recording status, performance metrics,
 * and system health for production deployments.
 */

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { utilsApi } from "@/api/utils";
import type { DiagnosticsStatus } from "@/api/utils";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  Cpu,
  HardDrive,
  Radio,
  Clock,
  WifiOff,
  ShieldCheck,
} from "lucide-react";
import { cmd } from "@/api/client";
import { logger } from "@/lib/logger";

interface RecordingMetrics {
  fps: number;
  frame_drops: number;
  bitrate_kbps: number;
  cpu_percent: number;
  memory_mb: number;
  buffer_segments: number;
  buffer_size_mb: number;
}

interface SystemMetrics {
  total_cpu_percent: number;
  available_ram_gb: number;
  available_disk_gb: number;
  gpu_percent?: number;
  gpu_memory_mb?: number;
}

type HealthStatus = "Healthy" | "Warning" | "Critical";

interface RecordingStatus {
  status: "idle" | "buffering" | "recording" | "processing" | "error";
  is_monitoring: boolean;
  buffer_duration_secs: number;
}

const MAX_BUFFER_SEGMENTS = 6;
const MAX_MEMORY_MB = 2048;

const getCommandErrorMessage = (error: unknown): string => {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return error.message;
  }

  return error instanceof Error
    ? error.message
    : "Diagnostics are currently unavailable.";
};

const getDiagnosticBadgeVariant = (
  status:
    | DiagnosticsStatus["overall_status"]
    | DiagnosticsStatus["checks"][number]["status"],
) => {
  switch (status) {
    case "ok":
      return "default";
    case "warning":
      return "secondary";
    case "blocked":
      return "destructive";
  }
};

export function StatusDashboard() {
  const { t } = useTranslation();
  const [recordingMetrics, setRecordingMetrics] =
    useState<RecordingMetrics | null>(null);
  const [systemMetrics, setSystemMetrics] = useState<SystemMetrics | null>(
    null,
  );
  const [healthStatus, setHealthStatus] = useState<HealthStatus>("Healthy");
  const [diagnosticsStatus, setDiagnosticsStatus] =
    useState<DiagnosticsStatus | null>(null);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);
  const [diagnosticsExportPath, setDiagnosticsExportPath] = useState<
    string | null
  >(null);
  const [isExportingDiagnostics, setIsExportingDiagnostics] = useState(false);
  const [recordingStatus, setRecordingStatus] = useState<RecordingStatus>({
    status: "idle",
    is_monitoring: false,
    buffer_duration_secs: 0,
  });

  useEffect(() => {
    const pollStatus = async () => {
      try {
        // Fetch recording status
        const status = await cmd<RecordingStatus>(
          "get_detailed_recording_status",
        );
        setRecordingStatus(status);

        // Fetch performance metrics if recording
        if (status.status === "buffering" || status.status === "recording") {
          // Fetch actual metrics from backend
          const recMetrics = await cmd<RecordingMetrics>(
            "get_recording_metrics",
          );
          if (recMetrics) {
            setRecordingMetrics(recMetrics);
          }

          const sysMetrics = await cmd<SystemMetrics>("get_system_metrics");
          setSystemMetrics(sysMetrics);

          // Determine health status (only if recMetrics exists)
          if (recMetrics) {
            if (recMetrics.fps < 45 || recMetrics.cpu_percent > 95) {
              setHealthStatus("Critical");
            } else if (recMetrics.fps < 55 || recMetrics.cpu_percent > 80) {
              setHealthStatus("Warning");
            } else {
              setHealthStatus("Healthy");
            }
          }
        }
      } catch (error) {
        logger.error("Failed to fetch metrics:", error);
      }

      try {
        setDiagnosticsStatus(await utilsApi.getDiagnosticsStatus());
        setDiagnosticsError(null);
      } catch (error) {
        setDiagnosticsStatus(null);
        setDiagnosticsError(getCommandErrorMessage(error));
        logger.error("Failed to fetch diagnostics:", error);
      }
    };

    pollStatus();

    // Poll metrics every 2 seconds
    const interval = setInterval(pollStatus, 2000);

    return () => clearInterval(interval);
  }, []);

  const getStatusBadgeVariant = (status: string) => {
    switch (status) {
      case "buffering":
        return "default";
      case "recording":
        return "destructive";
      case "processing":
        return "default";
      case "error":
        return "destructive";
      default:
        return "secondary";
    }
  };

  const getHealthBadgeVariant = (health: HealthStatus) => {
    switch (health) {
      case "Healthy":
        return "default";
      case "Warning":
        return "secondary";
      case "Critical":
        return "destructive";
    }
  };

  const getHealthIcon = (health: HealthStatus) => {
    switch (health) {
      case "Healthy":
        return <CheckCircle2 className="h-4 w-4" />;
      case "Warning":
        return <AlertTriangle className="h-4 w-4" />;
      case "Critical":
        return <XCircle className="h-4 w-4" />;
    }
  };

  const handleExportDiagnostics = async () => {
    setIsExportingDiagnostics(true);
    setDiagnosticsExportPath(null);
    try {
      const result = await utilsApi.exportDiagnosticsBundle(true);
      setDiagnosticsExportPath(result.output_path);
      setDiagnosticsError(null);
    } catch (error) {
      const message = getCommandErrorMessage(error);
      setDiagnosticsError(message);
      logger.error("Failed to export diagnostics bundle:", error);
    } finally {
      setIsExportingDiagnostics(false);
    }
  };

  return (
    <div data-testid="status-dashboard" className="space-y-4">
      {/* Commercial Readiness Diagnostics */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <div className="flex items-center justify-between">
            <h3 className="text-lg font-semibold flex items-center gap-2">
              <ShieldCheck className="h-5 w-5 text-gaming-cyan" />
              {t("statusDashboard.diagnostics.title", "Diagnostics & Updates")}
            </h3>
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleExportDiagnostics}
                disabled={isExportingDiagnostics}
              >
                {isExportingDiagnostics
                  ? t("statusDashboard.diagnostics.exporting", "Exporting")
                  : t("statusDashboard.diagnostics.export", "Export")}
              </Button>
              {diagnosticsStatus && (
                <Badge
                  variant={getDiagnosticBadgeVariant(
                    diagnosticsStatus.overall_status,
                  )}
                >
                  {diagnosticsStatus.overall_status}
                </Badge>
              )}
            </div>
          </div>
          <p className="text-sm text-muted-foreground">
            {t(
              "statusDashboard.diagnostics.description",
              "Updater, signing, and runtime configuration visibility",
            )}
          </p>
        </div>

        {diagnosticsError ? (
          <Alert>
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>
              {t(
                "statusDashboard.diagnostics.unavailable",
                "Desktop diagnostics unavailable",
              )}
            </AlertTitle>
            <AlertDescription>{diagnosticsError}</AlertDescription>
          </Alert>
        ) : diagnosticsStatus ? (
          <div className="space-y-2">
            {diagnosticsExportPath && (
              <Alert>
                <AlertTitle>
                  {t(
                    "statusDashboard.diagnostics.exportReady",
                    "Diagnostics exported",
                  )}
                </AlertTitle>
                <AlertDescription>{diagnosticsExportPath}</AlertDescription>
              </Alert>
            )}
            {diagnosticsStatus.checks.map((check) => (
              <div
                key={check.key}
                className="p-3 bg-black/20 rounded border border-white/5"
              >
                <div className="flex items-center justify-between gap-3">
                  <div className="font-medium text-sm">{check.label}</div>
                  <Badge variant={getDiagnosticBadgeVariant(check.status)}>
                    {check.status}
                  </Badge>
                </div>
                <p className="text-xs text-muted-foreground mt-1">
                  {check.message}
                </p>
                <p className="text-xs text-gaming-cyan mt-1">{check.action}</p>
              </div>
            ))}
          </div>
        ) : (
          <div className="flex items-center justify-center p-4">
            <Spinner size="sm" />
          </div>
        )}
      </div>

      {/* Recording Status Card */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <div className="flex items-center justify-between">
            <h3 className="text-lg font-semibold flex items-center gap-2">
              <Radio className="h-5 w-5" />
              {t("statusDashboard.recordingStatus")}
            </h3>
            <Badge
              data-testid="status-indicator"
              variant={getStatusBadgeVariant(recordingStatus.status)}
            >
              {recordingStatus.status}
            </Badge>
          </div>
          <p className="text-sm text-muted-foreground">
            {recordingStatus.is_monitoring
              ? t("statusDashboard.autoCapture.active")
              : t("statusDashboard.autoCapture.pressF8")}
          </p>
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <div className="text-sm text-muted-foreground">
              {t("statusDashboard.bufferDuration")}
            </div>
            <div className="text-2xl font-bold">
              {recordingStatus.buffer_duration_secs}s
            </div>
          </div>
          {recordingMetrics && (
            <div>
              <div className="text-sm text-muted-foreground">
                {t("statusDashboard.bufferSegments")}
              </div>
              <div className="text-2xl font-bold">
                {recordingMetrics.buffer_segments}/{MAX_BUFFER_SEGMENTS}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Performance Metrics */}
      {recordingMetrics && (
        <div className="gaming-panel p-6">
          <div className="mb-4">
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-semibold flex items-center gap-2">
                <Activity className="h-5 w-5" />
                {t("statusDashboard.performanceMetrics")}
              </h3>
              <Badge variant={getHealthBadgeVariant(healthStatus)}>
                {getHealthIcon(healthStatus)}
                <span className="ml-1">
                  {t(`statusDashboard.${healthStatus.toLowerCase()}`)}
                </span>
              </Badge>
            </div>
          </div>
          <div className="space-y-4">
            {/* FPS */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">
                  {t("statusDashboard.fps")}
                </span>
                <span className="text-sm text-muted-foreground">
                  {recordingMetrics.fps.toFixed(1)} / 60
                </span>
              </div>
              <Progress
                value={(recordingMetrics.fps / 60) * 100}
                className={recordingMetrics.fps < 55 ? "bg-yellow-500" : ""}
              />
            </div>

            {/* CPU */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium flex items-center gap-1">
                  <Cpu className="h-4 w-4" />
                  {t("statusDashboard.ffmpegCpu")}
                </span>
                <span className="text-sm text-muted-foreground">
                  {recordingMetrics.cpu_percent.toFixed(1)}%
                </span>
              </div>
              <Progress
                value={recordingMetrics.cpu_percent}
                className={
                  recordingMetrics.cpu_percent > 80 ? "bg-orange-500" : ""
                }
              />
            </div>

            {/* Memory */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium flex items-center gap-1">
                  <HardDrive className="h-4 w-4" />
                  {t("statusDashboard.memory")}
                </span>
                <span className="text-sm text-muted-foreground">
                  {recordingMetrics.memory_mb.toFixed(0)} MB
                </span>
              </div>
              <Progress
                value={(recordingMetrics.memory_mb / MAX_MEMORY_MB) * 100}
              />
            </div>

            {/* Bitrate */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">
                  {t("statusDashboard.bitrate")}
                </span>
                <span className="text-sm text-muted-foreground">
                  {(recordingMetrics.bitrate_kbps / 1000).toFixed(1)} Mbps
                </span>
              </div>
            </div>

            {/* Frame Drops */}
            {recordingMetrics.frame_drops > 0 && (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertTitle>
                  {t("statusDashboard.frameDropsDetected")}
                </AlertTitle>
                <AlertDescription>
                  {t("statusDashboard.frameDropsMessage", {
                    count: recordingMetrics.frame_drops,
                  })}
                </AlertDescription>
              </Alert>
            )}
          </div>
        </div>
      )}

      {/* System Resources */}
      {systemMetrics && (
        <div className="gaming-panel p-6">
          <div className="mb-4">
            <h3 className="text-lg font-semibold flex items-center gap-2">
              <Cpu className="h-5 w-5" />
              {t("statusDashboard.systemResources")}
            </h3>
          </div>
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div>
                <div className="text-sm text-muted-foreground">
                  {t("statusDashboard.totalCpu")}
                </div>
                <div className="text-2xl font-bold">
                  {systemMetrics.total_cpu_percent.toFixed(1)}%
                </div>
              </div>
              <div>
                <div className="text-sm text-muted-foreground">
                  {t("statusDashboard.availableRam")}
                </div>
                <div className="text-2xl font-bold">
                  {systemMetrics.available_ram_gb.toFixed(1)} GB
                </div>
              </div>
              <div>
                <div className="text-sm text-muted-foreground">
                  {t("statusDashboard.availableDisk")}
                </div>
                <div className="text-2xl font-bold">
                  {systemMetrics.available_disk_gb.toFixed(1)} GB
                </div>
              </div>
              {systemMetrics.gpu_percent && (
                <div>
                  <div className="text-sm text-muted-foreground">
                    {t("statusDashboard.gpu")}
                  </div>
                  <div className="text-2xl font-bold">
                    {systemMetrics.gpu_percent.toFixed(1)}%
                  </div>
                </div>
              )}
            </div>

            {/* Low disk warning */}
            {systemMetrics.available_disk_gb < 5 && (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertTitle>{t("statusDashboard.lowDiskSpace")}</AlertTitle>
                <AlertDescription>
                  {t("statusDashboard.lowDiskSpaceMessage")}
                </AlertDescription>
              </Alert>
            )}
          </div>
        </div>
      )}

      {/* Error state with actionable hints */}
      {recordingStatus.status === "error" && (
        <div className="gaming-panel p-6">
          <div className="space-y-3">
            <Alert variant="destructive">
              <XCircle className="h-4 w-4" />
              <AlertTitle>{t("errors.recordingFailed")}</AlertTitle>
              <AlertDescription>
                <div className="mt-2 space-y-2 text-sm">
                  <p className="font-medium">
                    {t("statusDashboard.errorHints.checkThese")}
                  </p>
                  <ul className="list-disc list-inside space-y-1">
                    <li>{t("statusDashboard.errorHints.gameRunning")}</li>
                    <li>{t("statusDashboard.errorHints.ffmpegPresent")}</li>
                    <li>{t("statusDashboard.errorHints.audioDevice")}</li>
                    <li>{t("statusDashboard.errorHints.diskSpace")}</li>
                  </ul>
                </div>
              </AlertDescription>
            </Alert>
          </div>
        </div>
      )}

      {/* Not monitoring hint */}
      {!recordingStatus.is_monitoring && recordingStatus.status === "idle" && (
        <div className="gaming-panel p-6">
          <Alert>
            <WifiOff className="h-4 w-4" />
            <AlertDescription>
              {t("statusDashboard.autoCapture.pressF8")}
            </AlertDescription>
          </Alert>
        </div>
      )}

      {/* Hotkey Reference */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <Clock className="h-5 w-5" />
            {t("statusDashboard.hotkeyReference")}
          </h3>
        </div>
        <div className="space-y-2 text-sm">
          <div className="flex justify-between">
            <span className="text-muted-foreground">
              {t("dashboard.hotkeys.toggleRecording")}
            </span>
            <kbd className="px-2 py-1 bg-muted rounded">F8</kbd>
          </div>
          <div className="flex justify-between">
            <span className="text-muted-foreground">
              {t("dashboard.hotkeys.manualSave")}
            </span>
            <kbd className="px-2 py-1 bg-muted rounded">F9</kbd>
          </div>
          <div className="flex justify-between">
            <span className="text-muted-foreground">
              {t("dashboard.hotkeys.deleteLast")}
            </span>
            <kbd className="px-2 py-1 bg-muted rounded">F10</kbd>
          </div>
        </div>
      </div>
    </div>
  );
}
