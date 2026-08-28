import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, ChevronDown, Stethoscope } from "lucide-react";
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

  // 막는 것 먼저, 그다음 경고. 백엔드가 만든 문구를 그대로 쓴다 — 여기서
  // 다시 쓰면 원인과 화면이 어긋나기 시작한다.
  //
  // `blockers` 는 이미 정규화 계층(`api/recording.ts`)이 백엔드의 blockers +
  // warnings 를 `severity` 로 구분해 하나로 합쳐 둔 목록이다.
  const readinessIssues = [...(readiness?.blockers ?? [])].sort((a, b) =>
    a.severity === b.severity ? 0 : a.severity === "critical" ? -1 : 1,
  );

  const captureOk =
    readiness?.component_statuses.ffmpeg.status === "ok" &&
    readiness?.component_statuses.audio.status === "ok" &&
    readiness?.component_statuses.gpu.status === "ok" &&
    readiness?.component_statuses.nvenc?.status === "ok" &&
    readiness?.component_statuses.overlay_exclusion?.status === "ok";
  const captureBlocked =
    readiness?.component_statuses.ffmpeg.status === "error" ||
    readiness?.component_statuses.audio.status === "error" ||
    readiness?.component_statuses.gpu.status === "error" ||
    readiness?.component_statuses.nvenc?.status === "error" ||
    readiness?.component_statuses.overlay_exclusion?.status === "error";

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
      readiness?.component_statuses.ffprobe?.status === "ok" &&
      readiness?.component_statuses.disk.status === "ok"
        ? "ok"
        : "warning") as CardStatus,
      label: t("dashboard.services.labels.autoEdit"),
      message:
        readiness?.component_statuses.ffmpeg.status === "ok" &&
        readiness?.component_statuses.ffprobe?.status === "ok" &&
        readiness?.component_statuses.disk.status === "ok"
          ? t("dashboard.services.messages.autoEditReady")
          : t("dashboard.services.messages.autoEditCheck"),
    },
    publish: {
      status: (readiness?.component_statuses.youtube?.status === "ok" &&
      youtubeAuthStatus.authenticated
        ? "ok"
        : "warning") as CardStatus,
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
          {/*
            막고 있는 것을 **구체적으로** 먼저 말한다.

            백엔드는 "FFmpeg 없음 / 번들 FFmpeg 설치" 처럼 무엇이 문제이고 무엇을
            하면 되는지까지 만들어 보낸다(`readiness.blockers[].message/action`).
            그런데 화면은 그 값을 버리고 카드에 "확인 필요" 라고만 적었다 — 녹화가
            안 되는 사용자가 마지막으로 열어 볼 화면에서 아무것도 알 수 없었다.
            아래 카드들은 여전히 한눈에 보는 요약이고, 이 목록이 답이다.
          */}
          {readinessIssues.length > 0 && (
            <ul className="space-y-2" data-testid="diagnostics-blockers">
              {readinessIssues.map((issue) => (
                <li
                  key={`${issue.severity}-${issue.id}`}
                  className={[
                    "flex items-start gap-2 rounded-lg border p-3 text-sm",
                    issue.severity === "critical"
                      ? "border-red-500/30 bg-red-500/10"
                      : "border-yellow-500/30 bg-yellow-500/10",
                  ].join(" ")}
                >
                  <AlertTriangle
                    className={[
                      "mt-0.5 h-4 w-4 shrink-0",
                      issue.severity === "critical"
                        ? "text-red-400"
                        : "text-yellow-400",
                    ].join(" ")}
                    aria-hidden="true"
                  />
                  <span className="min-w-0" style={{ wordBreak: "keep-all" }}>
                    <span className="block font-medium">{issue.message}</span>
                    {issue.action && (
                      <span className="block text-xs text-muted-foreground">
                        {issue.action}
                      </span>
                    )}
                  </span>
                </li>
              ))}
            </ul>
          )}
          {diagnosticsStatus && (
            <div
              className="space-y-2"
              data-testid="release-configuration-checks"
            >
              <h4 className="text-xs font-bold uppercase tracking-wider text-muted-foreground">
                Release configuration
              </h4>
              {diagnosticsStatus.checks.map((check) => (
                <div
                  key={check.key}
                  className="rounded-lg border border-white/10 bg-white/[0.02] p-3 text-sm"
                >
                  <div className="flex items-center justify-between gap-3">
                    <span className="font-medium">{check.label}</span>
                    <span
                      className={
                        check.status === "ok"
                          ? "text-green-400"
                          : "text-yellow-400"
                      }
                    >
                      {check.status.toUpperCase()}
                    </span>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {check.message}
                  </p>
                  {check.action && check.status !== "ok" && (
                    <p className="mt-1 text-xs text-muted-foreground">
                      {check.action}
                    </p>
                  )}
                </div>
              ))}
            </div>
          )}
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
