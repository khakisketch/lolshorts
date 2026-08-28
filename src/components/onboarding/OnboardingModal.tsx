import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  Check,
  HardDrive,
  Loader2,
  RefreshCw,
  Settings,
  ShieldCheck,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { recordingApi } from "@/api/recording";
import { settingsApi } from "@/api/settings";
import { storageApi } from "@/api/storage";
import {
  utilsApi,
  type DiagnosticsStatus,
  type DiskSpaceInfo,
} from "@/api/utils";
import type { RecordingReadiness, ReadinessComponent } from "@/types";
import type { StorageStats } from "@/types/storage";

const ONBOARDING_KEY = "lolshorts_onboarding_completed";
const ONBOARDING_VERSION = 2;
const HOURLY_ESTIMATE_GB = 9;
type CheckStatus = "ok" | "warning" | "error" | "checking";

interface WizardCheck {
  id: string;
  label: string;
  status: CheckStatus;
  message: string;
  requiredForRecording?: boolean;
}
interface WizardSnapshot {
  readiness: RecordingReadiness;
  diagnostics: DiagnosticsStatus | null;
  disk: DiskSpaceInfo | null;
  storage: StorageStats | null;
  autostart: {
    configured: boolean;
    enabled: boolean;
    error_code: string | null;
  } | null;
}

const fallbackReadiness: RecordingReadiness = {
  ready: false,
  blockers: [],
  component_statuses: {
    ffmpeg: { status: "warning", message: "Check pending" },
    ffprobe: { status: "warning", message: "Check pending" },
    gpu: { status: "warning", message: "Check pending" },
    nvenc: { status: "warning", message: "Check pending" },
    audio: { status: "warning", message: "Check pending" },
    disk: { status: "warning", message: "Check pending" },
    lcu: { status: "warning", message: "Check pending" },
  },
};

const componentStatus = (
  component: ReadinessComponent | undefined,
): CheckStatus => component?.status ?? "warning";
const bytesToGb = (bytes: number | undefined): string =>
  typeof bytes === "number" && Number.isFinite(bytes)
    ? `${(bytes / 1024 ** 3).toFixed(1)} GB`
    : "Unknown";
function isCompletedRecord(value: string | null): boolean {
  if (!value) return false;
  try {
    return (
      (JSON.parse(value) as { version?: number }).version === ONBOARDING_VERSION
    );
  } catch {
    return false;
  }
}

export function OnboardingModal() {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [snapshot, setSnapshot] = useState<WizardSnapshot | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [warningAcknowledged, setWarningAcknowledged] = useState(false);
  const text = useCallback(
    (key: string, fallback: string, values?: Record<string, string | number>) =>
      t(key, { defaultValue: fallback, ...values }),
    [t],
  );

  const refresh = useCallback(async () => {
    setIsChecking(true);
    const [readiness, diagnostics, disk, storage, autostart] =
      await Promise.allSettled([
        recordingApi.getRecordingReadiness(),
        utilsApi.getDiagnosticsStatus(),
        utilsApi.getDiskSpaceInfo(),
        storageApi.getStorageStats(),
        settingsApi.getAutostartStatus(),
      ]);
    setSnapshot({
      readiness:
        readiness.status === "fulfilled" ? readiness.value : fallbackReadiness,
      diagnostics:
        diagnostics.status === "fulfilled" ? diagnostics.value : null,
      disk: disk.status === "fulfilled" ? disk.value : null,
      storage: storage.status === "fulfilled" ? storage.value : null,
      autostart: autostart.status === "fulfilled" ? autostart.value : null,
    });
    setIsChecking(false);
  }, []);

  useEffect(() => {
    if (isCompletedRecord(localStorage.getItem(ONBOARDING_KEY))) return;
    const timer = window.setTimeout(() => {
      setIsOpen(true);
      void refresh();
    }, 500);
    return () => window.clearTimeout(timer);
  }, [refresh]);

  const checks = useMemo<WizardCheck[]>(() => {
    if (!snapshot)
      return [
        ["ffmpeg", text("onboarding.readiness.labels.ffmpeg", "FFmpeg")],
        ["ffprobe", text("onboarding.readiness.labels.ffprobe", "ffprobe")],
        [
          "nvenc",
          text(
            "onboarding.readiness.labels.nvenc",
            "NVIDIA NVENC support target",
          ),
        ],
        ["audio", text("onboarding.readiness.labels.audio", "System audio")],
        ["disk", text("onboarding.readiness.labels.disk", "Storage space")],
        ["lcu", text("onboarding.readiness.labels.lcu", "League Client / LCU")],
      ].map(([id, label]) => ({
        id,
        label,
        status: "checking",
        message: text("onboarding.readiness.checking", "Checking…"),
      }));
    const components = snapshot.readiness.component_statuses;
    const core: WizardCheck[] = [
      {
        id: "ffmpeg",
        label: text("onboarding.readiness.labels.ffmpeg", "FFmpeg"),
        status: componentStatus(components.ffmpeg),
        message: components.ffmpeg.message,
        requiredForRecording: true,
      },
      {
        id: "ffprobe",
        label: text("onboarding.readiness.labels.ffprobe", "ffprobe"),
        status: componentStatus(components.ffprobe),
        message:
          components.ffprobe?.message ??
          text("onboarding.readiness.notChecked", "Not checked"),
        requiredForRecording: true,
      },
      {
        id: "nvenc",
        label: text(
          "onboarding.readiness.labels.nvenc",
          "NVIDIA NVENC support target",
        ),
        status: componentStatus(components.nvenc),
        message:
          components.nvenc?.message ??
          text("onboarding.readiness.notChecked", "Not checked"),
        requiredForRecording: true,
      },
      {
        id: "audio",
        label: text("onboarding.readiness.labels.audio", "System audio"),
        status: componentStatus(components.audio),
        message: components.audio.message,
        requiredForRecording: true,
      },
      {
        id: "disk",
        label: text("onboarding.readiness.labels.disk", "Storage space"),
        status: componentStatus(components.disk),
        message: components.disk.message,
        requiredForRecording: true,
      },
      {
        id: "lcu",
        label: text("onboarding.readiness.labels.lcu", "League Client / LCU"),
        status: componentStatus(components.lcu),
        message: components.lcu.message,
      },
    ];
    const services: Array<[keyof typeof components, string]> = [
      [
        "release_config",
        text(
          "onboarding.readiness.labels.releaseConfig",
          "Release configuration",
        ),
      ],
      [
        "supabase",
        text("onboarding.readiness.labels.supabase", "Free account service"),
      ],
      ["youtube", text("onboarding.readiness.labels.youtube", "YouTube")],
      ["updater", text("onboarding.readiness.labels.updater", "App updates")],
      [
        "telemetry",
        text(
          "onboarding.readiness.labels.telemetry",
          "Anonymous error telemetry (optional)",
        ),
      ],
      [
        "autostart",
        text("onboarding.readiness.labels.autostart", "Windows startup"),
      ],
      [
        "overlay_exclusion",
        text(
          "onboarding.readiness.labels.overlayExclusion",
          "Overlay capture exclusion",
        ),
      ],
    ];
    return [
      ...core,
      ...services.map(([id, label]) => {
        const component = components[id];
        return {
          id,
          label,
          status: componentStatus(component),
          message:
            component?.message ??
            text(
              "onboarding.readiness.unavailable",
              "Not available in this app build",
            ),
        };
      }),
    ];
  }, [snapshot, text]);

  const recordingReady = checks
    .filter((check) => check.requiredForRecording)
    .every((check) => check.status === "ok");
  const canComplete = recordingReady || warningAcknowledged;
  const progress =
    checks.length === 0
      ? 0
      : (checks.filter((check) => check.status === "ok").length /
          checks.length) *
        100;
  const complete = (completion: "passed" | "warnings_skipped") => {
    localStorage.setItem(
      ONBOARDING_KEY,
      JSON.stringify({
        version: ONBOARDING_VERSION,
        completedAt: new Date().toISOString(),
        completion,
      }),
    );
    setIsOpen(false);
  };
  const openSettings = () => window.location.assign("/settings");

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogContent
        className="max-h-[85vh] overflow-y-auto sm:max-w-2xl"
        data-testid="readiness-onboarding"
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-xl">
            <ShieldCheck className="h-5 w-5 text-gaming-cyan" />
            {text("onboarding.readiness.title", "Recording readiness")}
          </DialogTitle>
          <DialogDescription>
            {text(
              "onboarding.readiness.description",
              "Check this PC before your first recording. Recording and the clip library work without an account; editing, export, and YouTube use a free account.",
            )}
          </DialogDescription>
          <Progress value={progress} className="mt-3 h-1" />
        </DialogHeader>
        <section className="space-y-2" aria-live="polite">
          {checks.map((check) => (
            <div
              key={check.id}
              className="flex items-start gap-3 rounded-lg border border-white/10 bg-white/[0.02] p-3"
            >
              {check.status === "checking" ? (
                <Loader2 className="mt-0.5 h-4 w-4 animate-spin" />
              ) : check.status === "ok" ? (
                <Check className="mt-0.5 h-4 w-4 text-green-400" />
              ) : (
                <AlertTriangle className="mt-0.5 h-4 w-4 text-yellow-400" />
              )}
              <div className="min-w-0 flex-1">
                <p className="font-medium">{check.label}</p>
                <p className="text-sm text-muted-foreground">{check.message}</p>
              </div>
            </div>
          ))}
        </section>
        <section
          className="rounded-lg border border-white/10 p-3"
          data-testid="onboarding-storage-summary"
        >
          <div className="flex items-center gap-2 font-medium">
            <HardDrive className="h-4 w-4" />
            {text("onboarding.readiness.storagePlan", "Storage plan")}
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            {text(
              "onboarding.readiness.storageEstimate",
              "20 Mbps recording uses about {{hours}} GB per hour.",
              { hours: HOURLY_ESTIMATE_GB },
            )}
          </p>
          <p className="mt-1 text-sm text-muted-foreground">
            {text("onboarding.readiness.currentLibrary", "Current library:")}{" "}
            {bytesToGb(
              snapshot?.storage?.total_disk_usage_bytes ??
                snapshot?.storage?.total_size_bytes,
            )}{" "}
            · {text("onboarding.readiness.freeSpace", "Free space:")}{" "}
            {snapshot?.disk && snapshot.disk.known !== false
              ? `${snapshot.disk.available_gb.toFixed(1)} GB`
              : text("onboarding.readiness.unknown", "Unknown")}
          </p>
          {snapshot?.disk &&
            snapshot.disk.known !== false &&
            snapshot.disk.available_gb < HOURLY_ESTIMATE_GB && (
              <p className="mt-1 text-sm text-yellow-300">
                {text(
                  "onboarding.readiness.lowSpace",
                  "Less than one estimated hour is free. Choose a larger recording drive before a long session.",
                )}
              </p>
            )}
        </section>
        {snapshot?.diagnostics &&
          snapshot.diagnostics.overall_status !== "ok" && (
            <p className="text-sm text-muted-foreground">
              {text(
                "onboarding.readiness.diagnosticsNotice",
                "Diagnostics: {{status}}. Open Settings for the full repair guidance.",
                { status: snapshot.diagnostics.overall_status },
              )}
            </p>
          )}
        {snapshot?.autostart && !snapshot.autostart.configured && (
          <p className="text-sm text-muted-foreground">
            {text(
              "onboarding.readiness.autostartUnavailable",
              "Windows startup is unavailable: {{reason}}.",
              {
                reason:
                  snapshot.autostart.error_code ?? "configuration check failed",
              },
            )}
          </p>
        )}
        {!recordingReady && !isChecking && (
          <label className="flex cursor-pointer items-start gap-2 rounded-lg border border-yellow-500/30 bg-yellow-500/10 p-3 text-sm">
            <input
              type="checkbox"
              checked={warningAcknowledged}
              onChange={(event) => setWarningAcknowledged(event.target.checked)}
              className="mt-1"
            />
            <span>
              {text(
                "onboarding.readiness.warningAcknowledgement",
                "I understand recording is not ready yet and will fix these warnings in Settings before relying on capture.",
              )}
            </span>
          </label>
        )}
        <div className="flex flex-wrap justify-between gap-2">
          <Button
            variant="outline"
            onClick={() => void refresh()}
            disabled={isChecking}
            data-testid="onboarding-retry"
          >
            <RefreshCw className="mr-2 h-4 w-4" />
            {isChecking
              ? text("onboarding.readiness.checkingButton", "Checking…")
              : text("onboarding.readiness.retry", "Retry checks")}
          </Button>
          <div className="flex gap-2">
            <Button variant="outline" onClick={openSettings}>
              <Settings className="mr-2 h-4 w-4" />
              {text("onboarding.readiness.settings", "Settings & diagnostics")}
            </Button>
            <Button
              disabled={!canComplete || isChecking}
              onClick={() =>
                complete(recordingReady ? "passed" : "warnings_skipped")
              }
              data-testid="onboarding-complete"
            >
              {recordingReady
                ? text("onboarding.readiness.finish", "Finish setup")
                : text(
                    "onboarding.readiness.finishWarnings",
                    "Finish with warnings",
                  )}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function useOnboarding() {
  const resetOnboarding = () => {
    localStorage.removeItem(ONBOARDING_KEY);
    window.location.reload();
  };
  return {
    resetOnboarding,
    isOnboardingCompleted: () =>
      isCompletedRecord(localStorage.getItem(ONBOARDING_KEY)),
  };
}
