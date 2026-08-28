import {
  RECORDING_READINESS_POLL_INTERVAL_MS,
  useRecordingStore,
} from "./recordingStore";
import { recordingApi } from "../api/recording";

jest.mock("../api/recording", () => ({
  recordingApi: {
    getStatus: jest.fn(),
    getPerformanceStats: jest.fn(),
    getRecordingReadiness: jest.fn(),
    start: jest.fn(),
    stop: jest.fn(),
  },
  isRecording: (status: { status: string }) =>
    status.status === "recording" || status.status === "buffering",
}));

const mockRecordingApi = jest.mocked(recordingApi);
const backendStatus = {
  status: "idle" as const,
  is_monitoring: false,
  buffer_duration_secs: 90,
  capture_mode: null,
  capture_backend: null,
  capture_warning: null,
};
const readiness = {
  ready: true,
  blockers: [],
  warnings: [],
  component_statuses: {
    ffmpeg: { status: "ok" as const, message: "Ready" },
  },
};

describe("recording status polling", () => {
  beforeEach(() => {
    useRecordingStore.getState().stopStatusPolling();
    useRecordingStore.setState({
      readiness: null,
      error: null,
      _pollInterval: null,
      _readinessPollInterval: null,
    });
    jest.clearAllMocks();
    mockRecordingApi.getStatus.mockResolvedValue(backendStatus);
    mockRecordingApi.getRecordingReadiness.mockResolvedValue(readiness);
  });

  afterEach(() => {
    useRecordingStore.getState().stopStatusPolling();
    jest.useRealTimers();
  });

  it("keeps one-second status synchronization free of readiness probes", async () => {
    await useRecordingStore.getState().syncStatus();

    expect(mockRecordingApi.getStatus).toHaveBeenCalledTimes(1);
    expect(mockRecordingApi.getRecordingReadiness).not.toHaveBeenCalled();
  });

  it("refreshes readiness initially and then on the slower interval", async () => {
    jest.useFakeTimers();

    useRecordingStore.getState().startStatusPolling();
    await Promise.resolve();

    expect(mockRecordingApi.getRecordingReadiness).toHaveBeenCalledTimes(1);

    await jest.advanceTimersByTimeAsync(
      RECORDING_READINESS_POLL_INTERVAL_MS - 1,
    );
    expect(mockRecordingApi.getRecordingReadiness).toHaveBeenCalledTimes(1);

    await jest.advanceTimersByTimeAsync(1);
    expect(mockRecordingApi.getRecordingReadiness).toHaveBeenCalledTimes(2);
  });
});
