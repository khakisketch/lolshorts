import { render, screen, waitFor } from "@testing-library/react";
import { AudioSettings } from "./AudioSettings";

// Mock i18n
jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

// Mock Tauri invoke (used to fetch microphone/system-audio devices on mount)
const mockInvoke = jest.fn();
jest.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const baseSettings = {
  record_microphone: false,
  microphone_device: null,
  microphone_volume: 100,
  record_system_audio: true,
  system_audio_device: null,
  system_audio_volume: 100,
  sample_rate: "hz48000" as const,
  bitrate: "kbps192" as const,
};

describe("AudioSettings", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((command: string) => {
      if (command === "list_microphone_devices")
        return Promise.resolve(["Mic 1", "Mic 2"]);
      if (command === "list_system_audio_devices")
        return Promise.resolve(["Speakers", "Headset"]);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
  });

  it("fetches device lists via list_microphone_devices and list_system_audio_devices", async () => {
    const onChange = jest.fn();
    render(<AudioSettings settings={baseSettings} onChange={onChange} />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("list_microphone_devices");
      expect(mockInvoke).toHaveBeenCalledWith("list_system_audio_devices");
    });

    // No "devices not found" warnings once both lists resolve with entries.
    await waitFor(() => {
      expect(
        screen.queryByText("errors.audioNoDevicesAvailable"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByText("errors.audioDeviceNotFoundHint"),
      ).not.toBeInTheDocument();
    });
  });

  it("enables the microphone toggle, device select, and volume slider", async () => {
    const onChange = jest.fn();
    render(<AudioSettings settings={baseSettings} onChange={onChange} />);

    const micSwitch = await screen.findByRole("switch", {
      name: "settings.recordingConfig.audioSettings.microphoneRecording.enableMicrophone",
    });
    expect(micSwitch).not.toBeDisabled();
    expect(micSwitch).toHaveAttribute("aria-checked", "false");

    const micSelect = screen.getByRole("combobox", {
      name: "settings.recordingConfig.audioSettings.microphoneRecording.microphoneDevice",
    });
    expect(micSelect).not.toBeDisabled();

    const sliders = screen.getAllByRole("slider");
    expect(sliders[0]).not.toHaveAttribute("data-disabled");
  });

  it('shows a mixing notice instead of the old "coming soon" gate', async () => {
    const onChange = jest.fn();
    render(<AudioSettings settings={baseSettings} onChange={onChange} />);

    await waitFor(() => {
      expect(
        screen.getByText(
          "settings.recordingConfig.audioSettings.microphoneRecording.mixingNotice",
        ),
      ).toBeInTheDocument();
    });

    expect(
      screen.queryByText(
        "settings.recordingConfig.audioSettings.microphoneRecording.comingSoon",
      ),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByText(
        "settings.recordingConfig.audioSettings.microphoneRecording.disabledNotice",
      ),
    ).not.toBeInTheDocument();
  });

  it("still allows configuring system audio", async () => {
    const onChange = jest.fn();
    render(<AudioSettings settings={baseSettings} onChange={onChange} />);

    const systemSwitch = await screen.findByRole("switch", {
      name: "settings.recordingConfig.audioSettings.systemAudioRecording.enableSystemAudio",
    });
    expect(systemSwitch).not.toBeDisabled();
  });

  it("shows a warning when no audio devices are found at all", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "list_microphone_devices") return Promise.resolve([]);
      if (command === "list_system_audio_devices") return Promise.resolve([]);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    const onChange = jest.fn();
    render(<AudioSettings settings={baseSettings} onChange={onChange} />);

    await waitFor(() => {
      expect(
        screen.getByText("errors.audioNoDevicesAvailable"),
      ).toBeInTheDocument();
    });
  });
});
