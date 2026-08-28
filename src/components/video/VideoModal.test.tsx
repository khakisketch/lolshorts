import { fireEvent, render, screen } from "@testing-library/react";
import { VideoModal } from "./VideoModal";

jest.mock("./VideoPlayer", () => ({
  VideoPlayer: ({ className }: { className?: string }) => (
    <div data-testid="video-player" className={className} />
  ),
}));

describe("VideoModal", () => {
  it("uses one centered positioning model and stays inside the viewport", () => {
    render(
      <VideoModal
        isOpen
        onClose={jest.fn()}
        src="asset://localhost/ultrawide.mp4"
        title="Ultrawide clip"
      />,
    );

    const dialog = screen.getByRole("dialog");

    expect(dialog).toHaveClass("left-[50%]", "top-[50%]");
    expect(dialog).toHaveClass("translate-x-[-50%]", "translate-y-[-50%]");
    expect(dialog).toHaveClass("w-[calc(100vw-2rem)]", "h-[calc(100vh-2rem)]");
    expect(dialog).not.toHaveClass("inset-4");
    expect(screen.getByTestId("video-player")).toHaveClass("min-h-0", "flex-1");
  });

  it("closes on Escape and only when the backdrop itself is clicked", () => {
    const onClose = jest.fn();
    render(
      <VideoModal
        isOpen
        onClose={onClose}
        src="asset://localhost/ultrawide.mp4"
        title="Ultrawide clip"
      />,
    );

    fireEvent.click(screen.getByTestId("video-player"));
    expect(onClose).not.toHaveBeenCalled();

    const backdrop = document.querySelector(".backdrop-blur-sm");
    expect(backdrop).not.toBeNull();
    fireEvent.click(backdrop!);
    expect(onClose).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
