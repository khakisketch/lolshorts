import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useState, useCallback, useEffect } from 'react';
import { ClipMetadata } from '@/types/storage';
import { useEditorStore, CompositionSettings, TimelineClip } from '@/stores/editorStore';
import { videoApi } from '@/api/video';
import { AppError } from '@/api/client';
import { getErrorMessage } from '@/lib/utils';

export interface ExportProgressEvent {
  progress: number;
  current_clip: number;
  total_clips: number;
}

export interface ExportCompleteEvent {
  output_path: string;
  duration: number;
  file_size: number;
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
    const unlistenProgress = listen<ExportProgressEvent>('export-progress', (event) => {
      setExportProgress(event.payload.progress);
    });

    const unlistenComplete = listen<ExportCompleteEvent>('export-complete', (event) => {
      setExportStatus('complete');
      setExportOutputPath(event.payload.output_path);
      setExportProgress(100);
    });

    const unlistenError = listen<string>('export-error', (event) => {
      setExportStatus('error');
      setExportError(event.payload);
      setExportProgress(0);
    });

    return () => {
      unlistenProgress.then(fn => fn()).catch(() => {});
      unlistenComplete.then(fn => fn()).catch(() => {});
      unlistenError.then(fn => fn()).catch(() => {});
    };
  }, [setExportProgress, setExportStatus, setExportError, setExportOutputPath]);

  /**
   * Load all clips for a specific game
   */
  const loadGameClips = useCallback(async (gameId: string): Promise<ClipMetadata[]> => {
    setLoading(true);
    setError(null);

    try {
      const clips = await videoApi.getClips(gameId);
      setAvailableClips(clips);
      return clips;
    } catch (err) {
      const errorMsg = err instanceof AppError ? err.message : 'Failed to load clips';
      setError(errorMsg);
      throw err;
    } finally {
      setLoading(false);
    }
  }, [setAvailableClips]);

  /**
   * Generate thumbnail for a video at specific timestamp
   */
  const generateThumbnail = useCallback(async (
    videoPath: string,
    timestamp: number,
    outputPath?: string // Optional, needed for new API
  ): Promise<string> => {
    if (outputPath) {
        return await videoApi.generateThumbnail(videoPath, outputPath, timestamp);
    } else {
        // Fallback to auto-generation if no output path provided (using preview logic)
        return await videoApi.generateClipThumbnail(videoPath);
    }
  }, []);

  /**
   * Compose multiple clips into a single Short video
   */
  const composeShorts = useCallback(async (
    timelineClips: TimelineClip[],
    _settings: CompositionSettings, // Settings applied on backend side
    outputPath: string
  ): Promise<string> => {
    setLoading(true);
    setError(null);
    setExportStatus('exporting');
    setExportProgress(0);
    setExportError(null);

    try {
      // Extract file paths from timeline clips
      const clipPaths = timelineClips
        .filter(clip => clip.file_path && clip.file_path.length > 0)
        .map(clip => clip.file_path);

      if (clipPaths.length === 0) {
        throw new Error("No valid clips to export");
      }

      const result = await invoke<string>('compose_shorts', {
        clipPaths,
        outputPath
      });

      setExportStatus('complete');
      setExportOutputPath(result);
      return result;
    } catch (err) {
      const errorMsg = getErrorMessage(err);
      setError(errorMsg);
      setExportStatus('error');
      setExportError(errorMsg);
      throw err;
    } finally {
      setLoading(false);
    }
  }, [setExportStatus, setExportProgress, setExportError, setExportOutputPath]);

  /**
   * Extract a single clip from game footage
   * Updated to match Backend Signature: inputPath, outputPath, startTime, duration
   */
  const extractClip = useCallback(async (
    inputPath: string,
    outputPath: string,
    startTime: number,
    duration: number
  ): Promise<string> => {
    setLoading(true);
    setError(null);

    try {
      return await videoApi.extractClip(inputPath, outputPath, startTime, duration);
    } catch (err) {
      const errorMsg = err instanceof AppError ? err.message : 'Extraction failed';
      setError(errorMsg);
      throw err;
    } finally {
      setLoading(false);
    }
  }, []);

  /**
   * Create a long-form montage from clips
   */
  const createMontage = useCallback(async (
    timelineClips: TimelineClip[],
    outputPath: string
  ): Promise<string> => {
    setLoading(true);
    setError(null);
    setExportStatus('exporting');
    setExportProgress(0);
    setExportError(null);

    try {
      const clipPaths = timelineClips.map(c => c.file_path || '');
      // Filter out empty paths
      const validPaths = clipPaths.filter(p => p.length > 0);
      
      if (validPaths.length === 0) {
        throw new Error("No valid clips to export");
      }

      const result = await videoApi.createLongformVideo(validPaths, outputPath);

      setExportStatus('complete');
      setExportOutputPath(result);
      return result;
    } catch (err) {
      const errorMsg = err instanceof AppError ? err.message : 'Montage export failed';
      setError(errorMsg);
      setExportStatus('error');
      setExportError(errorMsg);
      throw err;
    } finally {
      setLoading(false);
    }
  }, [setExportStatus, setExportProgress, setExportError, setExportOutputPath]);

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