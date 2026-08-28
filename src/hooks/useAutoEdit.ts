import { useState, useCallback, useEffect, useRef } from "react";
import {
  CanvasTemplate,
  CanvasTemplateInfo,
  AutoEditConfig,
  AutoEditJobReceipt,
  AutoEditPlan,
  AutoEditProgress,
  VideoError,
} from "@/types/autoEdit";
import { useAutoEditStore } from "@/stores/autoEditStore";
import { videoApi } from "@/api/video";
import { getErrorMessage } from "@/lib/utils";

/**
 * Parse backend error string into structured VideoError
 */
function parseVideoError(errorString: string): VideoError {
  // Try to extract error type and recovery suggestions from the error message
  const lines = errorString
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  // First line is the error message
  const message = lines[0] || errorString;

  // Try to determine error type from message
  let error_type = "ProcessingError";
  const recovery_suggestions: string[] = [];
  let technical_details: string | undefined;

  // Parse error type patterns
  if (message.includes("not found")) {
    error_type = message.includes("FFmpeg") ? "FfmpegNotFound" : "FileNotFound";
  } else if (message.includes("disk space")) {
    error_type = "InsufficientDiskSpace";
  } else if (message.includes("corrupted") || message.includes("invalid")) {
    error_type = "CorruptedVideo";
  } else if (message.includes("No clips found")) {
    error_type = "NoClipsFound";
  } else if (message.includes("Not enough clips")) {
    error_type = "InsufficientClips";
  } else if (message.includes("canvas")) {
    error_type = "CanvasApplicationError";
  } else if (message.includes("Audio mixing")) {
    error_type = "AudioMixingError";
  } else if (message.includes("merge") || message.includes("concatenat")) {
    error_type = "ConcatenationError";
  }

  // Extract recovery suggestions (lines starting with "Try:", "-", or bullets)
  let inRecoverySuggestions = false;
  for (const line of lines.slice(1)) {
    if (
      line.startsWith("Try:") ||
      line.startsWith("Make sure") ||
      line.startsWith("Check that")
    ) {
      inRecoverySuggestions = true;
      continue;
    }

    if (inRecoverySuggestions || line.startsWith("-") || line.startsWith("•")) {
      const suggestion = line.replace(/^[-•]\s*/, "").trim();
      if (suggestion) {
        recovery_suggestions.push(suggestion);
      }
    } else if (line.startsWith("Technical details:")) {
      technical_details = line.replace("Technical details:", "").trim();
    }
  }

  // Default recovery suggestions if none found
  if (recovery_suggestions.length === 0) {
    recovery_suggestions.push("Try again with different settings");
    recovery_suggestions.push("Check the logs for more details");
    recovery_suggestions.push("Contact support if the issue persists");
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
  const progressTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const pollingJobIdRef = useRef<string | null>(null);
  const isPollingRef = useRef(false);
  const startPollingRef = useRef<((jobId: string) => void) | null>(null);

  const stopProgressPolling = useCallback(() => {
    if (progressTimeoutRef.current) {
      clearTimeout(progressTimeoutRef.current);
      progressTimeoutRef.current = null;
    }
    pollingJobIdRef.current = null;
    isPollingRef.current = false;
  }, []);

  /**
   * Start auto-edit job
   */
  const startAutoEdit = useCallback(
    async (config: AutoEditConfig): Promise<AutoEditJobReceipt> => {
      setIsLoading(true);
      setError(null);
      setStoreError(null);

      try {
        // Call backend to start auto-edit
        const receipt = await videoApi.startAutoEdit(config);

        // A job receipt is deliberately not a completed result. Store an
        // immediate queued snapshot so the existing progress UI has a stable
        // lifecycle state before the first IPC poll returns.
        setJobId(receipt.job_id);
        setProgress({
          job_id: receipt.job_id,
          status: receipt.status === "Idle" ? "Queued" : receipt.status,
          progress_percentage: 0,
          current_stage: "Queued",
          clips_selected: 0,
          total_clips: 0,
          outputs: [],
        });
        startPollingRef.current?.(receipt.job_id);

        return receipt;
      } catch (err) {
        const errorMsg = getErrorMessage(err);
        const parsedError = parseVideoError(errorMsg);
        setError(errorMsg);
        setStoreError(parsedError);
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [setJobId, setProgress, setStoreError],
  );

  const resumeMediaJob = useCallback(
    async (jobId: string): Promise<AutoEditJobReceipt> => {
      setIsLoading(true);
      setError(null);
      setStoreError(null);
      try {
        const receipt = await videoApi.resumeMediaJob(jobId);
        setJobId(receipt.job_id);
        setProgress({
          job_id: receipt.job_id,
          status: "Queued",
          progress_percentage: 0,
          current_stage: "Queued",
          clips_selected: 0,
          total_clips: 0,
          outputs: [],
        });
        startPollingRef.current?.(receipt.job_id);
        return receipt;
      } finally {
        setIsLoading(false);
      }
    },
    [setJobId, setProgress, setStoreError],
  );

  /**
   * Poll for auto-edit progress
   */
  const pollProgress = useCallback(
    async (jobId?: string): Promise<AutoEditProgress | null> => {
      try {
        const activeJobId =
          jobId ?? pollingJobIdRef.current ?? useAutoEditStore.getState().jobId;
        if (!activeJobId) return null;
        const progress = await videoApi.getAutoEditProgress(activeJobId);

        if (progress) {
          setProgress(progress);

          if (progress.status === "Complete") {
            const output = progress.outputs[0];
            if (output) {
              setResult({
                job_id: output.result_id || progress.job_id,
                output_path: output.output_path,
                duration: output.duration,
                clips_used: output.clips_used,
                file_size_bytes: output.file_size_bytes,
                outputs: progress.outputs,
              });
            } else if (progress.output_path) {
              // Legacy progress payloads exposed only output_path.
              setResult({
                job_id: progress.job_id,
                output_path: progress.output_path,
                duration: 0,
                clips_used: 0,
                file_size_bytes: 0,
              });
            }
          }

          if (progress.status === "Failed") {
            const errorMessage =
              progress.error || "Auto-edit generation failed";
            setError(errorMessage);
            setStoreError(parseVideoError(errorMessage));
          } else if (progress.status === "Cancelled") {
            setError(null);
            setStoreError(null);
          }

          if (
            progress.status === "Complete" ||
            progress.status === "Failed" ||
            progress.status === "Cancelled"
          ) {
            stopProgressPolling();
          }
        }

        return progress;
      } catch {
        // Polling failure is non-critical - will retry next interval
        return null;
      }
    },
    [setProgress, setResult, setStoreError, stopProgressPolling],
  );

  /**
   * Start polling for progress (call after starting auto-edit)
   */
  const startProgressPolling = useCallback(
    (jobIdOrLegacyInterval?: string | number) => {
      const jobId =
        typeof jobIdOrLegacyInterval === "string"
          ? jobIdOrLegacyInterval
          : useAutoEditStore.getState().jobId;
      if (!jobId || isPollingRef.current) return;

      pollingJobIdRef.current = jobId;
      isPollingRef.current = true;
      let retryIndex = 0;
      const delays = [1000, 2000, 4000, 8000];

      const poll = async () => {
        const progress = await pollProgress(jobId);
        if (
          !isPollingRef.current ||
          !pollingJobIdRef.current ||
          progress?.status === "Complete" ||
          progress?.status === "Failed" ||
          progress?.status === "Cancelled"
        ) {
          return;
        }
        const delay = progress
          ? 1000
          : delays[Math.min(retryIndex, delays.length - 1)];
        retryIndex = progress ? 0 : retryIndex + 1;
        progressTimeoutRef.current = setTimeout(poll, delay);
      };

      void poll();
    },
    [pollProgress],
  );
  startPollingRef.current = (jobId) => startProgressPolling(jobId);

  const planAutoEdit = useCallback(
    (config: AutoEditConfig): Promise<AutoEditPlan> =>
      videoApi.planAutoEdit(config),
    [],
  );

  const cancelAutoEdit = useCallback(
    async (requestedJobId?: string): Promise<AutoEditProgress | null> => {
      const jobId = requestedJobId ?? useAutoEditStore.getState().jobId;
      if (!jobId) return null;
      const progress = await videoApi.cancelAutoEdit(jobId);
      setProgress(progress);
      if (
        progress.status === "Cancelled" ||
        progress.status === "Complete" ||
        progress.status === "Failed"
      ) {
        stopProgressPolling();
      }
      return progress;
    },
    [setProgress, stopProgressPolling],
  );

  /**
   * Save a canvas template
   */
  const saveCanvasTemplate = useCallback(
    async (template: CanvasTemplate): Promise<void> => {
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
    },
    [],
  );

  /**
   * Load a canvas template by ID
   */
  const loadCanvasTemplate = useCallback(
    async (templateId: string): Promise<CanvasTemplate> => {
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
    },
    [],
  );

  /**
   * List all available canvas templates
   */
  const listCanvasTemplates = useCallback(async (): Promise<
    CanvasTemplateInfo[]
  > => {
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
  const deleteCanvasTemplate = useCallback(
    async (templateId: string): Promise<void> => {
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
    },
    [],
  );

  // Cleanup polling on unmount
  useEffect(() => {
    return () => {
      stopProgressPolling();
    };
  }, [stopProgressPolling]);

  return {
    isLoading,
    error,
    startAutoEdit,
    resumeMediaJob,
    planAutoEdit,
    cancelAutoEdit,
    pollProgress,
    startProgressPolling,
    stopProgressPolling,
    saveCanvasTemplate,
    loadCanvasTemplate,
    listCanvasTemplates,
    deleteCanvasTemplate,
  };
}
