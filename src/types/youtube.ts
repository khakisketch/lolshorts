// YouTube integration type definitions
// Aligned with Rust backend: src-tauri/src/youtube/

/**
 * YouTube video type (matches YouTubeVideo in Rust upload.rs)
 */
export interface YouTubeVideo {
  id: string;
  title: string;
  description: string;
  thumbnail_url: string | null;
  published_at: string;
  privacy_status: string;
  view_count: number | null;
}

/**
 * Privacy status enum (matches PrivacyStatus in Rust)
 */
export type PrivacyStatus = "public" | "unlisted" | "private";

/**
 * Video metadata for upload (matches VideoMetadata in Rust)
 */
export interface VideoMetadata {
  title: string;
  description: string;
  tags: string[];
  category_id: string;
  privacy_status: PrivacyStatus;
  made_for_kids: boolean;
}

/**
 * Upload progress (matches UploadProgress in Rust upload.rs)
 */
export interface UploadProgress {
  bytes_uploaded: number;
  total_bytes: number;
  percentage: number; // 0-100
  status: UploadStatus;
  video_id: string | null;
  error: string | null;
}

// Matches Rust UploadStatus enum (lowercase serialization)
export type UploadStatus =
  | "initializing"
  | "uploading"
  | "processing"
  | "complete"
  | "failed";

/**
 * Upload history entry (matches UploadHistoryEntry in Rust)
 */
export interface UploadHistoryEntry {
  video_id: string;
  title: string;
  uploaded_at: number; // Unix timestamp
  privacy_status: string;
  thumbnail_url: string | null;
  view_count: number | null;
}

/**
 * Quota information (matches QuotaInfo in Rust)
 */
export interface QuotaInfo {
  daily_limit: number;
  used: number;
  remaining: number;
  reset_at: number; // Unix timestamp (midnight Pacific Time)
}

/**
 * Authentication status (matches AuthStatus in Rust)
 */
export interface AuthStatus {
  authenticated: boolean;
  expires_at: number | null; // Unix timestamp
  has_refresh_token: boolean;
}

/**
 * Schedule info for a queued upload (matches UploadSchedule in Rust)
 */
export interface UploadSchedule {
  scheduled_at: string | null; // ISO 8601
  queue_position: number | null;
}

// Matches Rust ScheduledUploadStatus enum (lowercase serialization)
export type ScheduledUploadStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

/**
 * Pending scheduled upload entry (matches ScheduledUpload in Rust)
 */
export interface ScheduledUpload {
  id: string;
  video_path: string;
  title: string;
  description: string;
  tags: string[];
  privacy_status: string;
  thumbnail_path: string | null;
  schedule: UploadSchedule;
  created_at: number; // Unix timestamp
  status: ScheduledUploadStatus;
  error: string | null;
  error_message?: string | null; // Backend compatibility field
  attempts?: number;
  completed_video_id?: string | null;
  updated_at?: number | null;
}
