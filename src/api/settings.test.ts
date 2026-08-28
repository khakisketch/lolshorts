import { cmd } from "./client";
import { settingsApi } from "./settings";

jest.mock("./client", () => ({
  cmd: jest.fn().mockResolvedValue(undefined),
}));

const mockedCmd = cmd as jest.MockedFunction<typeof cmd>;

describe("settings API contract", () => {
  beforeEach(() => {
    mockedCmd.mockClear();
  });

  it("maps each settings command to its backend contract", async () => {
    await settingsApi.getRecordingSettings();
    await settingsApi.saveRecordingSettings({} as never);
    await settingsApi.resetToDefault();
    await settingsApi.getAutostartStatus();
    await settingsApi.setLaunchOnWindowsStartup(true);
    await settingsApi.detectPlatformConfig();
    await settingsApi.getRecommendedSettings();

    expect(mockedCmd.mock.calls).toEqual([
      ["get_recording_settings"],
      ["save_recording_settings", { settings: {} }],
      ["reset_settings_to_default"],
      ["get_autostart_status"],
      ["set_launch_on_windows_startup", { enabled: true }],
      ["detect_platform_config"],
      ["get_recommended_settings"],
    ]);
  });
});
