import { render, screen } from "@testing-library/react";
import { VideoPlayer } from "./VideoPlayer";

describe("VideoPlayer", () => {
  it("contains arbitrary source aspect ratios without cropping", () => {
    render(
      <VideoPlayer
        src="asset://localhost/ultrawide.mp4"
        title="Ultrawide clip"
      />,
    );

    const video = screen.getByLabelText("Ultrawide clip");

    expect(video).toHaveClass("h-full", "w-full", "object-contain");
    expect(video).not.toHaveClass("aspect-video", "object-cover");
  });
});
