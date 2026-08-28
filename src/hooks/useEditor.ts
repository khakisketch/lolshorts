import { listen } from "@tauri-apps/api/event";
import { useState, useCallback, useEffect } from "react";
import { ClipMetadata } from "@/types/storage";
import {
  useEditorStore,
  CompositionSettings,
  TimelineClip,
} from "@/stores/editorStore";
import { videoApi } from "@/api/video";
import { AppError } from "@/api/client";
import { getErrorMessage } from "@/lib/utils";

// Matches the payloads actually emitted by the backend (emit_export_progress
// / emit_export_complete in video/commands.rs) for compose_shorts,
// compose_shorts_v2, create_longform_video, and export_video.
export interface ExportProgressEvent {
  progress: number;
}

export interface ExportCompleteEvent {
  output_path: string;
}

export function useEditor() {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const {
    setAvailableClips,
    setExportProgress,
    setExportStatus,
    setExportError,
    setExportOutputPath,
  } = useEditorStore();

  // Listen for export progress events
  useEffect(() => {
    const unlistenProgress = listen<ExportProgressEvent>(
      "export-progress",
      (event) => {
        setExportProgress(event.payload.progress);
      },
    );

    const unlistenComplete = listen<ExportCompleteEvent>(
      "export-complete",
      (event) => {
        setExportStatus("complete");
        setExportOutputPath(event.payload.output_path);
        setExportProgress(100);
      },
    );

    const unlistenError = listen<string>("export-error", (event) => {
      setExportStatus("error");
      setExportError(event.payload);
      setExportProgress(0);
    });

    return () => {
      unlistenProgress.then((fn) => fn()).catch(() => {});
      unlistenComplete.then((fn) => fn()).catch(() => {});
      unlistenError.then((fn) => fn()).catch(() => {});
    };
  }, [setExportProgress, setExportStatus, setExportError, setExportOutputPath]);

  /**
   * Load all clips for a specific game
   */
  const loadGameClips = useCallback(
    async (gameId: string): Promise<ClipMetadata[]> => {
      setLoading(true);
      setError(null);

      try {
        const clips = await videoApi.getClips(gameId);
        setAvailableClips(clips);
        return clips;
      } catch (err) {
        const errorMsg =
          err instanceof AppError ? err.message : "Failed to load clips";
        setError(errorMsg);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [setAvailableClips],
  );

  /**
   * Generate thumbnail for a video at specific timestamp
   */
  const generateThumbnail = useCallback(
    async (
      videoPath: string,
      timestamp: number,
      outputPath?: string, // Optional, needed for new API
    ): Promise<string> => {
      if (outputPath) {
        return await videoApi.generateThumbnail(
          videoPath,
          outputPath,
          timestamp,
        );
      } else {
        // Fallback to auto-generation if no output path provided (using preview logic)
        return await videoApi.generateClipThumbnail(videoPath);
      }
    },
    [],
  );

  /**
   * Compose multiple clips into a single Short video.
   *
   * Uses compose_shorts_v2, which honors per-clip trim (trimStart/trimEnd)
   * plus the composition's aspect ratio and transition settings. The
   * editor's trimEnd is an absolute end-position (defaults to clip
   * duration), while the backend's trim_end is "seconds to cut from the
   * clip's end" - convert between the two conventions here.
   */
  const composeShorts = useCallback(
    async (
      timelineClips: TimelineClip[],
      settings: CompositionSettings,
      outputPath: string,
    ): Promise<string> => {
      setLoading(true);
      setError(null);
      setExportStatus("exporting");
      setExportProgress(0);
      setExportError(null);

      try {
        const clips = timelineClips
          .filter((clip) => clip.file_path && clip.file_path.length > 0)
          .map((clip) => {
            const trimStart =
              typeof clip.trimStart === "number" && clip.trimStart > 0
                ? clip.trimStart
                : undefined;
            const trimEnd =
              typeof clip.trimEnd === "number" && clip.trimEnd < clip.duration
                ? clip.duration - clip.trimEnd
                : undefined;
            return {
              path: clip.file_path,
              trim_start: trimStart,
              trim_end: trimEnd,
            };
          });

        if (clips.length === 0) {
          throw new Error("No valid clips to export");
        }

        const result = await videoApi.composeShortsV2(
          clips,
          settings.aspectRatio,
          settings.transitionType,
          settings.transitionDuration,
          outputPath,
        );

        // Status/progress are driven by the export-progress / export-complete
        // Tauri events emitted by the backend (see the listener above) - the
        // backend always emits export-complete before this promise resolves,
        // so a redundant manual 'complete' transition here would race the
        // event-driven progress bar (0->100 jump). Just record the output
        // path as a fallback in case the event was somehow missed.
        setExportOutputPath(result);
        return result;
      } catch (err) {
        const errorMsg = getErrorMessage(err);
        setError(errorMsg);
        setExportStatus("error");
        setExportError(errorMsg);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [setExportStatus, setExportProgress, setExportError, setExportOutputPath],
  );

  /**
   * Extract a single clip from game footage
   * Updated to match Backend Signature: inputPath, outputPath, startTime, duration
   */
  const extractClip = useCallback(
    async (
      inputPath: string,
      outputPath: string,
      startTime: number,
      duration: number,
    ): Promise<string> => {
      setLoading(true);
      setError(null);

      try {
        return await videoApi.extractClip(
          inputPath,
          outputPath,
          startTime,
          duration,
        );
      } catch (err) {
        const errorMsg =
          err instanceof AppError ? err.message : "Extraction failed";
        setError(errorMsg);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  /**
   * Create a long-form montage from clips
   */
  const createMontage = useCallback(
    async (
      timelineClips: TimelineClip[],
      outputPath: string,
    ): Promise<string> => {
      setLoading(true);
      setError(null);
      setExportStatus("exporting");
      setExportProgress(0);
      setExportError(null);

      try {
        const clipPaths = timelineClips.map((c) => c.file_path || "");
        // Filter out empty paths
        const validPaths = clipPaths.filter((p) => p.length > 0);

        if (validPaths.length === 0) {
          throw new Error("No valid clips to export");
        }

        const result = await videoApi.createLongformVideo(
          validPaths,
          outputPath,
        );

        // Same reasoning as composeShorts: create_longform_video also emits
        // export-progress/export-complete, so avoid a redundant manual
        // 'complete' transition here.
        setExportOutputPath(result);
        return result;
      } catch (err) {
        const errorMsg =
          err instanceof AppError ? err.message : "Montage export failed";
        setError(errorMsg);
        setExportStatus("error");
        setExportError(errorMsg);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [setExportStatus, setExportProgress, setExportError, setExportOutputPath],
  );

  return {
    isLoading: loading,
    error,
    loadGameClips,
    generateThumbnail,
    composeShorts,
    createMontage,
    extractClip,
  };
}
