import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom";
import { RecordingControls } from "./RecordingControls";
import { useRecordingStore } from "../stores/recordingStore";
import type { RecordingSettings } from "../types";

// Mock Tauri API
const mockInvoke = jest.fn();
global.window.__TAURI__ = {
  invoke: mockInvoke,
  event: {
    listen: jest.fn(),
    emit: jest.fn(),
  },
};

// Mock toast
jest.mock("@/components/ui/use-toast", () => ({
  toast: jest.fn(),
}));

// Mock settings API
function createMockRecordingSettings(): RecordingSettings {
  return {
    schema_version: 4,
    video: {
      resolution: "r1920x1080",
      frame_rate: "fps60",
      bitrate_preset: "high",
      codec: "h264",
      encoder: "auto",
    },
    audio: {
      record_microphone: false,
      microphone_device: null,
      microphone_volume: 100,
      record_system_audio: true,
      system_audio_device: "default",
      system_audio_volume: 100,
      sample_rate: "hz48000",
      bitrate: "kbps192",
    },
    event_filter: {
      record_kills: true,
      record_multikills: true,
      record_first_blood: true,
      record_deaths: false,
      record_shutdown: true,
      record_assists: false,
      record_dragon: true,
      record_baron: true,
      record_elder: true,
      record_herald: true,
      record_turret: true,
      record_inhibitor: true,
      record_nexus: true,
      record_ace: true,
      record_game_end: true,
      record_steal: true,
      min_priority: 2,
    },
    game_mode: {
      record_ranked_solo: true,
      record_ranked_flex: true,
      record_normal: true,
      record_quick_play: true,
      record_aram: true,
      record_arena: true,
      record_special: true,
      record_custom: false,
      record_practice: false,
    },
    clip_timing: {
      default_pre_duration: 15,
      default_post_duration: 5,
      event_timings: {},
      merge_consecutive_events: true,
      merge_time_threshold: 10,
    },
    hotkeys: {
      toggle_recording: "F8",
      manual_save_clip: "F9",
      delete_last_clip: "F10",
    },
    storage: {
      auto_delete_enabled: false,
      auto_delete_days: 30,
      max_storage_gb: 50,
      delete_exported_clips: false,
    },
    launch_on_windows_startup: true,
    minimize_to_tray: true,
    show_notifications: true,
    show_replay_popup: true,
    crash_reporting_enabled: false,
    overlay_enabled: true,
  };
}

jest.mock("@/api/settings", () => ({
  settingsApi: {
    getRecordingSettings: jest
      .fn()
      .mockResolvedValue(createMockRecordingSettings()),
    saveRecordingSettings: jest.fn().mockResolvedValue(undefined),
  },
}));

// Mock recording API
jest.mock("@/api/recording", () => ({
  recordingApi: {
    startAutoCapture: jest.fn().mockResolvedValue(undefined),
    stopAutoCapture: jest.fn().mockResolvedValue(undefined),
    saveReplay: jest.fn().mockResolvedValue("/path/to/replay.mp4"),
  },
}));

// Mock store
jest.mock("@/stores/recordingStore", () => ({
  useRecordingStore: jest.fn(),
}));

const mockUseRecordingStore = jest.mocked(useRecordingStore);

describe("RecordingControls", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockUseRecordingStore.mockReturnValue({
      status: { isRecording: false },
      readiness: null,
    });
  });

  test("renders recording controls correctly", async () => {
    render(<RecordingControls />);

    // Wait for async effects to complete
    await waitFor(() => {
      // i18n mock returns keys, so we look for the i18n keys
      expect(
        screen.getByText("recordingControls.autoCapture.title"),
      ).toBeInTheDocument();
      expect(
        screen.getByText("recordingControls.manualReplay.title"),
      ).toBeInTheDocument();
      expect(
        screen.getByText("recordingControls.settings.title"),
      ).toBeInTheDocument();
    });
  });

  test("start auto capture button is present and clickable", async () => {
    render(<RecordingControls />);

    await waitFor(() => {
      const startButton = screen.getByText(
        "recordingControls.autoCapture.start",
      );
      // Verify button exists and is enabled
      expect(startButton).toBeInTheDocument();
      expect(startButton).not.toBeDisabled();
    });
  });

  test("only start button is visible initially", async () => {
    render(<RecordingControls />);

    await waitFor(() => {
      // Only start button should be visible initially
      expect(
        screen.getByText("recordingControls.autoCapture.start"),
      ).toBeInTheDocument();
      // Stop button should not be in DOM initially
      expect(
        screen.queryByText("recordingControls.autoCapture.stop"),
      ).not.toBeInTheDocument();
    });
  });

  test("save replay button is present", async () => {
    render(<RecordingControls />);

    await waitFor(() => {
      const saveButton = screen.getByText(
        "recordingControls.manualReplay.saveReplay",
      );
      expect(saveButton).toBeInTheDocument();
    });
  });

  test("save settings button is present", async () => {
    render(<RecordingControls />);

    await waitFor(() => {
      const saveSettingsButton = screen.getByText(
        "recordingControls.settings.saveSettings",
      );
      expect(saveSettingsButton).toBeInTheDocument();
    });
  });

  test("start auto capture button is disabled when blocked", async () => {
    mockUseRecordingStore.mockReturnValue({
      status: { isRecording: false },
      readiness: {
        ready: false,
        blockers: [
          { id: "ffmpeg", message: "FFmpeg not found", severity: "critical" },
        ],
        component_statuses: {
          ffmpeg: { status: "error", message: "Not found" },
          audio: { status: "ok", message: "OK" },
          disk: { status: "ok", message: "OK" },
          lcu: { status: "ok", message: "OK" },
          gpu: { status: "ok", message: "OK" },
        },
      },
    });

    render(<RecordingControls />);

    await waitFor(() => {
      const startButton = screen.getByText(
        "recordingControls.autoCapture.start",
      );
      expect(startButton).toBeDisabled();
      expect(
        screen.getByText("recordingControls.readiness.blocked"),
      ).toBeInTheDocument();
      expect(screen.getByText("FFmpeg not found")).toBeInTheDocument();
    });
  });

  test("start auto capture button is enabled when only warnings exist", async () => {
    mockUseRecordingStore.mockReturnValue({
      status: { isRecording: false },
      readiness: {
        ready: false,
        blockers: [
          { id: "disk", message: "Low disk space", severity: "warning" },
        ],
        component_statuses: {
          ffmpeg: { status: "ok", message: "OK" },
          audio: { status: "ok", message: "OK" },
          disk: { status: "warning", message: "Low space" },
          lcu: { status: "ok", message: "OK" },
          gpu: { status: "ok", message: "OK" },
        },
      },
    });

    render(<RecordingControls />);

    await waitFor(() => {
      const startButton = screen.getByText(
        "recordingControls.autoCapture.start",
      );
      expect(startButton).not.toBeDisabled();
      expect(
        screen.getByText("recordingControls.readiness.warning"),
      ).toBeInTheDocument();
      expect(screen.getByText("Low disk space")).toBeInTheDocument();
    });
  });
});
