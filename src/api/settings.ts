import { cmd } from "./client";
import {
  PlatformConfig,
  RecordingSettings,
  RecommendedSettings,
} from "@/types";

export interface AutostartStatus {
  configured: boolean;
  enabled: boolean;
  error_code: string | null;
}

export const settingsApi = {
  getRecordingSettings: () => cmd<RecordingSettings>("get_recording_settings"),

  saveRecordingSettings: (settings: RecordingSettings) =>
    cmd<void>("save_recording_settings", { settings }),

  resetToDefault: () => cmd<void>("reset_settings_to_default"),

  getAutostartStatus: () => cmd<AutostartStatus>("get_autostart_status"),

  setLaunchOnWindowsStartup: (enabled: boolean) =>
    cmd<AutostartStatus>("set_launch_on_windows_startup", { enabled }),

  // Platform optimization
  detectPlatformConfig: () => cmd<PlatformConfig>("detect_platform_config"),
  getRecommendedSettings: () =>
    cmd<RecommendedSettings>("get_recommended_settings"),
};
