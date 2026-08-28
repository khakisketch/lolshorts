import { act, renderHook, waitFor } from "@testing-library/react";
import { useAutoEdit } from "./useAutoEdit";
import { videoApi } from "@/api/video";
import { useAutoEditStore } from "@/stores/autoEditStore";

jest.mock("@/api/video", () => ({
  videoApi: {
    planAutoEdit: jest.fn(),
    startAutoEdit: jest.fn(),
    getAutoEditProgress: jest.fn(),
    cancelAutoEdit: jest.fn(),
    saveCanvasTemplate: jest.fn(),
    loadCanvasTemplate: jest.fn(),
    listCanvasTemplates: jest.fn(),
    deleteCanvasTemplate: jest.fn(),
  },
}));

const mockVideoApi = videoApi as jest.Mocked<typeof videoApi>;

describe("useAutoEdit job lifecycle", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    useAutoEditStore.getState().resetAll();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it("stores queued progress, immediately polls by job id, and maps the first completed output", async () => {
    mockVideoApi.startAutoEdit.mockResolvedValue({
      job_id: "job-42",
      status: "Queued",
    });
    mockVideoApi.getAutoEditProgress.mockResolvedValue({
      job_id: "job-42",
      status: "Complete",
      progress_percentage: 100,
      current_stage: "Completed",
      clips_selected: 3,
      total_clips: 3,
      outputs: [
        {
          result_id: "result-42",
          output_path: "C:/exports/first.mp4",
          duration: 59.5,
          clips_used: 3,
          file_size_bytes: 4096,
          output_kind: "short",
        },
      ],
    });

    const { result } = renderHook(() => useAutoEdit());
    let receipt;
    await act(async () => {
      receipt = await result.current.startAutoEdit({
        game_ids: ["game-1"],
        target_duration: 60,
      });
    });

    expect(receipt).toEqual({ job_id: "job-42", status: "Queued" });
    await waitFor(() =>
      expect(mockVideoApi.getAutoEditProgress).toHaveBeenCalledWith("job-42"),
    );
    expect(useAutoEditStore.getState().result).toEqual({
      job_id: "result-42",
      output_path: "C:/exports/first.mp4",
      duration: 59.5,
      clips_used: 3,
      file_size_bytes: 4096,
      outputs: [
        {
          result_id: "result-42",
          output_path: "C:/exports/first.mp4",
          duration: 59.5,
          clips_used: 3,
          file_size_bytes: 4096,
          output_kind: "short",
        },
      ],
    });
  });

  it("exposes planning and cancellation against the active job", async () => {
    mockVideoApi.planAutoEdit.mockResolvedValue({
      clips: [],
      estimated_duration_secs: 60,
      recommended_output_intent: "single_short",
      estimated_part_count: 1,
    });
    mockVideoApi.cancelAutoEdit.mockResolvedValue({
      job_id: "job-42",
      status: "Cancelled",
      progress_percentage: 0,
      current_stage: "Cancelled",
      clips_selected: 0,
      total_clips: 0,
      outputs: [],
    });
    useAutoEditStore.getState().setJobId("job-42");

    const { result } = renderHook(() => useAutoEdit());
    await expect(
      result.current.planAutoEdit({
        game_ids: ["game-1"],
        target_duration: 60,
      }),
    ).resolves.toMatchObject({
      estimated_part_count: 1,
    });
    await act(async () => {
      await result.current.cancelAutoEdit();
    });

    expect(mockVideoApi.cancelAutoEdit).toHaveBeenCalledWith("job-42");
    expect(useAutoEditStore.getState().progress?.status).toBe("Cancelled");
  });

  it("surfaces a terminal background failure through the shared error state", async () => {
    mockVideoApi.getAutoEditProgress.mockResolvedValue({
      job_id: "job-failed",
      status: "Failed",
      progress_percentage: 37,
      current_stage: "Failed",
      clips_selected: 2,
      total_clips: 4,
      outputs: [],
      error: "FFmpeg exited while composing part 2",
    });

    const { result } = renderHook(() => useAutoEdit());
    await act(async () => {
      await result.current.pollProgress("job-failed");
    });

    expect(result.current.error).toBe("FFmpeg exited while composing part 2");
    expect(useAutoEditStore.getState().error).toMatchObject({
      error_type: "ProcessingError",
      message: "FFmpeg exited while composing part 2",
    });
  });

  it("backs off polls at one then two seconds and stops after a terminal update", async () => {
    jest.useFakeTimers();
    mockVideoApi.getAutoEditProgress
      .mockResolvedValueOnce({
        job_id: "job-42",
        status: "Queued",
        progress_percentage: 0,
        current_stage: "Queued",
        clips_selected: 0,
        total_clips: 0,
        outputs: [],
      })
      .mockResolvedValueOnce({
        job_id: "job-42",
        status: "Queued",
        progress_percentage: 10,
        current_stage: "Selecting",
        clips_selected: 1,
        total_clips: 3,
        outputs: [],
      })
      .mockResolvedValueOnce({
        job_id: "job-42",
        status: "Cancelled",
        progress_percentage: 10,
        current_stage: "Cancelled",
        clips_selected: 1,
        total_clips: 3,
        outputs: [],
      });

    const { result } = renderHook(() => useAutoEdit());
    act(() => result.current.startProgressPolling("job-42"));
    await act(async () => {
      await Promise.resolve();
    });
    expect(mockVideoApi.getAutoEditProgress).toHaveBeenCalledTimes(1);

    await act(async () => {
      jest.advanceTimersByTime(1000);
      await Promise.resolve();
    });
    expect(mockVideoApi.getAutoEditProgress).toHaveBeenCalledTimes(2);
    await act(async () => {
      jest.advanceTimersByTime(2000);
      await Promise.resolve();
    });
    expect(mockVideoApi.getAutoEditProgress).toHaveBeenCalledTimes(3);
    act(() => jest.advanceTimersByTime(8000));
    expect(mockVideoApi.getAutoEditProgress).toHaveBeenCalledTimes(3);
    jest.useRealTimers();
  });
});
