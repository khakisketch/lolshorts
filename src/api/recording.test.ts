import { recordingApi } from "./recording";
import { cmd } from "./client";

jest.mock("./client", () => ({
  cmd: jest.fn(),
}));

const mockCmd = jest.mocked(cmd);

describe("recordingApi readiness normalization", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("normalizes capture diagnostics omitted by an older backend to null", async () => {
    mockCmd.mockResolvedValueOnce({
      status: "idle",
      is_monitoring: false,
      buffer_duration_secs: 90,
    });

    await expect(recordingApi.getStatus()).resolves.toEqual({
      status: "idle",
      is_monitoring: false,
      buffer_duration_secs: 90,
      capture_mode: null,
      capture_backend: null,
      capture_warning: null,
    });
  });

  it("normalizes backend readiness components, severities, and defaults", async () => {
    mockCmd.mockResolvedValueOnce({
      ready: false,
      blockers: [
        {
          code: "ffmpeg_missing",
          component: "FFmpeg",
          message: "FFmpeg is missing",
          action: "Install FFmpeg",
        },
      ],
      warnings: [
        {
          code: "low_disk",
          component: "storage",
          message: "Disk space is low",
          action: "Free space",
        },
      ],
      components: [
        {
          component: "system-audio",
          status: "unknown",
          message: "Using fallback audio",
        },
        { component: "storage", status: "warning", message: "Only 3 GB free" },
        {
          component: "League Client Update",
          status: "offline",
          message: "LCU disconnected",
        },
        { component: "hardware encoder", status: "ok", message: "NVENC ready" },
        {
          component: "untracked-service",
          status: "error",
          message: "Ignored by UI",
        },
      ],
    });

    const readiness = await recordingApi.getRecordingReadiness();

    expect(mockCmd).toHaveBeenCalledWith("get_recording_readiness");
    expect(readiness).toEqual({
      ready: false,
      blockers: [
        {
          id: "ffmpeg_missing",
          component: "FFmpeg",
          message: "FFmpeg is missing",
          action: "Install FFmpeg",
          severity: "critical",
        },
        {
          id: "low_disk",
          component: "storage",
          message: "Disk space is low",
          action: "Free space",
          severity: "warning",
        },
      ],
      component_statuses: {
        ffmpeg: { status: "ok", message: "OK" },
        ffprobe: { status: "warning", message: "Not checked" },
        audio: { status: "warning", message: "Using fallback audio" },
        disk: { status: "warning", message: "Only 3 GB free" },
        lcu: { status: "error", message: "LCU disconnected" },
        gpu: { status: "ok", message: "NVENC ready" },
        nvenc: { status: "warning", message: "Not checked" },
        release_config: { status: "warning", message: "Not checked" },
        supabase: { status: "warning", message: "Not checked" },
        youtube: { status: "warning", message: "Not checked" },
        updater: { status: "warning", message: "Not checked" },
        telemetry: { status: "warning", message: "Not checked" },
        autostart: { status: "warning", message: "Not checked" },
      },
    });
  });

  it("normalizes replay target readiness state and snake-case selected target", async () => {
    mockCmd.mockResolvedValueOnce({
      status: "unknown-backend-state",
      candidates: [{ summoner_name: "Faker", champion_id: 103, team_id: 100 }],
      selected_target: "Faker",
      error: "Unexpected replay parser state",
    });

    await expect(recordingApi.getReplayTargetReadiness()).resolves.toEqual({
      state: "failed",
      candidates: [{ summoner_name: "Faker", champion_id: 103, team_id: 100 }],
      selectedTarget: "Faker",
      error: "Unexpected replay parser state",
      retryable: true,
    });
    expect(mockCmd).toHaveBeenCalledWith("get_replay_target_candidates");
  });

  it("keeps missing optional telemetry visible without treating it as a blocker", async () => {
    mockCmd.mockResolvedValueOnce({
      ready: true,
      blockers: [],
      warnings: [],
      components: [],
      public_services: {
        release_config: { configured: true },
        supabase: { configured: true },
        youtube: { configured: true },
        updater: { configured: true },
        telemetry: {
          configured: false,
          error_code: "TELEMETRY_OPTIONAL_NOT_CONFIGURED",
        },
      },
    });

    const readiness = await recordingApi.getRecordingReadiness();

    expect(readiness.ready).toBe(true);
    expect(readiness.blockers).toEqual([]);
    expect(readiness.component_statuses.telemetry).toEqual({
      status: "warning",
      message: "Optional: TELEMETRY_OPTIONAL_NOT_CONFIGURED",
    });
  });
});
