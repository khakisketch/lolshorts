import { useState, useCallback, useEffect, useRef } from 'react';
import {
  CanvasTemplate,
  CanvasTemplateInfo,
  AutoEditConfig,
  AutoEditProgress,
  AutoEditResult,
  VideoError,
} from '@/types/autoEdit';
import { useAutoEditStore } from '@/stores/autoEditStore';
import { videoApi } from '@/api/video';
import { getErrorMessage } from '@/lib/utils';

/**
 * Parse backend error string into structured VideoError
 */
function parseVideoError(errorString: string): VideoError {
  // Try to extract error type and recovery suggestions from the error message
  const lines = errorString.split('\n').map(line => line.trim()).filter(Boolean);

  // First line is the error message
  const message = lines[0] || errorString;

  // Try to determine error type from message
  let error_type = 'ProcessingError';
  const recovery_suggestions: string[] = [];
  let technical_details: string | undefined;

  // Parse error type patterns
  if (message.includes('not found')) {
    error_type = message.includes('FFmpeg') ? 'FfmpegNotFound' : 'FileNotFound';
  } else if (message.includes('disk space')) {
    error_type = 'InsufficientDiskSpace';
  } else if (message.includes('corrupted') || message.includes('invalid')) {
    error_type = 'CorruptedVideo';
  } else if (message.includes('No clips found')) {
    error_type = 'NoClipsFound';
  } else if (message.includes('Not enough clips')) {
    error_type = 'InsufficientClips';
  } else if (message.includes('canvas')) {
    error_type = 'CanvasApplicationError';
  } else if (message.includes('Audio mixing')) {
    error_type = 'AudioMixingError';
  } else if (message.includes('merge') || message.includes('concatenat')) {
    error_type = 'ConcatenationError';
  }

  // Extract recovery suggestions (lines starting with "Try:", "-", or bullets)
  let inRecoverySuggestions = false;
  for (const line of lines.slice(1)) {
    if (line.startsWith('Try:') || line.startsWith('Make sure') || line.startsWith('Check that')) {
      inRecoverySuggestions = true;
      continue;
    }

    if (inRecoverySuggestions || line.startsWith('-') || line.startsWith('•')) {
      const suggestion = line.replace(/^[-•]\s*/, '').trim();
      if (suggestion) {
        recovery_suggestions.push(suggestion);
      }
    } else if (line.startsWith('Technical details:')) {
      technical_details = line.replace('Technical details:', '').trim();
    }
  }

  // Default recovery suggestions if none found
  if (recovery_suggestions.length === 0) {
    recovery_suggestions.push('Try again with different settings');
    recovery_suggestions.push('Check the logs for more details');
    recovery_suggestions.push('Contact support if the issue persists');
  }

  return {
    message,
    error_type,
    recovery_suggestions,
    technical_details,
  };
}

export function useAutoEdit() {
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const {
    setProgress,
    setResult,
    setError: setStoreError,
    setJobId,
  } = useAutoEditStore();

  // Polling ref for progress updates
  const progressIntervalRef = useRef<NodeJS.Timeout | null>(null);

  /**
   * Start auto-edit job
   */
  const startAutoEdit = useCallback(async (
    config: AutoEditConfig
  ): Promise<AutoEditResult> => {
    setIsLoading(true);
    setError(null);
    setStoreError(null);

    try {
      // Call backend to start auto-edit
      const result = await videoApi.startAutoEdit(config);

      // Store job ID for progress tracking
      setJobId(result.job_id);
      setResult(result);

      return result;
    } catch (err) {
      const errorMsg = getErrorMessage(err);
      const parsedError = parseVideoError(errorMsg);
      setError(errorMsg);
      setStoreError(parsedError);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, [setJobId, setResult, setStoreError]);

  /**
   * Poll for auto-edit progress
   */
  const pollProgress = useCallback(async (): Promise<AutoEditProgress | null> => {
    try {
      const progress = await videoApi.getAutoEditProgress();

      if (progress) {
        setProgress(progress);

        if (progress.status === 'Complete' && progress.output_path) {
          const currentResult = useAutoEditStore.getState().result;

          if (currentResult) {
            if (!currentResult.job_id && progress.job_id) {
              setResult({ ...currentResult, job_id: progress.job_id });
            }
          } else {
            setResult({
              job_id: progress.job_id,
              output_path: progress.output_path,
              duration: 0,
              clips_used: 0,
              file_size_bytes: 0,
            });
          }
        }

        // Stop polling if complete or failed
        if (progress.status === 'Complete' || progress.status === 'Failed') {
          if (progressIntervalRef.current) {
            clearInterval(progressIntervalRef.current);
            progressIntervalRef.current = null;
          }
        }
      }

      return progress;
    } catch {
      // Polling failure is non-critical - will retry next interval
      return null;
    }
  }, [setProgress, setResult]);

  /**
   * Start polling for progress (call after starting auto-edit)
   */
  const startProgressPolling = useCallback((intervalMs: number = 1000) => {
    // Clear any existing interval
    if (progressIntervalRef.current) {
      clearInterval(progressIntervalRef.current);
    }

    // Start new polling interval
    progressIntervalRef.current = setInterval(() => {
      pollProgress();
    }, intervalMs);

    // Initial poll
    pollProgress();
  }, [pollProgress]);

  /**
   * Stop polling for progress
   */
  const stopProgressPolling = useCallback(() => {
    if (progressIntervalRef.current) {
      clearInterval(progressIntervalRef.current);
      progressIntervalRef.current = null;
    }
  }, []);

  /**
   * Save a canvas template
   */
  const saveCanvasTemplate = useCallback(async (
    template: CanvasTemplate
  ): Promise<void> => {
    setIsLoading(true);
    setError(null);

    try {
      await videoApi.saveCanvasTemplate(template);
    } catch (err) {
      const errorMsg = getErrorMessage(err);
      setError(errorMsg);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Load a canvas template by ID
   */
  const loadCanvasTemplate = useCallback(async (
    templateId: string
  ): Promise<CanvasTemplate> => {
    setIsLoading(true);
    setError(null);

    try {
      const template = await videoApi.loadCanvasTemplate(templateId);
      return template;
    } catch (err) {
      const errorMsg = getErrorMessage(err);
      setError(errorMsg);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * List all available canvas templates
   */
  const listCanvasTemplates = useCallback(async (): Promise<CanvasTemplateInfo[]> => {
    setIsLoading(true);
    setError(null);

    try {
      const templates = await videoApi.listCanvasTemplates();
      return templates;
    } catch (err) {
      const errorMsg = getErrorMessage(err);
      setError(errorMsg);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Delete a canvas template
   */
  const deleteCanvasTemplate = useCallback(async (
    templateId: string
  ): Promise<void> => {
    setIsLoading(true);
    setError(null);

    try {
      await videoApi.deleteCanvasTemplate(templateId);
    } catch (err) {
      const errorMsg = getErrorMessage(err);
      setError(errorMsg);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Cleanup polling on unmount
  useEffect(() => {
    return () => {
      if (progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
      }
    };
  }, []);

  return {
    isLoading,
    error,
    startAutoEdit,
    pollProgress,
    startProgressPolling,
    stopProgressPolling,
    saveCanvasTemplate,
    loadCanvasTemplate,
    listCanvasTemplates,
    deleteCanvasTemplate,
  };
}
