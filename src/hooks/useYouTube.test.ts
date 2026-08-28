import { renderHook, act, waitFor } from "@testing-library/react";
import { useYouTube } from "./useYouTube";

// Mock the youtube API
jest.mock("@/api/youtube", () => ({
  youtubeApi: {
    getAuthStatus: jest.fn(),
    getUploadHistory: jest.fn(),
    getQuotaInfo: jest.fn(),
    startAuthWithServer: jest.fn(),
    completeAuth: jest.fn(),
    logout: jest.fn(),
    uploadVideo: jest.fn(),
    getUploadProgress: jest.fn(),
    addToHistory: jest.fn(),
    getUploadQueue: jest.fn(),
    scheduleUpload: jest.fn(),
    cancelScheduledUpload: jest.fn(),
  },
}));

// Mock client API
jest.mock("@/api/client", () => ({
  cmd: jest.fn(),
  listenToEvent: jest.fn().mockResolvedValue(jest.fn()),
}));

// Mock utils
jest.mock("@/lib/utils", () => ({
  getErrorMessage: jest.fn((err) => err?.message || "Unknown error"),
}));

import { youtubeApi } from "../api/youtube";

const mockYoutubeApi = youtubeApi as jest.Mocked<typeof youtubeApi>;

describe("useYouTube", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockYoutubeApi.getAuthStatus.mockResolvedValue({
      authenticated: false,
      expires_at: null,
      has_refresh_token: false,
    });
    mockYoutubeApi.getUploadHistory.mockResolvedValue([]);
    mockYoutubeApi.getUploadQueue.mockResolvedValue([]);
  });

  afterEach(async () => {
    await act(async () => {
      await Promise.resolve();
    });
  });

  describe("initialization", () => {
    it("should check auth status on mount", async () => {
      renderHook(() => useYouTube());
      await act(async () => {
        await Promise.resolve();
      });
      await waitFor(() => {
        expect(mockYoutubeApi.getAuthStatus).toHaveBeenCalled();
      });
    });

    it("should load history on mount", async () => {
      renderHook(() => useYouTube());
      await act(async () => {
        await Promise.resolve();
      });
      await waitFor(() => {
        expect(mockYoutubeApi.getUploadHistory).toHaveBeenCalled();
      });
    });

    it("should set isAuthenticated to false initially", async () => {
      const { result } = renderHook(() => useYouTube());
      await act(async () => {
        await Promise.resolve();
      });
      expect(result.current.isAuthenticated).toBe(false);
    });
  });

  describe("authentication", () => {
    it("should update auth status after checkAuthStatus", async () => {
      mockYoutubeApi.getAuthStatus.mockResolvedValue({
        authenticated: true,
        expires_at: Date.now() + 3600000,
        has_refresh_token: true,
      });

      const { result } = renderHook(() => useYouTube());

      await waitFor(() => {
        expect(result.current.isAuthenticated).toBe(true);
      });
    });

    it("should call startAuthWithServer when startAuth is called", async () => {
      mockYoutubeApi.startAuthWithServer.mockResolvedValue("https://auth.url");

      const { result } = renderHook(() => useYouTube());

      let authUrl: string | undefined;
      await act(async () => {
        authUrl = await result.current.startAuth();
      });

      expect(mockYoutubeApi.startAuthWithServer).toHaveBeenCalled();
      expect(authUrl).toBe("https://auth.url");
    });

    it("should handle logout correctly", async () => {
      mockYoutubeApi.getAuthStatus.mockResolvedValue({
        authenticated: true,
        expires_at: Date.now() + 3600000,
        has_refresh_token: true,
      });
      mockYoutubeApi.logout.mockResolvedValue(undefined);

      const { result } = renderHook(() => useYouTube());

      await waitFor(() => {
        expect(result.current.isAuthenticated).toBe(true);
      });

      await act(async () => {
        await result.current.logout();
      });

      expect(result.current.isAuthenticated).toBe(false);
      expect(result.current.uploadHistory).toEqual([]);
    });
  });

  describe("upload", () => {
    it("should upload video with correct parameters", async () => {
      const mockVideo = {
        id: "video123",
        title: "Test Video",
        description: "Test Description",
        url: "https://youtube.com/watch?v=video123",
        thumbnail_url: "https://img.youtube.com/vi/video123/default.jpg",
      };
      mockYoutubeApi.uploadVideo.mockResolvedValue(mockVideo);

      const { result } = renderHook(() => useYouTube());

      let uploadResult;
      await act(async () => {
        uploadResult = await result.current.uploadVideo(
          "/path/to/video.mp4",
          {
            title: "Test Video",
            description: "Test Description",
            tags: ["test", "video"],
            privacy_status: "private",
          },
          "/path/to/thumbnail.jpg",
        );
      });

      expect(mockYoutubeApi.uploadVideo).toHaveBeenCalledWith(
        "/path/to/video.mp4",
        "Test Video",
        "Test Description",
        ["test", "video"],
        "private",
        "/path/to/thumbnail.jpg",
      );
      expect(uploadResult).toEqual(mockVideo);
    });

    it("should reload history after successful upload", async () => {
      mockYoutubeApi.uploadVideo.mockResolvedValue({
        id: "video123",
        title: "Test",
        description: "",
        url: "",
        thumbnail_url: "",
      });

      const { result } = renderHook(() => useYouTube());

      // Clear initial calls
      mockYoutubeApi.getUploadHistory.mockClear();

      await act(async () => {
        await result.current.uploadVideo("/path.mp4", {
          title: "Test",
          description: "",
          privacy_status: "private",
        });
      });

      expect(mockYoutubeApi.getUploadHistory).toHaveBeenCalled();
    });
  });

  describe("quota", () => {
    it("should fetch quota info", async () => {
      const mockQuota = {
        daily_limit: 10000,
        used: 1600,
        remaining: 8400,
        reset_at: Date.now() + 86400000,
      };
      mockYoutubeApi.getQuotaInfo.mockResolvedValue(mockQuota);

      const { result } = renderHook(() => useYouTube());

      let quotaInfo;
      await act(async () => {
        quotaInfo = await result.current.getQuotaInfo();
      });

      expect(quotaInfo).toEqual(mockQuota);
    });

    it("should expose and rethrow quota fetch errors", async () => {
      mockYoutubeApi.getQuotaInfo.mockRejectedValue(new Error("Network error"));

      const { result } = renderHook(() => useYouTube());

      await act(async () => {
        await expect(result.current.getQuotaInfo()).rejects.toThrow(
          "Network error",
        );
      });

      await waitFor(() => {
        expect(result.current.error).toBe("Network error");
      });
    });
  });

  describe("progress polling", () => {
    beforeEach(() => {
      jest.useFakeTimers();
    });

    afterEach(() => {
      jest.useRealTimers();
    });

    it("should start polling when startProgressPolling is called", async () => {
      mockYoutubeApi.getUploadProgress.mockResolvedValue({
        progress: 50,
        status: "uploading",
      });

      const { result } = renderHook(() => useYouTube());

      act(() => {
        result.current.startProgressPolling();
      });

      // Advance timers
      await act(async () => {
        jest.advanceTimersByTime(1000);
      });

      expect(mockYoutubeApi.getUploadProgress).toHaveBeenCalled();
    });

    it("should stop polling when stopProgressPolling is called", async () => {
      const { result } = renderHook(() => useYouTube());
      await act(async () => {
        await Promise.resolve();
      });

      act(() => {
        result.current.startProgressPolling();
      });

      act(() => {
        result.current.stopProgressPolling();
      });

      // Clear previous calls
      mockYoutubeApi.getUploadProgress.mockClear();

      act(() => {
        jest.advanceTimersByTime(2000);
      });

      expect(mockYoutubeApi.getUploadProgress).not.toHaveBeenCalled();
    });
  });
});
