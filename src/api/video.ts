import { cmd } from './client';

import { CanvasTemplate, CanvasTemplateInfo, AutoEditConfig, AutoEditProgress, AutoEditResult } from '@/types/autoEdit';
import { ClipMetadata } from '@/types/storage';

export type { ClipMetadata };

interface BackendAutoEditResult {
  job_id?: string;
  output_path?: string;
  duration?: number;
  total_duration?: number;
  clips_used?: number;
  clip_count?: number;
  selected_clips?: unknown[];
  file_size_bytes?: number;
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
}

function numericValue(value: number | undefined, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function normalizeAutoEditStatus(status: string | undefined): AutoEditProgress['status'] {
  switch (status) {
    case 'Idle':
    case 'SelectingClips':
    case 'PreparingClips':
    case 'Concatenating':
    case 'ApplyingCanvas':
    case 'MixingAudio':
    case 'Complete':
    case 'Failed':
      return status;
    case 'queued':
      return 'SelectingClips';
    case 'processing':
      return 'Concatenating';
    case 'completed':
      return 'Complete';
    case 'failed':
      return 'Failed';
    default:
      return 'Idle';
  }
}

function normalizeAutoEditResult(result: BackendAutoEditResult): AutoEditResult {
  const outputPath = result.output_path ?? '';
  const clipsUsed = result.clips_used ?? result.clip_count ?? result.selected_clips?.length;

  return {
    job_id: result.job_id ?? '',
    output_path: outputPath,
    duration: numericValue(result.duration ?? result.total_duration),
    clips_used: numericValue(clipsUsed),
    file_size_bytes: numericValue(result.file_size_bytes),
  };
}

function normalizeAutoEditProgress(progress: BackendAutoEditProgress): AutoEditProgress {
  return {
    job_id: progress.job_id ?? '',
    status: normalizeAutoEditStatus(progress.status),
    progress_percentage: numericValue(progress.progress_percentage ?? progress.progress),
    current_stage: progress.current_stage ?? progress.current_step ?? '',
    clips_selected: numericValue(progress.clips_selected),
    total_clips: numericValue(progress.total_clips),
    estimated_completion_seconds: progress.estimated_completion_seconds ?? progress.estimated_seconds,
    output_path: progress.output_path ?? undefined,
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
    cmd<ClipMetadata[]>('get_clips', { game_id: gameId }),

  extractClip: (inputPath: string, outputPath: string, startTime: number, duration: number) =>
    cmd<string>('extract_clip', { input_path: inputPath, output_path: outputPath, start_time: startTime, duration }),

  /** @deprecated Use composeShortsV2, which honors per-clip trim, aspect ratio, and transitions. */
  composeShorts: (clipPaths: string[], outputPath: string) =>
    cmd<string>('compose_shorts', { clip_paths: clipPaths, output_path: outputPath }),

  composeShortsV2: (
    clips: ComposeShortsV2Clip[],
    aspectRatio: string,
    transitionType: string,
    transitionDuration: number,
    outputPath: string,
  ) =>
    cmd<string>('compose_shorts_v2', {
      clips,
      aspect_ratio: aspectRatio,
      transition_type: transitionType,
      transition_duration: transitionDuration,
      output_path: outputPath,
    }),

  generateThumbnail: (inputPath: string, outputPath: string, timeOffset: number) =>
    cmd<string>('generate_thumbnail', { input_path: inputPath, output_path: outputPath, time_offset: timeOffset }),

  generateClipThumbnail: (clipFilePath: string) =>
    cmd<string>('generate_clip_thumbnail', { clip_file_path: clipFilePath }),

  getVideoDuration: (inputPath: string) =>
    cmd<number>('get_video_duration', { input_path: inputPath }),

  deleteClip: (clipFilePath: string, gameId: string) =>
    cmd<void>('delete_clip', { clip_file_path: clipFilePath, game_id: gameId }),

  createLongformVideo: (clipPaths: string[], outputPath: string) =>
    cmd<string>('create_longform_video', { clip_paths: clipPaths, output_path: outputPath }),

  // Auto Edit
  startAutoEdit: (config: AutoEditConfig) =>
    cmd<BackendAutoEditResult>('start_auto_edit', { config }).then(normalizeAutoEditResult),

  getAutoEditProgress: () =>
    cmd<BackendAutoEditProgress | null>('get_auto_edit_progress').then((progress) =>
      progress ? normalizeAutoEditProgress(progress) : null,
    ),

  // Canvas Templates
  saveCanvasTemplate: (template: CanvasTemplate) =>
    cmd<void>('save_canvas_template', { template }),

  loadCanvasTemplate: (templateId: string) =>
    cmd<CanvasTemplate>('load_canvas_template', { template_id: templateId }),

  listCanvasTemplates: () =>
    cmd<CanvasTemplateInfo[]>('list_canvas_templates'),

  deleteCanvasTemplate: (templateId: string) =>
    cmd<void>('delete_canvas_template', { template_id: templateId }),
};
