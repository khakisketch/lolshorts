import { render, screen, waitFor } from "@testing-library/react";
import { VideoSettings } from "./VideoSettings";

// Mock i18n
jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

// Mock Tauri invoke (used to detect available encoders on mount)
jest.mock("@tauri-apps/api/core", () => ({
  invoke: jest.fn().mockResolvedValue({
    available: [],
    auto_detected: "software",
    total_count: 0,
  }),
}));

const baseSettings = {
  resolution: "r1920x1080" as const,
  frame_rate: "fps60" as const,
  bitrate_preset: "medium" as const,
  codec: "h265" as const,
  encoder: "auto" as const,
};

describe("VideoSettings", () => {
  const originalUserAgent = navigator.userAgent;

  afterEach(() => {
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: originalUserAgent,
    });
  });

  it("explains that capture uses the native game resolution regardless of this setting", async () => {
    const onChange = jest.fn();
    render(<VideoSettings settings={baseSettings} onChange={onChange} />);

    await waitFor(() => {
      expect(
        screen.getByText(
          "settings.recordingConfig.videoSettings.resolution.captureNotice",
        ),
      ).toBeInTheDocument();
    });
  });

  it("disables the compatibility resolution field on Windows and reports native window size", async () => {
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
    });

    render(<VideoSettings settings={baseSettings} onChange={jest.fn()} />);

    expect(await screen.findByTestId("advanced-resolution")).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(screen.getByTestId("native-window-size")).toHaveTextContent(
      "settings.recordingConfig.videoSettings.resolution.nativeWindowSize",
    );
  });
});
