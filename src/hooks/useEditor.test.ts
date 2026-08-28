import { renderHook, act, waitFor } from "@testing-library/react";
import { useEditor } from "./useEditor";

// Mock Tauri APIs
const mockListen = jest.fn();
const mockInvoke = jest.fn();

jest.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => mockListen(...args),
}));

jest.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Mock video API. composeShortsV2 uses the *real* implementation (a thin
// pass-through to cmd()) so the "compose_shorts_v2 payload" test below can
// verify the trim/aspect/transition fields actually reach the invoke layer,
// not just useEditor's call into videoApi.
jest.mock("@/api/video", () => {
  const actual = jest.requireActual("@/api/video");
  return {
    videoApi: {
      getClips: jest.fn(),
      generateThumbnail: jest.fn(),
      generateClipThumbnail: jest.fn(),
      composeShorts: jest.fn(),
      composeShortsV2: actual.videoApi.composeShortsV2,
      extractClip: jest.fn(),
      createLongformVideo: jest.fn(),
    },
  };
});

// Mock editor store
const mockSetAvailableClips = jest.fn();
const mockSetExportProgress = jest.fn();
const mockSetExportStatus = jest.fn();
const mockSetExportError = jest.fn();
const mockSetExportOutputPath = jest.fn();

jest.mock("@/stores/editorStore", () => ({
  useEditorStore: () => ({
    setAvailableClips: mockSetAvailableClips,
    setExportProgress: mockSetExportProgress,
    setExportStatus: mockSetExportStatus,
    setExportError: mockSetExportError,
    setExportOutputPath: mockSetExportOutputPath,
  }),
}));

// Mock utils
jest.mock("@/lib/utils", () => ({
  getErrorMessage: jest.fn((err) => err?.message || "Unknown error"),
}));

// Mock the Tauri invoke wrapper. composeShortsV2 (real implementation, see
// above) calls through this `cmd`, so mockCmd lets us capture the exact
// command name + payload sent to compose_shorts_v2.
const mockCmd = jest.fn();

jest.mock("@/api/client", () => ({
  cmd: (...args: unknown[]) => mockCmd(...args),
  AppError: class AppError extends Error {
    code: string;
    constructor(response: { code: string; message: string }) {
      super(response.message);
      this.code = response.code;
    }
  },
}));

import { videoApi } from "@/api/video";

const mockVideoApi = videoApi as jest.Mocked<typeof videoApi>;

describe("useEditor", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    // Setup default mock for listen - returns cleanup function
    mockListen.mockImplementation(() => Promise.resolve(() => {}));
  });

  describe("initialization", () => {
    it("should setup event listeners on mount", () => {
      renderHook(() => useEditor());

      expect(mockListen).toHaveBeenCalledWith(
        "export-progress",
        expect.any(Function),
      );
      expect(mockListen).toHaveBeenCalledWith(
        "export-complete",
        expect.any(Function),
      );
      expect(mockListen).toHaveBeenCalledWith(
        "export-error",
        expect.any(Function),
      );
    });

    it("should cleanup listeners on unmount", async () => {
      const cleanupFn = jest.fn();
      mockListen.mockImplementation(() => Promise.resolve(cleanupFn));

      const { unmount } = renderHook(() => useEditor());

      unmount();

      // Wait for cleanup promises to resolve
      await waitFor(() => {
        expect(cleanupFn).toHaveBeenCalled();
      });
    });

    it("should start with loading false and no error", () => {
      const { result } = renderHook(() => useEditor());

      expect(result.current.isLoading).toBe(false);
      expect(result.current.error).toBeNull();
    });
  });

  describe("loadGameClips", () => {
    it("should load clips for a game", async () => {
      const mockClips = [
        { id: "clip1", file_path: "/path/to/clip1.mp4", duration: 10 },
        { id: "clip2", file_path: "/path/to/clip2.mp4", duration: 15 },
      ];
      mockVideoApi.getClips.mockResolvedValue(mockClips);

      const { result } = renderHook(() => useEditor());

      let clips;
      await act(async () => {
        clips = await result.current.loadGameClips("game123");
      });

      expect(mockVideoApi.getClips).toHaveBeenCalledWith("game123");
      expect(mockSetAvailableClips).toHaveBeenCalledWith(mockClips);
      expect(clips).toEqual(mockClips);
    });

    it("should handle errors when loading clips", async () => {
      mockVideoApi.getClips.mockRejectedValue(new Error("Failed to load"));

      const { result } = renderHook(() => useEditor());

      await act(async () => {
        try {
          await result.current.loadGameClips("game123");
        } catch {
          // Expected to throw
        }
      });

      // Error message comes from useEditor implementation
      expect(result.current.error).toBe("Failed to load clips");
    });
  });

  describe("generateThumbnail", () => {
    it("should generate thumbnail with output path", async () => {
      mockVideoApi.generateThumbnail.mockResolvedValue(
        "/path/to/thumbnail.jpg",
      );

      const { result } = renderHook(() => useEditor());

      let thumbnailPath;
      await act(async () => {
        thumbnailPath = await result.current.generateThumbnail(
          "/video.mp4",
          5.0,
          "/output/thumb.jpg",
        );
      });

      expect(mockVideoApi.generateThumbnail).toHaveBeenCalledWith(
        "/video.mp4",
        "/output/thumb.jpg",
        5.0,
      );
      expect(thumbnailPath).toBe("/path/to/thumbnail.jpg");
    });

    it("should use auto-generation without output path", async () => {
      mockVideoApi.generateClipThumbnail.mockResolvedValue(
        "/auto/thumbnail.jpg",
      );

      const { result } = renderHook(() => useEditor());

      let thumbnailPath;
      await act(async () => {
        thumbnailPath = await result.current.generateThumbnail(
          "/video.mp4",
          5.0,
        );
      });

      expect(mockVideoApi.generateClipThumbnail).toHaveBeenCalledWith(
        "/video.mp4",
      );
      expect(thumbnailPath).toBe("/auto/thumbnail.jpg");
    });
  });

  describe("composeShorts", () => {
    it("sends a compose_shorts_v2 payload with trim, aspect ratio, and transition", async () => {
      mockCmd.mockResolvedValue("/output/composed.mp4");

      const { result } = renderHook(() => useEditor());

      const timelineClips = [
        {
          file_path: "/clip1.mp4",
          duration: 20,
          order: 0,
          trimStart: 2,
          trimEnd: 18,
        },
        {
          file_path: "/clip2.mp4",
          duration: 10,
          order: 1,
        },
      ];

      const settings = {
        aspectRatio: "9:16",
        transitionType: "fade",
        transitionDuration: 0.5,
      };

      let outputPath;
      await act(async () => {
        outputPath = await result.current.composeShorts(
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          timelineClips as any,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          settings as any,
          "/output/composed.mp4",
        );
      });

      // trimEnd=18 on a 20s clip means "cut the last 2s" - the backend's
      // trim_end convention (seconds cut from the end), not the store's
      // absolute end-position convention.
      expect(mockCmd).toHaveBeenCalledWith("compose_shorts_v2", {
        clips: [
          { path: "/clip1.mp4", trim_start: 2, trim_end: 2 },
          { path: "/clip2.mp4", trim_start: undefined, trim_end: undefined },
        ],
        aspect_ratio: "9:16",
        transition_type: "fade",
        transition_duration: 0.5,
        output_path: "/output/composed.mp4",
      });
      expect(mockVideoApi.composeShorts).not.toHaveBeenCalled();
      expect(mockSetExportStatus).toHaveBeenCalledWith("exporting");
      // Status/progress are event-driven (export-complete listener) now that
      // the backend actually emits them - a manual 'complete' here would
      // race the event stream and cause a 0->100 progress jump.
      expect(mockSetExportStatus).not.toHaveBeenCalledWith("complete");
      expect(outputPath).toBe("/output/composed.mp4");
    });

    it("should throw error when no valid clips", async () => {
      const { result } = renderHook(() => useEditor());

      await expect(
        act(async () => {
          await result.current.composeShorts(
            [],
            {
              aspectRatio: "9:16",
              transitionType: "none",
              transitionDuration: 0,
            },
            "/output.mp4",
          );
        }),
      ).rejects.toThrow("No valid clips to export");

      expect(mockSetExportStatus).toHaveBeenCalledWith("error");
      expect(mockCmd).not.toHaveBeenCalled();
    });
  });

  describe("extractClip", () => {
    it("should extract clip from video", async () => {
      mockVideoApi.extractClip.mockResolvedValue("/extracted/clip.mp4");

      const { result } = renderHook(() => useEditor());

      let outputPath;
      await act(async () => {
        outputPath = await result.current.extractClip(
          "/input.mp4",
          "/output/clip.mp4",
          10.0,
          5.0,
        );
      });

      expect(mockVideoApi.extractClip).toHaveBeenCalledWith(
        "/input.mp4",
        "/output/clip.mp4",
        10.0,
        5.0,
      );
      expect(outputPath).toBe("/extracted/clip.mp4");
    });
  });

  describe("createMontage", () => {
    it("should create montage from clips", async () => {
      mockVideoApi.createLongformVideo.mockResolvedValue("/montage.mp4");

      const { result } = renderHook(() => useEditor());

      const timelineClips = [
        { clip_id: "1", file_path: "/clip1.mp4", duration: 10, order: 0 },
        { clip_id: "2", file_path: "/clip2.mp4", duration: 15, order: 1 },
      ];

      let outputPath;
      await act(async () => {
        outputPath = await result.current.createMontage(
          timelineClips,
          "/montage.mp4",
        );
      });

      expect(mockVideoApi.createLongformVideo).toHaveBeenCalledWith(
        ["/clip1.mp4", "/clip2.mp4"],
        "/montage.mp4",
      );
      expect(outputPath).toBe("/montage.mp4");
    });
  });

  describe("event handling", () => {
    it("should handle export-progress event", async () => {
      let progressCallback:
        | ((event: { payload: { progress: number } }) => void)
        | undefined;
      mockListen.mockImplementation(
        (
          event: string,
          callback: (event: { payload: { progress: number } }) => void,
        ) => {
          if (event === "export-progress") {
            progressCallback = callback;
          }
          return Promise.resolve(() => {});
        },
      );

      renderHook(() => useEditor());

      // Simulate progress event
      await act(async () => {
        progressCallback?.({
          payload: { progress: 50, current_clip: 1, total_clips: 2 },
        });
      });

      expect(mockSetExportProgress).toHaveBeenCalledWith(50);
    });

    it("should handle export-complete event", async () => {
      let completeCallback:
        | ((event: { payload: { output_path: string } }) => void)
        | undefined;
      mockListen.mockImplementation(
        (
          event: string,
          callback: (event: { payload: { output_path: string } }) => void,
        ) => {
          if (event === "export-complete") {
            completeCallback = callback;
          }
          return Promise.resolve(() => {});
        },
      );

      renderHook(() => useEditor());

      await act(async () => {
        completeCallback?.({
          payload: {
            output_path: "/output.mp4",
            duration: 30,
            file_size: 1000000,
          },
        });
      });

      expect(mockSetExportStatus).toHaveBeenCalledWith("complete");
      expect(mockSetExportOutputPath).toHaveBeenCalledWith("/output.mp4");
      expect(mockSetExportProgress).toHaveBeenCalledWith(100);
    });

    it("should handle export-error event", async () => {
      let errorCallback: ((event: { payload: string }) => void) | undefined;
      mockListen.mockImplementation(
        (event: string, callback: (event: { payload: string }) => void) => {
          if (event === "export-error") {
            errorCallback = callback;
          }
          return Promise.resolve(() => {});
        },
      );

      renderHook(() => useEditor());

      await act(async () => {
        errorCallback?.({ payload: "Export failed" });
      });

      expect(mockSetExportStatus).toHaveBeenCalledWith("error");
      expect(mockSetExportError).toHaveBeenCalledWith("Export failed");
      expect(mockSetExportProgress).toHaveBeenCalledWith(0);
    });
  });
});
