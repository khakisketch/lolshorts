import { fireEvent, render, screen } from "@testing-library/react";
import { QuotaDisplay } from "./QuotaDisplay";
import { YouTubeAuth } from "./YouTubeAuth";
import { YouTubeUpload } from "./YouTubeUpload";

jest.mock("@/hooks/useYouTube");

jest.mock("@/api/youtube", () => ({
  youtubeApi: {
    getUploadQueue: jest.fn(),
    cancelScheduledUpload: jest.fn(),
    scheduleUpload: jest.fn(),
  },
}));

jest.mock("@/lib/logger", () => ({
  logger: {
    error: jest.fn(),
  },
}));

jest.mock("@tauri-apps/plugin-shell", () => ({
  open: jest.fn(),
}));

jest.mock("@tauri-apps/plugin-dialog", () => ({
  open: jest.fn(),
}));

type UseYouTubeReturn = ReturnType<
  typeof import("../../hooks/useYouTube").useYouTube
>;
type UseYouTubeMock = jest.MockedFunction<() => UseYouTubeReturn>;
type YouTubeApi = typeof import("../../api/youtube").youtubeApi;

const mockedUseYouTube = (
  jest.requireMock("@/hooks/useYouTube") as { useYouTube: UseYouTubeMock }
).useYouTube;
const mockedYoutubeApi = (
  jest.requireMock("@/api/youtube") as { youtubeApi: jest.Mocked<YouTubeApi> }
).youtubeApi;

function makeUseYouTube(
  overrides: Partial<UseYouTubeReturn> = {},
): UseYouTubeReturn {
  return {
    authStatus: {
      authenticated: true,
      expires_at: null,
      has_refresh_token: true,
    },
    isAuthenticated: true,
    isLoading: false,
    error: null,
    queueError: null,
    uploadHistory: [],
    uploadQueue: [],
    uploadProgress: null,
    startAuth: jest.fn(),
    startAuthWithServer: jest.fn(),
    completeAuth: jest.fn(),
    logout: jest.fn(),
    uploadVideo: jest.fn(),
    checkAuthStatus: jest.fn(),
    addToHistory: jest.fn(),
    getQuotaInfo: jest.fn(),
    getUploadHistory: jest.fn(),
    loadQueue: jest.fn(),
    startProgressPolling: jest.fn(),
    stopProgressPolling: jest.fn(),
    authEventError: null,
    clearAuthEventError: jest.fn(),
    ...overrides,
  };
}

describe("YouTube visible failure states", () => {
  beforeEach(() => {
    jest.resetAllMocks();
    mockedYoutubeApi.getUploadQueue.mockResolvedValue([]);
  });

  it("shows a destructive quota alert when quota loading fails", async () => {
    mockedUseYouTube.mockReturnValue(
      makeUseYouTube({
        getQuotaInfo: jest.fn().mockRejectedValue(new Error("Quota API down")),
      }),
    );

    render(<QuotaDisplay />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Quota API down",
    );
  });

  it("shows an auth action alert when starting YouTube auth fails", async () => {
    const startAuthWithServer = jest
      .fn()
      .mockRejectedValue(new Error("OAuth server unavailable"));
    mockedUseYouTube.mockReturnValue(
      makeUseYouTube({
        authStatus: {
          authenticated: false,
          expires_at: null,
          has_refresh_token: false,
        },
        isAuthenticated: false,
        startAuthWithServer,
      }),
    );

    render(<YouTubeAuth />);
    fireEvent.click(
      screen.getByRole("button", {
        name: "youtube.auth.connectYouTubeAccount",
      }),
    );

    expect(
      await screen.findByText("OAuth server unavailable"),
    ).toBeInTheDocument();
  });

  it("does not expose legacy scheduled uploads in the free public edition", () => {
    mockedUseYouTube.mockReturnValue(
      makeUseYouTube({
        queueError: "Queue backend down",
      }),
    );

    render(<YouTubeUpload />);

    expect(screen.queryByText("Queue backend down")).not.toBeInTheDocument();
    expect(
      screen.queryByText("youtube.schedule.scheduleButton"),
    ).not.toBeInTheDocument();
    expect(screen.getByText("youtube.upload.uploadButton")).toBeInTheDocument();
  });

  it("does not load scheduled uploads before YouTube authentication", () => {
    mockedUseYouTube.mockReturnValue(
      makeUseYouTube({
        authStatus: {
          authenticated: false,
          expires_at: null,
          has_refresh_token: false,
        },
        isAuthenticated: false,
      }),
    );

    render(<YouTubeUpload />);

    expect(mockedYoutubeApi.getUploadQueue).not.toHaveBeenCalled();
    expect(
      screen.getByText("youtube.upload.connectRequired"),
    ).toBeInTheDocument();
  });
});
