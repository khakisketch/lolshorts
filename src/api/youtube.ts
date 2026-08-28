import { cmd } from "./client";
import type {
  YouTubeVideo,
  AuthStatus,
  QuotaInfo,
  UploadHistoryEntry,
  UploadProgress,
  ScheduledUpload,
} from "@/types/youtube";

// Re-export types for convenience
export type {
  YouTubeVideo,
  AuthStatus,
  QuotaInfo,
  UploadHistoryEntry,
  UploadProgress,
  ScheduledUpload,
};

type RawScheduledUpload = Omit<
  ScheduledUpload,
  "status" | "error" | "schedule"
> & {
  status?: ScheduledUpload["status"] | null;
  error?: string | null;
  error_message?: string | null;
  schedule?: Partial<ScheduledUpload["schedule"]> | null;
};

const normalizeScheduledUpload = (
  item: RawScheduledUpload,
): ScheduledUpload => ({
  ...item,
  status: item.status ?? "pending",
  error: item.error ?? item.error_message ?? null,
  schedule: {
    scheduled_at: item.schedule?.scheduled_at ?? null,
    queue_position: item.schedule?.queue_position ?? null,
  },
});

// For display purposes (legacy compatibility - can be removed if unused)
export interface YouTubeVideoDetails {
  id: string;
  title: string;
  description: string;
  thumbnail_url: string;
  published_at: string;
  view_count: number;
}

export const youtubeApi = {
  startAuth: () => cmd<string>("youtube_start_auth"),

  startAuthWithServer: () => cmd<string>("youtube_start_auth_with_server"),

  completeAuth: (code: string, state: string) =>
    cmd<void>("youtube_complete_auth", { code, state }),

  getAuthStatus: () => cmd<AuthStatus>("youtube_get_auth_status"),

  logout: () => cmd<void>("youtube_logout"),

  // Updated to match backend: video_path, title, description, tags, privacy_status, thumbnail_path
  uploadVideo: (
    videoPath: string,
    title: string,
    description: string,
    tags: string[],
    privacyStatus: string,
    thumbnailPath?: string,
  ) =>
    cmd<YouTubeVideo>("youtube_upload_video", {
      video_path: videoPath,
      title,
      description,
      tags,
      privacy_status: privacyStatus,
      thumbnail_path: thumbnailPath ?? null,
    }),

  // Backend doesn't take uploadId parameter
  getUploadProgress: () =>
    cmd<UploadProgress | null>("youtube_get_upload_progress"),

  // Backend expects YouTubeVideo type
  addToHistory: (video: YouTubeVideo) =>
    cmd<void>("youtube_add_to_history", { video }),

  getUploadHistory: () =>
    cmd<UploadHistoryEntry[]>("youtube_get_upload_history"),

  getQuotaInfo: () => cmd<QuotaInfo>("youtube_get_quota_info"),

  // Get video details by ID
  getVideoDetails: (videoId: string) =>
    cmd<YouTubeVideo>("youtube_get_video_details", { video_id: videoId }),

  scheduleUpload: (
    videoPath: string,
    title: string,
    description: string,
    tags: string[],
    privacyStatus: string,
    thumbnailPath: string | undefined,
    scheduledAt: string | undefined,
  ) =>
    cmd<RawScheduledUpload>("youtube_schedule_upload", {
      params: {
        video_path: videoPath,
        title,
        description,
        tags,
        privacy_status: privacyStatus,
        thumbnail_path: thumbnailPath ?? null,
        scheduled_at: scheduledAt ?? null,
      },
    }).then(normalizeScheduledUpload),

  getUploadQueue: async () => {
    const queue = await cmd<RawScheduledUpload[]>("youtube_get_upload_queue");
    return queue.map(normalizeScheduledUpload);
  },

  cancelScheduledUpload: (id: string) =>
    cmd<void>("youtube_cancel_scheduled_upload", { id }),
};
