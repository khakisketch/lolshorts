import { fireEvent, render, screen } from "@testing-library/react";
import { VideoPreview } from "./VideoPreview";

jest.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://${path}`,
}));

jest.mock("@/api/utils", () => ({
  utilsApi: {
    openFileWithDefaultApp: jest.fn(),
  },
}));

// A single stable snapshot: fresh object/array literals per call would make
// `selectedClip` a new reference every render, retriggering the clip-changed
// effect (and its clearError) and masking the error state under test.
jest.mock("@/stores/editorStore", () => {
  const state = {
    selectedClipId: "C:/clips/missing.mp4",
    availableClips: [
      {
        file_path: "C:/clips/missing.mp4",
        game_id: "game-a",
        event_type: "PentaKill",
        duration: 12,
      },
    ],
    timelineClips: [],
    isPlaying: false,
    play: jest.fn(),
    pause: jest.fn(),
    setCurrentTime: jest.fn(),
  };
  return { useEditorStore: () => state };
});

describe("VideoPreview onError (G007)", () => {
  beforeAll(() => {
    window.HTMLMediaElement.prototype.load = jest.fn();
    window.HTMLMediaElement.prototype.play = jest
      .fn()
      .mockResolvedValue(undefined);
    window.HTMLMediaElement.prototype.pause = jest.fn();
  });

  it("surfaces a message when the preview source fails to load", () => {
    const { container } = render(<VideoPreview />);

    const video = container.querySelector("video") as HTMLVideoElement;
    expect(video).not.toBeNull();
    expect(screen.queryByTestId("video-preview-error")).not.toBeInTheDocument();

    fireEvent.error(video);

    expect(screen.getByTestId("video-preview-error")).toHaveTextContent(
      "video.errors.previewUnavailable",
    );
  });

  it("retries loading and clears the message", () => {
    const { container } = render(<VideoPreview />);
    const video = container.querySelector("video") as HTMLVideoElement;

    fireEvent.error(video);
    expect(screen.getByTestId("video-preview-error")).toBeInTheDocument();

    (window.HTMLMediaElement.prototype.load as jest.Mock).mockClear();
    fireEvent.click(screen.getByRole("button", { name: "common.retry" }));

    expect(window.HTMLMediaElement.prototype.load).toHaveBeenCalled();
    expect(screen.queryByTestId("video-preview-error")).not.toBeInTheDocument();
  });
});
