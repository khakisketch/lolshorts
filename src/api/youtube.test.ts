import { youtubeApi } from "./youtube";
import { cmd } from "./client";

jest.mock("./client", () => ({
  cmd: jest.fn(),
}));

const mockCmd = jest.mocked(cmd);

describe("youtubeApi scheduled upload normalization", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("normalizes scheduled upload responses with default status and error aliases", async () => {
    mockCmd.mockResolvedValueOnce({
      id: "scheduled-1",
      video_path: "C:/clips/penta.mp4",
      title: "Pentakill",
      description: "A scheduled clip",
      tags: ["lol", "shorts"],
      privacy_status: "private",
      thumbnail_path: null,
      schedule: { scheduled_at: "2026-04-26T09:00:00.000Z" },
      created_at: 1777194000,
      status: null,
      error_message: "Quota temporarily exhausted",
    });

    const result = await youtubeApi.scheduleUpload(
      "C:/clips/penta.mp4",
      "Pentakill",
      "A scheduled clip",
      ["lol", "shorts"],
      "private",
      undefined,
      "2026-04-26T09:00:00.000Z",
    );

    expect(mockCmd).toHaveBeenCalledWith("youtube_schedule_upload", {
      params: {
        video_path: "C:/clips/penta.mp4",
        title: "Pentakill",
        description: "A scheduled clip",
        tags: ["lol", "shorts"],
        privacy_status: "private",
        thumbnail_path: null,
        scheduled_at: "2026-04-26T09:00:00.000Z",
      },
    });
    expect(result).toMatchObject({
      id: "scheduled-1",
      status: "pending",
      error: "Quota temporarily exhausted",
      schedule: {
        scheduled_at: "2026-04-26T09:00:00.000Z",
        queue_position: null,
      },
    });
  });

  it("normalizes upload queue status, schedule, and error fields", async () => {
    mockCmd.mockResolvedValueOnce([
      {
        id: "queued-1",
        video_path: "C:/clips/one.mp4",
        title: "Queued clip",
        description: "",
        tags: [],
        privacy_status: "unlisted",
        thumbnail_path: null,
        schedule: null,
        created_at: 1777190000,
        status: null,
        error_message: null,
      },
      {
        id: "queued-2",
        video_path: "C:/clips/two.mp4",
        title: "Failed clip",
        description: "",
        tags: [],
        privacy_status: "private",
        thumbnail_path: null,
        schedule: { queue_position: 2 },
        created_at: 1777190100,
        status: "failed",
        error: "OAuth token expired",
      },
    ]);

    await expect(youtubeApi.getUploadQueue()).resolves.toEqual([
      expect.objectContaining({
        id: "queued-1",
        status: "pending",
        error: null,
        schedule: { scheduled_at: null, queue_position: null },
      }),
      expect.objectContaining({
        id: "queued-2",
        status: "failed",
        error: "OAuth token expired",
        schedule: { scheduled_at: null, queue_position: 2 },
      }),
    ]);
    expect(mockCmd).toHaveBeenCalledWith("youtube_get_upload_queue");
  });
});
