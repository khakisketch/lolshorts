export interface ClipThumbnailRequest {
  gameId: string;
  clipFilePath: string;
}

type ThumbnailWorker = (request: ClipThumbnailRequest) => Promise<string>;

/** Small FIFO used by the vault so scrolling cannot start an FFmpeg process per card. */
export function createClipThumbnailQueue(worker: ThumbnailWorker, limit = 2) {
  const concurrency = Math.max(1, limit);
  let active = 0;
  const pending: Array<{
    request: ClipThumbnailRequest;
    resolve: (path: string) => void;
    reject: (error: unknown) => void;
  }> = [];
  const inFlight = new Map<string, Promise<string>>();
  const attempted = new Set<string>();

  const keyFor = ({ gameId, clipFilePath }: ClipThumbnailRequest) =>
    `${gameId}\u0000${clipFilePath}`;

  const drain = () => {
    while (active < concurrency && pending.length > 0) {
      const item = pending.shift();
      if (!item) break;
      active += 1;
      worker(item.request)
        .then(item.resolve, item.reject)
        .finally(() => {
          active -= 1;
          inFlight.delete(keyFor(item.request));
          drain();
        });
    }
  };

  return {
    request(request: ClipThumbnailRequest): Promise<string> | null {
      const key = keyFor(request);
      const existing = inFlight.get(key);
      if (existing) return existing;
      if (attempted.has(key)) return null;

      attempted.add(key);
      const promise = new Promise<string>((resolve, reject) => {
        pending.push({ request, resolve, reject });
        drain();
      });
      inFlight.set(key, promise);
      return promise;
    },
    hasAttempted(request: ClipThumbnailRequest) {
      return attempted.has(keyFor(request));
    },
  };
}
