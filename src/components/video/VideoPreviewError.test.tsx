import { fireEvent, render, screen } from "@testing-library/react";
import { VideoPreviewError } from "./VideoPreviewError";

const mockOpenFile = jest.fn();

jest.mock("@/api/utils", () => ({
  utilsApi: {
    openFileWithDefaultApp: (...args: unknown[]) => mockOpenFile(...args),
  },
}));

describe("VideoPreviewError", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("names the failure and offers a retry action", () => {
    const onRetry = jest.fn();
    render(<VideoPreviewError onRetry={onRetry} />);

    expect(screen.getByRole("alert")).toHaveTextContent(
      "video.errors.previewUnavailable",
    );

    fireEvent.click(screen.getByRole("button", { name: "common.retry" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("opens the underlying file when a path is provided", () => {
    render(
      <VideoPreviewError
        filePath="C:/clips/penta.mp4"
        onRetry={jest.fn()}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", { name: "video.errors.openInSystemPlayer" }),
    );
    expect(mockOpenFile).toHaveBeenCalledWith("C:/clips/penta.mp4");
  });

  it("hides the open-file action when there is no path", () => {
    render(<VideoPreviewError onRetry={jest.fn()} />);

    expect(
      screen.queryByRole("button", {
        name: "video.errors.openInSystemPlayer",
      }),
    ).not.toBeInTheDocument();
  });
});
