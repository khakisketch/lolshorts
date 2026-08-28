import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { OnboardingModal } from "./OnboardingModal";

const mockGetRecordingReadiness = jest.fn();
const mockGetDiagnosticsStatus = jest.fn();
const mockGetDiskSpaceInfo = jest.fn();
const mockGetStorageStats = jest.fn();
const mockGetAutostartStatus = jest.fn();

jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (
      _key: string,
      options?: string | { defaultValue?: string; [key: string]: unknown },
    ) =>
      (() => {
        if (typeof options === "string") return options;
        const fallback = options?.defaultValue ?? _key;
        return fallback.replace(/\{\{(\w+)\}\}/g, (_, key: string) =>
          String(options?.[key] ?? `{{${key}}}`),
        );
      })(),
  }),
}));
jest.mock("@/api/recording", () => ({
  recordingApi: {
    getRecordingReadiness: (...args: unknown[]) =>
      mockGetRecordingReadiness(...args),
  },
}));
jest.mock("@/api/utils", () => ({
  utilsApi: {
    getDiagnosticsStatus: (...args: unknown[]) =>
      mockGetDiagnosticsStatus(...args),
    getDiskSpaceInfo: (...args: unknown[]) => mockGetDiskSpaceInfo(...args),
  },
}));
jest.mock("@/api/storage", () => ({
  storageApi: {
    getStorageStats: (...args: unknown[]) => mockGetStorageStats(...args),
  },
}));
jest.mock("@/api/settings", () => ({
  settingsApi: {
    getAutostartStatus: (...args: unknown[]) => mockGetAutostartStatus(...args),
  },
}));

const readiness = (
  status: "ok" | "warning" | "error" = "ok",
  telemetryStatus: "ok" | "warning" | "error" = "ok",
) => ({
  ready: status === "ok",
  blockers: [],
  component_statuses: {
    ffmpeg: { status, message: "ffmpeg" },
    ffprobe: { status, message: "ffprobe" },
    gpu: { status, message: "gpu" },
    nvenc: { status, message: "nvenc" },
    audio: { status, message: "audio" },
    disk: { status, message: "disk" },
    lcu: { status: "warning", message: "League is not running" },
    release_config: { status: "ok", message: "configured" },
    supabase: { status: "ok", message: "configured" },
    youtube: { status: "ok", message: "configured" },
    updater: { status: "ok", message: "configured" },
    autostart: { status: "ok", message: "configured" },
    telemetry: { status: telemetryStatus, message: "telemetry" },
  },
});

beforeEach(() => {
  localStorage.clear();
  jest.useFakeTimers();
  mockGetRecordingReadiness.mockResolvedValue(readiness());
  mockGetDiagnosticsStatus.mockResolvedValue({
    overall_status: "ok",
    checks: [],
  });
  mockGetDiskSpaceInfo.mockResolvedValue({
    known: true,
    available_gb: 80,
    total_gb: 100,
    used_gb: 20,
  });
  mockGetStorageStats.mockResolvedValue({
    total_games: 1,
    total_clips: 2,
    total_size_bytes: 1024 ** 3,
  });
  mockGetAutostartStatus.mockResolvedValue({
    configured: true,
    enabled: false,
    error_code: null,
  });
});

afterEach(() => jest.useRealTimers());

describe("OnboardingModal readiness wizard", () => {
  it("checks the readiness, diagnostics, disk, library, and autostart APIs before completion", async () => {
    render(<OnboardingModal />);
    await act(async () => {
      jest.advanceTimersByTime(500);
    });

    await screen.findByTestId("readiness-onboarding");
    expect(mockGetRecordingReadiness).toHaveBeenCalledTimes(1);
    expect(mockGetDiagnosticsStatus).toHaveBeenCalledTimes(1);
    expect(mockGetDiskSpaceInfo).toHaveBeenCalledTimes(1);
    expect(mockGetStorageStats).toHaveBeenCalledTimes(1);
    expect(mockGetAutostartStatus).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("onboarding-storage-summary")).toHaveTextContent(
      "9 GB per hour",
    );

    fireEvent.click(screen.getByTestId("onboarding-complete"));
    await waitFor(() =>
      expect(localStorage.getItem("lolshorts_onboarding_completed")).toContain(
        '"version":2',
      ),
    );
  });

  it("requires explicit acknowledgement before completing a failing capture check", async () => {
    mockGetRecordingReadiness.mockResolvedValue(readiness("warning"));
    render(<OnboardingModal />);
    await act(async () => {
      jest.advanceTimersByTime(500);
    });
    const complete = await screen.findByTestId("onboarding-complete");
    expect(complete).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    expect(complete).toBeEnabled();
  });

  it("does not require optional telemetry for a recording-ready setup", async () => {
    mockGetRecordingReadiness.mockResolvedValue(readiness("ok", "warning"));
    render(<OnboardingModal />);
    await act(async () => {
      jest.advanceTimersByTime(500);
    });

    const complete = await screen.findByTestId("onboarding-complete");
    expect(complete).toBeEnabled();
    expect(
      screen.getByText("Anonymous error telemetry (optional)"),
    ).toBeInTheDocument();
  });

  it("shows an unknown disk measurement without a false low-space warning", async () => {
    mockGetDiskSpaceInfo.mockResolvedValue({
      known: false,
      available_gb: 0,
      total_gb: 0,
      used_gb: 0,
    });

    render(<OnboardingModal />);
    await act(async () => {
      jest.advanceTimersByTime(500);
    });

    const storage = await screen.findByTestId("onboarding-storage-summary");
    expect(storage).toHaveTextContent("Free space: Unknown");
    expect(storage).not.toHaveTextContent(
      "Less than one estimated hour is free",
    );
  });
});
