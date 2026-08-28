import { videoApi } from "./video";
import { cmd } from "./client";

jest.mock("./client", () => ({
  cmd: jest.fn(),
}));

const mockCmd = jest.mocked(cmd);

describe("videoApi AutoEdit normalization", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("returns the asynchronous auto-edit job receipt immediately", async () => {
    mockCmd.mockResolvedValueOnce({
      job_id: "job-42",
      status: "queued",
    });

    await expect(
      videoApi.startAutoEdit({
        game_ids: ["game-1"],
        target_duration: 60,
      }),
    ).resolves.toEqual({
      job_id: "job-42",
      status: "Queued",
    });

    expect(mockCmd).toHaveBeenCalledWith("start_auto_edit", {
      config: {
        game_ids: ["game-1"],
        target_duration: 60,
      },
    });
  });

  it("normalizes backend AutoEdit progress status and progress fields", async () => {
    mockCmd.mockResolvedValueOnce({
      job_id: "auto_edit_20260425_120000",
      status: "completed",
      progress: 100,
      current_step: "Complete",
      estimated_seconds: 0,
      output_path: "C:/clips/auto-edit.mp4",
      outputs: [
        {
          result_id: "result-42",
          output_path: "C:/clips/auto-edit.mp4",
          duration: 58.8,
          clips_used: 2,
          file_size_bytes: 1234,
          output_kind: "short",
        },
      ],
    });

    await expect(
      videoApi.getAutoEditProgress("auto_edit_20260425_120000"),
    ).resolves.toEqual({
      job_id: "auto_edit_20260425_120000",
      status: "Complete",
      progress_percentage: 100,
      current_stage: "Complete",
      clips_selected: 0,
      total_clips: 0,
      estimated_completion_seconds: 0,
      output_path: "C:/clips/auto-edit.mp4",
      outputs: [
        {
          result_id: "result-42",
          output_path: "C:/clips/auto-edit.mp4",
          duration: 58.8,
          clips_used: 2,
          file_size_bytes: 1234,
          output_kind: "short",
          part_index: undefined,
          part_count: undefined,
        },
      ],
    });

    expect(mockCmd).toHaveBeenCalledWith("get_auto_edit_progress", {
      job_id: "auto_edit_20260425_120000",
    });
  });

  it("plans and cancels by job id through the lifecycle contract", async () => {
    mockCmd
      .mockResolvedValueOnce({
        clips: [],
        estimated_duration_secs: 70,
        recommended_output_intent: "shorts_series",
        estimated_part_count: 2,
      })
      .mockResolvedValueOnce({
        job_id: "job-42",
        status: "cancelled",
        outputs: [],
      });

    await expect(
      videoApi.planAutoEdit({ game_ids: ["game-1"], target_duration: 60 }),
    ).resolves.toMatchObject({
      recommended_output_intent: "shorts_series",
    });
    await expect(videoApi.cancelAutoEdit("job-42")).resolves.toMatchObject({
      job_id: "job-42",
      status: "Cancelled",
    });

    expect(mockCmd).toHaveBeenNthCalledWith(1, "plan_auto_edit", {
      config: { game_ids: ["game-1"], target_duration: 60 },
    });
    expect(mockCmd).toHaveBeenNthCalledWith(2, "cancel_auto_edit", {
      job_id: "job-42",
    });
  });
});
