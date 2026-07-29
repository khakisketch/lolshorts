import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Stethoscope } from "lucide-react";
import { useRecordingStore } from "@/stores/recordingStore";
import { utilsApi } from "@/api/utils";
import { youtubeApi } from "@/api/youtube";
import { cmd } from "@/api/client";
import { HealthHub } from "@/components/HealthHub";
import { SupportSummary } from "@/components/SupportSummary";
import { StatusDashboard } from "@/components/StatusDashboard";
import type { DiagnosticsStatus, SystemMetrics } from "@/api/utils";
import type { AuthStatus } from "@/types/youtube";

const disconnectedYouTubeAuthStatus: AuthStatus = {
  authenticated: false,
  expires_at: null,
  has_refresh_token: false,
};

type CardStatus = "ok" | "warning" | "blocked" | "checking";

/**
 * Advanced diagnostics moved off the main dashboard. Holds the service-health
 * strip, the copyable support summary, and the full status/updates dashboard.
 * Collapsed by default so the Settings page stays calm — one click reveals it
 * and mounts the polling children only when actually opened.
 */
export function DiagnosticsSection() {
  const { t } = useTranslation();
  const { readiness } = useRecordingStore();
  const [open, setOpen] = useState(false);

  const [systemMetrics, setSystemMetrics] = useState<SystemMetrics | null>(
    null,
  );
  const [diagnosticsStatus, setDiagnosticsStatus] =
    useState<DiagnosticsStatus | null>(null);
  const [youtubeAuthStatus, setYoutubeAuthStatus] = useState<AuthStatus>(
    disconnectedYouTubeAuthStatus,
  );

  useEffect(() => {
    if (!open) return;
    let mounted = true;

    const load = async () => {
      try {
        const sys = await cmd<SystemMetrics>("get_system_metrics");
        if (mounted) setSystemMetrics(sys);
      } catch {
        /* leave null — storage card derives from readiness instead */
      }
      try {
        const diag = await utilsApi.getDiagnosticsStatus();
        if (mounted) setDiagnosticsStatus(diag);
      } catch {
        /* ignore */
      }
      try {
        const auth = await youtubeApi.getAuthStatus();
        if (mounted)
          setYoutubeAuthStatus(auth ?? disconnectedYouTubeAuthStatus);
      } catch {
        if (mounted) setYoutubeAuthStatus(disconnectedYouTubeAuthStatus);
      }
    };

    load();
    return () => {
      mounted = false;
    };
  }, [open]);

  // Storage: single source of truth is the recording-readiness disk check so
  // this card can never contradict the dashboard readiness summary. We attach
  // the concrete free-space number so a "blocked" verdict is self-explanatory.
  const availableDiskGb = systemMetrics?.available_disk_gb;
  const gbKnown = typeof availableDiskGb === "number";
  const diskStatus = readiness?.component_statuses.disk.status;
  const storageStatus: CardStatus =
    diskStatus === "ok"
      ? "ok"
      : diskStatus === "warning"
        ? "warning"
        : diskStatus === "error"
          ? "blocked"
          : availableDiskGb == null
            ? "checking"
            : availableDiskGb > 10
              ? "ok"
              : availableDiskGb > 2
                ? "warning"
                : "blocked";
  // Single source of truth: the readiness disk check drives the verdict, and
  // when the free-space number is known we attach it so the verdict is
  // self-explanatory (fixes the "readiness=ok but card=Blocked" mismatch).
  const statusToKey: Record<CardStatus, string> = {
    ok: "ready",
    warning: "needsSetup",
    blocked: "blocked",
    checking: "checking",
  };
  const storageMessage =
    storageStatus === "checking"
      ? t("dashboard.services.messages.storageChecking")
      : gbKnown
        ? t(`dashboard.services.messages.storage${cap(storageStatus)}`, {
            gb: (availableDiskGb as number).toFixed(1),
          })
        : t(`dashboard.services.status.${statusToKey[storageStatus]}`);

  const captureOk =
    readiness?.component_statuses.ffmpeg.status === "ok" &&
    readiness?.component_statuses.audio.status === "ok" &&
    readiness?.component_statuses.gpu.status === "ok";
  const captureBlocked =
    readiness?.component_statuses.ffmpeg.status === "error" ||
    readiness?.component_statuses.audio.status === "error" ||
    readiness?.component_statuses.gpu.status === "error";

  const healthData = {
    capture: {
      status: (captureOk
        ? "ok"
        : captureBlocked
          ? "blocked"
          : "warning") as CardStatus,
      label: t("dashboard.services.labels.capture"),
      message: captureOk
        ? t("dashboard.services.messages.captureReady")
        : t("dashboard.services.messages.captureCheck"),
    },
    replay: {
      status: (readiness?.component_statuses.lcu.status === "ok"
        ? "ok"
        : readiness?.component_statuses.lcu.status === "error"
          ? "blocked"
          : "warning") as CardStatus,
      label: t("dashboard.services.labels.replay"),
      message:
        readiness?.component_statuses.lcu.status === "ok"
          ? t("dashboard.services.messages.replayReady")
          : t("dashboard.services.messages.replayCheck"),
    },
    autoEdit: {
      status: (readiness?.component_statuses.ffmpeg.status === "ok" &&
      readiness?.component_statuses.disk.status === "ok"
        ? "ok"
        : "warning") as CardStatus,
      label: t("dashboard.services.labels.autoEdit"),
      message:
        readiness?.component_statuses.ffmpeg.status === "ok" &&
        readiness?.component_statuses.disk.status === "ok"
          ? t("dashboard.services.messages.autoEditReady")
          : t("dashboard.services.messages.autoEditCheck"),
    },
    publish: {
      status: (youtubeAuthStatus.authenticated ? "ok" : "warning") as CardStatus,
      label: t("dashboard.services.labels.publish"),
      message: youtubeAuthStatus.authenticated
        ? t("dashboard.services.messages.publishReady")
        : t("dashboard.services.messages.publishCheck"),
    },
    storage: {
      status: storageStatus,
      label: t("dashboard.services.labels.storage"),
      message: storageMessage,
    },
    service: {
      status: (diagnosticsStatus?.overall_status === "ok"
        ? "ok"
        : diagnosticsStatus?.overall_status === "warning"
          ? "warning"
          : diagnosticsStatus?.overall_status === "blocked"
            ? "blocked"
            : "checking") as CardStatus,
      label: t("dashboard.services.labels.service"),
      message:
        diagnosticsStatus?.overall_status === "ok"
          ? t("dashboard.services.messages.serviceOk")
          : diagnosticsStatus?.overall_status === "warning"
            ? t("dashboard.services.messages.serviceWarning")
            : diagnosticsStatus?.overall_status === "blocked"
              ? t("dashboard.services.messages.serviceBlocked")
              : t("dashboard.services.status.checking"),
    },
  };

  return (
    <section data-testid="diagnostics-section" className="gaming-panel">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex w-full items-center gap-3 px-6 py-4 text-left min-h-[44px]"
      >
        <Stethoscope className="h-5 w-5 shrink-0 text-gaming-cyan" />
        <span className="flex-1">
          <span className="block text-base font-semibold">
            {t("settings.diagnostics.title")}
          </span>
          <span
            className="block text-sm text-muted-foreground"
            style={{ wordBreak: "keep-all" }}
          >
            {t("settings.diagnostics.description")}
          </span>
        </span>
        <ChevronDown
          className={`h-5 w-5 shrink-0 text-muted-foreground transition-transform ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      {open && (
        <div className="space-y-4 border-t border-white/5 p-6">
          <div>
            <h4 className="mb-3 text-xs font-bold uppercase tracking-wider text-muted-foreground">
              {t("dashboard.services.title")}
            </h4>
            <HealthHub healthData={healthData} />
          </div>
          <SupportSummary />
          <StatusDashboard />
        </div>
      )}
    </section>
  );
}

function cap(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
