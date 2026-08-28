import { cmd } from "./client";
import {
  RecordingReadiness,
  ReadinessComponent,
  ReadinessBlocker,
} from "@/types";
import { PlayerInfo } from "./lcu";

// Backend response types for recording readiness
interface BackendReadinessIssue {
  code: string;
  component: string;
  message: string;
  action: string;
}

interface BackendComponentStatus {
  component: string;
  status: string;
  message: string;
}

interface BackendReadiness {
  ready: boolean;
  blockers?: BackendReadinessIssue[];
  warnings?: BackendReadinessIssue[];
  components?: BackendComponentStatus[];
  public_services?: {
    release_config: BackendConfigurationStatus;
    supabase: BackendConfigurationStatus;
    youtube: BackendConfigurationStatus;
    updater: BackendConfigurationStatus;
    telemetry: BackendConfigurationStatus;
  } | null;
  autostart?: {
    configured: boolean;
    enabled: boolean;
    error_code?: string | null;
  } | null;
}

interface BackendConfigurationStatus {
  configured: boolean;
  error_code?: string | null;
}

interface BackendReplayTargetCandidates {
  status: string;
  candidates?: PlayerInfo[];
  selected_target?: string | null;
  error?: string | null;
  retryable?: boolean;
}

type ReadinessComponentKey = keyof RecordingReadiness["component_statuses"];

const normalizeReadinessComponentKey = (
  component: string,
): ReadinessComponentKey | null => {
  const normalized = component
    .trim()
    .toLowerCase()
    .replace(/[-\s]+/g, "_");

  if (normalized === "ffmpeg") return "ffmpeg";
  if (normalized === "ffprobe") return "ffprobe";
  if (
    normalized === "audio" ||
    normalized === "microphone" ||
    normalized === "system_audio"
  )
    return "audio";
  if (normalized === "disk" || normalized === "storage") return "disk";
  if (
    normalized === "lcu" ||
    normalized === "league_client" ||
    normalized === "league_client_update"
  )
    return "lcu";
  if (
    normalized === "gpu" ||
    normalized === "encoder" ||
    normalized === "hardware_encoder"
  )
    return "gpu";
  if (normalized === "support_matrix" || normalized === "nvenc") return "nvenc";
  if (normalized === "capture") return "capture";
  if (normalized === "overlay_exclusion") return "overlay_exclusion";

  return null;
};

const normalizeReadinessComponentStatus = (
  status: string,
): ReadinessComponent["status"] => {
  const normalized = status.trim().toLowerCase();

  if (normalized === "ok") return "ok";
  if (normalized === "warning" || normalized === "unknown") return "warning";
  return "error";
};

const normalizeReplayTargetState = (
  status: string,
): ReplayTargetReadiness["state"] => {
  if (
    status === "unavailable" ||
    status === "loading" ||
    status === "empty" ||
    status === "ready" ||
    status === "failed"
  ) {
    return status;
  }

  return "failed";
};

// Matches Rust RecordingStatusInfo struct
export type CaptureMode =
  | "hwnd_desktop_region"
  | "title_window"
  | "desktop_fallback"
  | "monitor_desktop";

export type CaptureBackend = "desktop_duplication" | "gdi_grab";

export interface RecordingStatus {
  status: "idle" | "buffering" | "recording" | "processing" | "error";
  is_monitoring: boolean;
  buffer_duration_secs: number;
  capture_mode: CaptureMode | null;
  capture_backend: CaptureBackend | null;
  capture_warning: string | null;
}

// Helper computed properties (for backward compatibility)
export function isRecording(status: RecordingStatus): boolean {
  return status.status === "recording" || status.status === "buffering";
}

export function normalizeRecordingStatus(
  status: RecordingStatus,
): RecordingStatus {
  return {
    ...status,
    capture_mode: status.capture_mode ?? null,
    capture_backend: status.capture_backend ?? null,
    capture_warning: status.capture_warning ?? null,
  };
}

export interface RecordingMetrics {
  fps: number;
  frame_drops: number;
  bitrate_kbps: number;
  cpu_percent: number;
  memory_mb: number;
  buffer_segments: number;
  buffer_size_mb: number;
}

export interface ReplayTargetReadiness {
  state: "unavailable" | "loading" | "empty" | "ready" | "failed";
  candidates: PlayerInfo[];
  selectedTarget: string | null;
  error?: string;
  retryable: boolean;
}

export type AudioDeviceType = "Microphone" | "SystemAudio";

export interface AudioDevice {
  name: string;
  device_type: AudioDeviceType;
}

export interface PerformanceStats {
  recording: {
    total_frames: number;
    uptime_seconds: number;
    current_fps: number;
    /** False if system audio capture is failing during an active recording. */
    audio_active: boolean;
    /** False if microphone capture is failing during an active recording. */
    mic_active: boolean;
  };
  system: {
    status: string;
  };
}

export const recordingApi = {
  // Core Controls
  start: () => cmd<void>("start_recording"),
  stop: () => cmd<void>("stop_recording"),

  // Status & Metrics
  getStatus: () =>
    cmd<RecordingStatus>("get_detailed_recording_status").then(
      normalizeRecordingStatus,
    ),
  getMetrics: () => cmd<RecordingMetrics>("get_recording_metrics"),
  getPerformanceStats: () => cmd<PerformanceStats>("get_performance_stats"),
  getRecordingReadiness: async () => {
    const raw = await cmd<BackendReadiness>("get_recording_readiness");

    // Map backend components array to UI object
    const component_statuses: RecordingReadiness["component_statuses"] = {
      ffmpeg: { status: "ok", message: "OK" },
      ffprobe: { status: "warning", message: "Not checked" },
      audio: { status: "ok", message: "OK" },
      disk: { status: "ok", message: "OK" },
      lcu: { status: "ok", message: "OK" },
      gpu: { status: "ok", message: "OK" },
      nvenc: { status: "warning", message: "Not checked" },
      release_config: { status: "warning", message: "Not checked" },
      supabase: { status: "warning", message: "Not checked" },
      youtube: { status: "warning", message: "Not checked" },
      updater: { status: "warning", message: "Not checked" },
      telemetry: { status: "warning", message: "Not checked" },
      autostart: { status: "warning", message: "Not checked" },
    };

    (raw.components ?? []).forEach((c) => {
      const key = normalizeReadinessComponentKey(c.component);
      if (!key) return;

      component_statuses[key] = {
        status: normalizeReadinessComponentStatus(c.status),
        message: c.message,
      };
    });

    const assignPublicStatus = (
      key: "release_config" | "supabase" | "youtube" | "updater" | "telemetry",
      status?: BackendConfigurationStatus,
      optional = false,
    ) => {
      if (!status) return;
      component_statuses[key] = {
        status: status.configured ? "ok" : "warning",
        message: status.configured
          ? "Configured"
          : `${optional ? "Optional: " : ""}${status.error_code ?? "Configuration missing"}`,
      };
    };
    assignPublicStatus("release_config", raw.public_services?.release_config);
    assignPublicStatus("supabase", raw.public_services?.supabase);
    assignPublicStatus("youtube", raw.public_services?.youtube);
    assignPublicStatus("updater", raw.public_services?.updater);
    assignPublicStatus("telemetry", raw.public_services?.telemetry, true);
    if (raw.autostart) {
      component_statuses.autostart = {
        status: raw.autostart.configured ? "ok" : "warning",
        message: raw.autostart.configured
          ? raw.autostart.enabled
            ? "Enabled"
            : "Disabled"
          : (raw.autostart.error_code ?? "Autostart unavailable"),
      };
    }

    // Combine blockers and warnings into a single UI blockers list
    const blockers: ReadinessBlocker[] = [
      ...(raw.blockers ?? []).map((b) => ({
        id: b.code,
        component: b.component,
        message: b.message,
        action: b.action,
        severity: "critical" as const,
      })),
      ...(raw.warnings ?? []).map((w) => ({
        id: w.code,
        component: w.component,
        message: w.message,
        action: w.action,
        severity: "warning" as const,
      })),
    ];

    return {
      ready: raw.ready,
      blockers,
      component_statuses,
    };
  },

  // Configuration
  listAudioDevices: () => cmd<AudioDevice[]>("list_audio_devices"),
  refreshAudioDevices: () => cmd<AudioDevice[]>("refresh_audio_devices"),

  // Auto Capture
  startAutoCapture: () => cmd<void>("start_auto_capture"),
  stopAutoCapture: () => cmd<void>("stop_auto_capture"),

  getReplayTargetReadiness: async () => {
    const response = await cmd<BackendReplayTargetCandidates>(
      "get_replay_target_candidates",
    );
    const state = normalizeReplayTargetState(response.status);

    return {
      state,
      candidates: response.candidates ?? [],
      selectedTarget: response.selected_target ?? null,
      error: response.error ?? undefined,
      retryable:
        response.retryable ??
        (state === "loading" || state === "unavailable" || state === "failed"),
    };
  },

  saveReplay: (duration_secs: number) =>
    cmd<string>("save_replay", { duration_secs }),
};
