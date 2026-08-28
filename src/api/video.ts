import { cmd } from "./client";

import {
  CanvasTemplate,
  CanvasTemplateInfo,
  AutoEditConfig,
  AutoEditJobReceipt,
  AutoEditOutput,
  AutoEditPlan,
  AutoEditProgress,
  MediaJobSnapshot,
  OutputValidationReport,
} from "@/types/autoEdit";
import { ClipMetadata } from "@/types/storage";

export type { ClipMetadata };

interface BackendAutoEditReceipt {
  job_id?: string;
  status?: string;
}

interface BackendAutoEditProgress {
  job_id?: string;
  status?: string;
  progress_percentage?: number;
  progress?: number;
  current_stage?: string;
  current_step?: string;
  clips_selected?: number;
  total_clips?: number;
  estimated_completion_seconds?: number;
  estimated_seconds?: number;
  output_path?: string | null;
  outputs?: BackendAutoEditOutput[] | null;
  error?: string | null;
}

interface BackendAutoEditOutput {
  result_id?: string;
  output_path?: string;
  duration?: number;
  clips_used?: number;
  file_size_bytes?: number;
  output_kind?: string;
  part_index?: number | null;
  part_count?: number | null;
}

function numericValue(value: number | undefined, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function normalizeAutoEditStatus(
  status: string | undefined,
): AutoEditProgress["status"] {
  switch (status) {
    case "Idle":
    case "Queued":
    case "SelectingClips":
    case "PreparingClips":
    case "Concatenating":
    case "ApplyingCanvas":
    case "MixingAudio":
    case "Complete":
    case "Failed":
    case "Cancelled":
      return status;
    case "queued":
      return "Queued";
    case "processing":
      return "Concatenating";
    case "completed":
    case "Completed":
      return "Complete";
    case "failed":
      return "Failed";
    case "cancelled":
      return "Cancelled";
    default:
      return "Idle";
  }
}

function normalizeAutoEditReceipt(
  receipt: BackendAutoEditReceipt,
): AutoEditJobReceipt {
  return {
    job_id: receipt.job_id ?? "",
    status: normalizeAutoEditStatus(receipt.status),
  };
}

function normalizeAutoEditOutput(
  output: BackendAutoEditOutput,
): AutoEditOutput {
  const outputKinds: Record<string, AutoEditOutput["output_kind"]> = {
    short: "short",
    short_series_part: "short_series_part",
    vertical_video: "vertical_video",
  };
  return {
    result_id: output.result_id ?? "",
    output_path: output.output_path ?? "",
    duration: numericValue(output.duration),
    clips_used: numericValue(output.clips_used),
    file_size_bytes: numericValue(output.file_size_bytes),
    output_kind: outputKinds[output.output_kind ?? ""] ?? "short",
    part_index: output.part_index ?? undefined,
    part_count: output.part_count ?? undefined,
  };
}

function normalizeAutoEditProgress(
  progress: BackendAutoEditProgress,
): AutoEditProgress {
  return {
    job_id: progress.job_id ?? "",
    status: normalizeAutoEditStatus(progress.status),
    progress_percentage: numericValue(
      progress.progress_percentage ?? progress.progress,
    ),
    current_stage: progress.current_stage ?? progress.current_step ?? "",
    clips_selected: numericValue(progress.clips_selected),
    total_clips: numericValue(progress.total_clips),
    estimated_completion_seconds:
      progress.estimated_completion_seconds ?? progress.estimated_seconds,
    output_path: progress.output_path ?? undefined,
    outputs: (progress.outputs ?? []).map(normalizeAutoEditOutput),
    error: progress.error ?? undefined,
  };
}

export interface ComposeShortsV2Clip {
  path: string;
  trim_start?: number;
  /** Seconds to cut from the *end* of the clip (backend converts to an absolute duration). */
  trim_end?: number;
}

export const videoApi = {
  getClips: (gameId: string) =>
    cmd<ClipMetadata[]>("get_clips", { game_id: gameId }),

  extractClip: (
    inputPath: string,
    outputPath: string,
    startTime: number,
    duration: number,
  ) =>
    cmd<string>("extract_clip", {
      input_path: inputPath,
      output_path: outputPath,
      start_time: startTime,
      duration,
    }),

  /** @deprecated Use composeShortsV2, which honors per-clip trim, aspect ratio, and transitions. */
  composeShorts: (clipPaths: string[], outputPath: string) =>
    cmd<string>("compose_shorts", {
      clip_paths: clipPaths,
      output_path: outputPath,
    }),

  composeShortsV2: (
    clips: ComposeShortsV2Clip[],
    aspectRatio: string,
    transitionType: string,
    transitionDuration: number,
    outputPath: string,
  ) =>
    cmd<string>("compose_shorts_v2", {
      clips,
      aspect_ratio: aspectRatio,
      transition_type: transitionType,
      transition_duration: transitionDuration,
      output_path: outputPath,
    }),

  generateThumbnail: (
    inputPath: string,
    outputPath: string,
    timeOffset: number,
  ) =>
    cmd<string>("generate_thumbnail", {
      input_path: inputPath,
      output_path: outputPath,
      time_offset: timeOffset,
    }),

  generateClipThumbnail: (clipFilePath: string) =>
    cmd<string>("generate_clip_thumbnail", { clip_file_path: clipFilePath }),

  getVideoDuration: (inputPath: string) =>
    cmd<number>("get_video_duration", { input_path: inputPath }),

  deleteClip: (clipFilePath: string, gameId: string) =>
    cmd<void>("delete_clip", { clip_file_path: clipFilePath, game_id: gameId }),

  createLongformVideo: (clipPaths: string[], outputPath: string) =>
    cmd<string>("create_longform_video", {
      clip_paths: clipPaths,
      output_path: outputPath,
    }),

  // Auto Edit
  planAutoEdit: (config: AutoEditConfig) =>
    cmd<AutoEditPlan>("plan_auto_edit", { config }),

  startAutoEdit: (config: AutoEditConfig) =>
    cmd<BackendAutoEditReceipt>("start_auto_edit", { config }).then(
      normalizeAutoEditReceipt,
    ),

  getAutoEditProgress: (jobId: string) =>
    cmd<BackendAutoEditProgress | null>("get_auto_edit_progress", {
      job_id: jobId,
    }).then((progress) =>
      progress ? normalizeAutoEditProgress(progress) : null,
    ),

  cancelAutoEdit: (jobId: string) =>
    cmd<BackendAutoEditProgress>("cancel_auto_edit", { job_id: jobId }).then(
      normalizeAutoEditProgress,
    ),

  exportAutoEditForPlatform: (
    resultId: string,
    platformPreset: "youtube_shorts" | "tiktok" | "instagram_reels",
  ) =>
    cmd<string>("export_auto_edit_for_platform", {
      result_id: resultId,
      platform_preset: platformPreset,
    }),

  getMediaJob: (jobId: string) =>
    cmd<MediaJobSnapshot>("get_media_job", { job_id: jobId }),

  listRecoverableMediaJobs: () =>
    cmd<MediaJobSnapshot[]>("list_recoverable_media_jobs"),

  pauseMediaJob: (jobId: string) =>
    cmd<MediaJobSnapshot>("pause_media_job", { job_id: jobId }),

  resumeMediaJob: (jobId: string) =>
    cmd<BackendAutoEditReceipt>("resume_media_job", { job_id: jobId }).then(
      normalizeAutoEditReceipt,
    ),

  discardMediaJob: (jobId: string) =>
    cmd<void>("discard_media_job", { job_id: jobId }),

  startPlatformExport: (
    resultId: string,
    platformPreset: "youtube_shorts" | "tiktok" | "instagram_reels",
  ) =>
    cmd<BackendAutoEditReceipt>("start_platform_export", {
      result_id: resultId,
      platform_preset: platformPreset,
    }).then(normalizeAutoEditReceipt),

  revalidateAutoEditResult: (resultId: string) =>
    cmd<OutputValidationReport>("revalidate_auto_edit_result", {
      result_id: resultId,
    }),

  // Canvas Templates
  saveCanvasTemplate: (template: CanvasTemplate) =>
    cmd<void>("save_canvas_template", { template }),

  loadCanvasTemplate: (templateId: string) =>
    cmd<CanvasTemplate>("load_canvas_template", { template_id: templateId }),

  listCanvasTemplates: () => cmd<CanvasTemplateInfo[]>("list_canvas_templates"),

  deleteCanvasTemplate: (templateId: string) =>
    cmd<void>("delete_canvas_template", { template_id: templateId }),
};
