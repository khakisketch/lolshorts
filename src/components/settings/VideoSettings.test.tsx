import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { VideoSettings } from "./VideoSettings";

// Radix Select needs pointer-capture / scrollIntoView APIs jsdom lacks.
beforeAll(() => {
  window.HTMLElement.prototype.scrollIntoView = jest.fn();
  Object.assign(window.HTMLElement.prototype, {
    hasPointerCapture: jest.fn(() => false),
    setPointerCapture: jest.fn(),
    releasePointerCapture: jest.fn(),
  });
});

const CODEC = {
  av1: "settings.recordingConfig.videoSettings.videoCodec.labels.av1",
  h264: "settings.recordingConfig.videoSettings.videoCodec.labels.h264",
  h265: "settings.recordingConfig.videoSettings.videoCodec.labels.h265",
  warning:
    "settings.recordingConfig.videoSettings.videoCodec.h265PreviewWarning",
} as const;

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

  it("offers only H.264 and H.265 in the codec picker (G008)", async () => {
    render(<VideoSettings settings={baseSettings} onChange={jest.fn()} />);

    const codecTrigger = await screen.findByTestId("advanced-codec");
    fireEvent.click(codecTrigger);

    const options = await screen.findAllByRole("option");
    const labels = options.map((o) => o.textContent);
    expect(labels).toEqual([CODEC.h264, CODEC.h265]);
    expect(labels).not.toContain(CODEC.av1);
  });

  it("warns that H.265 clips may not preview in the app (G008)", async () => {
    render(<VideoSettings settings={baseSettings} onChange={jest.fn()} />);

    expect(await screen.findByText(CODEC.warning)).toBeInTheDocument();
  });

  it("falls back a legacy AV1 setting to H.264 (G008)", async () => {
    const onChange = jest.fn();
    render(
      <VideoSettings
        settings={{ ...baseSettings, codec: "av1" as never }}
        onChange={onChange}
      />,
    );

    await waitFor(() =>
      expect(onChange).toHaveBeenCalledWith(
        expect.objectContaining({ codec: "h264" }),
      ),
    );
    // The removed AV1 label must never reach the screen.
    expect(screen.queryByText(CODEC.av1)).not.toBeInTheDocument();
    // The H.265-only preview warning is not shown for the H.264 fallback.
    expect(screen.queryByText(CODEC.warning)).not.toBeInTheDocument();
  });
});
