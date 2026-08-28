import { createClipThumbnailQueue } from "./clipThumbnailQueue";

describe("clip thumbnail queue", () => {
  it("runs at most two jobs and deduplicates or suppresses session retries", async () => {
    let active = 0;
    let maximum = 0;
    const releases: Array<() => void> = [];
    const worker = jest.fn(
      () =>
        new Promise<string>((resolve) => {
          active += 1;
          maximum = Math.max(maximum, active);
          releases.push(() => {
            active -= 1;
            resolve("thumb.jpg");
          });
        }),
    );
    const queue = createClipThumbnailQueue(worker, 2);
    const requests = ["a", "b", "c"].map((clipFilePath) =>
      queue.request({ gameId: "game", clipFilePath }),
    );
    const duplicate = queue.request({ gameId: "game", clipFilePath: "a" });
    expect(duplicate).toBe(requests[0]);
    expect(worker).toHaveBeenCalledTimes(2);

    releases.shift()?.();
    await Promise.resolve();
    await Promise.resolve();
    expect(worker).toHaveBeenCalledTimes(3);
    while (releases.length > 0) releases.shift()?.();
    await Promise.all(requests);

    expect(maximum).toBe(2);
    expect(queue.request({ gameId: "game", clipFilePath: "a" })).toBeNull();
  });
});
