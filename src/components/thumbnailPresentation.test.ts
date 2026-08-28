import { readFileSync } from "node:fs";
import { resolve } from "node:path";

function componentSource(path: string) {
  return readFileSync(resolve(process.cwd(), path), "utf8");
}

describe("16:9 thumbnail presentation", () => {
  it.each([
    [
      "home clip card",
      "src/components/clips/ClipCard.tsx",
      "relative block aspect-video w-full shrink-0 bg-black",
    ],
    [
      "editor clip library card",
      "src/components/editor/ClipCard.tsx",
      "relative aspect-video bg-black",
    ],
    [
      "result card",
      "src/components/results/ResultsViewer.tsx",
      "aspect-video bg-black relative",
    ],
  ])(
    "%s letterboxes source imagery in its 16:9 card",
    (_name, path, containerClass) => {
      const source = componentSource(path);

      expect(source).toContain(containerClass);
      expect(source).toContain("object-contain");
    },
  );

  it("keeps timeline strips cropped for compact navigation", () => {
    expect(componentSource("src/components/editor/TimelineClip.tsx")).toContain(
      "object-cover",
    );
  });
});
