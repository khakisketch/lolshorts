use anyhow::{Context, Result};
use reqwest::{multipart, Body, Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info, warn};

use super::oauth::YouTubeOAuthClient;

/// YouTube Data API v3 base URL
const YOUTUBE_API_BASE: &str = "https://www.googleapis.com/youtube/v3";

/// YouTube Resumable Upload URL
const YOUTUBE_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/youtube/v3/videos";

/// Chunk size for resumable upload (256KB minimum, 8MB recommended)
const CHUNK_SIZE: u64 = 8 * 1024 * 1024; // 8MB

/// Maximum retry attempts for resumable upload
const MAX_RETRIES: u32 = 5;

/// Retry delay base (exponential backoff)
const RETRY_DELAY_MS: u64 = 1000;

/// Video metadata for YouTube upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub category_id: String, // 20 = Gaming
    pub privacy_status: PrivacyStatus,
    pub made_for_kids: bool,
}

/// YouTube API limits
const MAX_TITLE_LENGTH: usize = 100;
const MAX_DESCRIPTION_LENGTH: usize = 5000;
const MAX_TAG_LENGTH: usize = 500;
const MAX_TOTAL_TAGS_LENGTH: usize = 500;

impl VideoMetadata {
    /// Validate metadata against YouTube API limits
    pub fn validate(&self) -> Result<()> {
        // Validate title
        if self.title.is_empty() {
            return Err(anyhow::anyhow!("Video title cannot be empty"));
        }
        if self.title.len() > MAX_TITLE_LENGTH {
            return Err(anyhow::anyhow!(
                "Video title exceeds {} characters (current: {})",
                MAX_TITLE_LENGTH,
                self.title.len()
            ));
        }

        // Validate description
        if self.description.len() > MAX_DESCRIPTION_LENGTH {
            return Err(anyhow::anyhow!(
                "Video description exceeds {} characters (current: {})",
                MAX_DESCRIPTION_LENGTH,
                self.description.len()
            ));
        }

        // Validate individual tags
        for tag in &self.tags {
            if tag.len() > MAX_TAG_LENGTH {
                return Err(anyhow::anyhow!(
                    "Tag '{}...' exceeds {} characters",
                    &tag[..20.min(tag.len())],
                    MAX_TAG_LENGTH
                ));
            }
        }

        // Validate total tags length
        let total_tags_len: usize = self.tags.iter().map(|t| t.len()).sum();
        if total_tags_len > MAX_TOTAL_TAGS_LENGTH {
            return Err(anyhow::anyhow!(
                "Total tags length exceeds {} characters (current: {})",
                MAX_TOTAL_TAGS_LENGTH,
                total_tags_len
            ));
        }

        Ok(())
    }

    /// Ensure the upload is discoverable on YouTube's Shorts surface.
    ///
    /// If "shorts" (case-insensitive) doesn't already appear anywhere in the
    /// title, description, or tags, appends `#Shorts` to the description and
    /// adds a `Shorts` tag. Each addition is skipped individually if it would
    /// push the description or total tags length past YouTube's API limits
    /// (see [`VideoMetadata::validate`]), so this call can never turn
    /// previously-valid metadata into metadata that fails validation.
    pub fn ensure_shorts_tag(&mut self) {
        let already_tagged = self.title.to_lowercase().contains("shorts")
            || self.description.to_lowercase().contains("shorts")
            || self
                .tags
                .iter()
                .any(|tag| tag.to_lowercase().contains("shorts"));

        if already_tagged {
            return;
        }

        const SHORTS_DESCRIPTION_SUFFIX: &str = "\n#Shorts";
        const SHORTS_TAG: &str = "Shorts";

        if self.description.len() + SHORTS_DESCRIPTION_SUFFIX.len() <= MAX_DESCRIPTION_LENGTH {
            self.description.push_str(SHORTS_DESCRIPTION_SUFFIX);
        }

        let total_tags_len: usize = self.tags.iter().map(|t| t.len()).sum();
        if SHORTS_TAG.len() <= MAX_TAG_LENGTH
            && total_tags_len + SHORTS_TAG.len() <= MAX_TOTAL_TAGS_LENGTH
        {
            self.tags.push(SHORTS_TAG.to_string());
        }
    }
}

/// YouTube video privacy status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyStatus {
    Public,
    Unlisted,
    Private,
}

/// Upload progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadProgress {
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
    pub percentage: f64,
    pub status: UploadStatus,
    pub video_id: Option<String>,
    pub error: Option<String>,
}

/// Upload status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UploadStatus {
    Initializing,
    Uploading,
    Processing,
    Complete,
    Failed,
}

/// YouTube video information after upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YouTubeVideo {
    pub id: String,
    pub title: String,
    pub description: String,
    pub thumbnail_url: Option<String>,
    pub published_at: String,
    pub privacy_status: String,
    pub view_count: Option<u64>,
}

/// YouTube upload client
pub struct YouTubeUploadClient {
    oauth_client: Arc<YouTubeOAuthClient>,
    http_client: Client,
    progress: Arc<RwLock<Option<UploadProgress>>>,
}

impl YouTubeUploadClient {
    /// Create new YouTube upload client
    pub fn new(oauth_client: Arc<YouTubeOAuthClient>) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(600)) // 10 minutes for upload
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            oauth_client,
            http_client,
            progress: Arc::new(RwLock::new(None)),
        })
    }

    /// Create an upload client with isolated OAuth credentials while sharing progress state.
    pub fn with_oauth_client(&self, oauth_client: Arc<YouTubeOAuthClient>) -> Self {
        Self {
            oauth_client,
            http_client: self.http_client.clone(),
            progress: Arc::clone(&self.progress),
        }
    }

    /// Upload video to YouTube
    ///
    /// # Arguments
    /// * `video_path` - Path to video file
    /// * `metadata` - Video metadata (title, description, tags, etc.)
    /// * `thumbnail_path` - Optional path to custom thumbnail
    pub async fn upload_video(
        &self,
        video_path: &Path,
        metadata: VideoMetadata,
        thumbnail_path: Option<&Path>,
    ) -> Result<YouTubeVideo> {
        info!("Starting YouTube video upload: {}", video_path.display());

        // Validate metadata before upload
        metadata.validate().context("Invalid video metadata")?;

        // Initialize progress
        self.update_progress(UploadProgress {
            bytes_uploaded: 0,
            total_bytes: 0,
            percentage: 0.0,
            status: UploadStatus::Initializing,
            video_id: None,
            error: None,
        })
        .await;

        // Get valid access token
        let access_token = self
            .oauth_client
            .get_valid_token()
            .await
            .context("Failed to get valid access token")?;

        // Open video file and get size without loading into memory
        let file = File::open(video_path)
            .await
            .context("Failed to open video file")?;
        let file_size = file
            .metadata()
            .await
            .context("Failed to get file metadata")?
            .len();

        debug!("Video file size: {} bytes", file_size);

        // Update progress to uploading
        self.update_progress(UploadProgress {
            bytes_uploaded: 0,
            total_bytes: file_size,
            percentage: 0.0,
            status: UploadStatus::Uploading,
            video_id: None,
            error: None,
        })
        .await;

        // Create video resource JSON
        let video_resource = serde_json::json!({
            "snippet": {
                "title": metadata.title,
                "description": metadata.description,
                "tags": metadata.tags,
                "categoryId": metadata.category_id,
            },
            "status": {
                "privacyStatus": format!("{:?}", metadata.privacy_status).to_lowercase(),
                "madeForKids": metadata.made_for_kids,
                "selfDeclaredMadeForKids": metadata.made_for_kids,
            }
        });

        // Create multipart form with streaming video data
        let part_metadata = multipart::Part::text(video_resource.to_string())
            .mime_str("application/json")
            .context("Failed to create metadata part")?;

        // Stream video file using ReaderStream to avoid loading entire file into memory
        let stream = ReaderStream::new(file);
        let body = Body::wrap_stream(stream);
        let part_video = multipart::Part::stream(body)
            .mime_str("video/*")
            .context("Failed to create video part")?;

        let form = multipart::Form::new()
            .part("snippet", part_metadata)
            .part("media", part_video);

        // Upload video
        let upload_url = format!(
            "{}/videos?uploadType=multipart&part=snippet,status",
            YOUTUBE_API_BASE
        );

        let response = self
            .http_client
            .post(&upload_url)
            .bearer_auth(&access_token)
            .multipart(form)
            .send()
            .await
            .context("Failed to send upload request")?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("Upload failed: {}", error_text);

            self.update_progress(UploadProgress {
                bytes_uploaded: 0,
                total_bytes: file_size,
                percentage: 0.0,
                status: UploadStatus::Failed,
                video_id: None,
                error: Some(error_text.clone()),
            })
            .await;

            return Err(anyhow::anyhow!("YouTube upload failed: {}", error_text));
        }

        let upload_response: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse upload response")?;

        let video_id = upload_response["id"]
            .as_str()
            .context("No video ID in response")?
            .to_string();

        info!("Video uploaded successfully: {}", video_id);

        // Update progress to processing
        self.update_progress(UploadProgress {
            bytes_uploaded: file_size,
            total_bytes: file_size,
            percentage: 100.0,
            status: UploadStatus::Processing,
            video_id: Some(video_id.clone()),
            error: None,
        })
        .await;

        // Upload custom thumbnail if provided
        if let Some(thumb_path) = thumbnail_path {
            if let Err(e) = self.upload_thumbnail(&video_id, thumb_path).await {
                warn!("Failed to upload thumbnail: {}", e);
            }
        }

        // Get video details
        let video = self.get_video_details(&video_id).await?;

        // Mark as complete
        self.update_progress(UploadProgress {
            bytes_uploaded: file_size,
            total_bytes: file_size,
            percentage: 100.0,
            status: UploadStatus::Complete,
            video_id: Some(video_id.clone()),
            error: None,
        })
        .await;

        Ok(video)
    }

    /// Upload custom thumbnail for video
    async fn upload_thumbnail(&self, video_id: &str, thumbnail_path: &Path) -> Result<()> {
        info!(
            "Uploading custom thumbnail for video {}: {}",
            video_id,
            thumbnail_path.display()
        );

        let access_token = self.oauth_client.get_valid_token().await?;

        // Read thumbnail file
        let mut file = File::open(thumbnail_path).await?;
        let mut thumbnail_data = Vec::new();
        file.read_to_end(&mut thumbnail_data).await?;

        let thumbnail_url = format!("{}/thumbnails/set?videoId={}", YOUTUBE_API_BASE, video_id);

        let response = self
            .http_client
            .post(&thumbnail_url)
            .bearer_auth(&access_token)
            .header("Content-Type", "image/jpeg")
            .body(thumbnail_data)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Thumbnail upload failed: {}", error_text));
        }

        info!("Thumbnail uploaded successfully");
        Ok(())
    }

    /// Get video details from YouTube
    pub async fn get_video_details(&self, video_id: &str) -> Result<YouTubeVideo> {
        let access_token = self.oauth_client.get_valid_token().await?;

        let url = format!(
            "{}/videos?part=snippet,status,statistics&id={}",
            YOUTUBE_API_BASE, video_id
        );

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Failed to get video details: {}",
                error_text
            ));
        }

        let data: serde_json::Value = response.json().await?;
        let items = data["items"].as_array().context("No items in response")?;

        if items.is_empty() {
            return Err(anyhow::anyhow!("Video not found: {}", video_id));
        }

        let video = &items[0];

        Ok(YouTubeVideo {
            id: video_id.to_string(),
            title: video["snippet"]["title"].as_str().unwrap_or("").to_string(),
            description: video["snippet"]["description"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            thumbnail_url: video["snippet"]["thumbnails"]["high"]["url"]
                .as_str()
                .map(|s| s.to_string()),
            published_at: video["snippet"]["publishedAt"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            privacy_status: video["status"]["privacyStatus"]
                .as_str()
                .unwrap_or("private")
                .to_string(),
            view_count: video["statistics"]["viewCount"]
                .as_str()
                .and_then(|s| s.parse().ok()),
        })
    }

    /// Get current upload progress
    pub async fn get_progress(&self) -> Option<UploadProgress> {
        self.progress.read().await.clone()
    }

    /// Update upload progress
    async fn update_progress(&self, progress: UploadProgress) {
        let mut p = self.progress.write().await;
        *p = Some(progress);
    }

    /// Clear upload progress
    pub async fn clear_progress(&self) {
        let mut p = self.progress.write().await;
        *p = None;
    }

    /// Upload video using Resumable Upload protocol
    ///
    /// This is the recommended method for large files as it:
    /// - Supports resume on network failures
    /// - Provides accurate progress tracking
    /// - Handles large files efficiently
    ///
    /// # Arguments
    /// * `video_path` - Path to video file
    /// * `metadata` - Video metadata (title, description, tags, etc.)
    /// * `thumbnail_path` - Optional path to custom thumbnail
    pub async fn upload_video_resumable(
        &self,
        video_path: &Path,
        metadata: VideoMetadata,
        thumbnail_path: Option<&Path>,
    ) -> Result<YouTubeVideo> {
        info!(
            "Starting resumable YouTube upload: {}",
            video_path.display()
        );

        // Validate metadata before upload
        metadata.validate().context("Invalid video metadata")?;

        // Get file size
        let file_metadata = tokio::fs::metadata(video_path)
            .await
            .context("Failed to get file metadata")?;
        let file_size = file_metadata.len();

        info!(
            "Video file size: {} bytes ({:.2} MB)",
            file_size,
            file_size as f64 / 1024.0 / 1024.0
        );

        // Initialize progress
        self.update_progress(UploadProgress {
            bytes_uploaded: 0,
            total_bytes: file_size,
            percentage: 0.0,
            status: UploadStatus::Initializing,
            video_id: None,
            error: None,
        })
        .await;

        // Step 1: Initialize resumable upload session
        let upload_url = self.init_resumable_upload(&metadata, file_size).await?;
        info!("Resumable upload session initialized");

        // Step 2: Upload file in chunks with progress tracking
        let video_id = self
            .upload_chunks(video_path, &upload_url, file_size)
            .await?;

        // Step 3: Update progress to processing
        self.update_progress(UploadProgress {
            bytes_uploaded: file_size,
            total_bytes: file_size,
            percentage: 100.0,
            status: UploadStatus::Processing,
            video_id: Some(video_id.clone()),
            error: None,
        })
        .await;

        // Step 4: Upload custom thumbnail if provided
        if let Some(thumb_path) = thumbnail_path {
            if let Err(e) = self.upload_thumbnail(&video_id, thumb_path).await {
                warn!("Failed to upload thumbnail: {}", e);
            }
        }

        // Step 5: Get video details
        let video = self.get_video_details(&video_id).await?;

        // Mark as complete
        self.update_progress(UploadProgress {
            bytes_uploaded: file_size,
            total_bytes: file_size,
            percentage: 100.0,
            status: UploadStatus::Complete,
            video_id: Some(video_id.clone()),
            error: None,
        })
        .await;

        info!("Resumable upload completed successfully: {}", video_id);
        Ok(video)
    }

    /// Initialize a resumable upload session
    async fn init_resumable_upload(
        &self,
        metadata: &VideoMetadata,
        file_size: u64,
    ) -> Result<String> {
        let access_token = self
            .oauth_client
            .get_valid_token()
            .await
            .context("Failed to get valid access token")?;

        // Create video resource JSON
        let video_resource = serde_json::json!({
            "snippet": {
                "title": metadata.title,
                "description": metadata.description,
                "tags": metadata.tags,
                "categoryId": metadata.category_id,
            },
            "status": {
                "privacyStatus": format!("{:?}", metadata.privacy_status).to_lowercase(),
                "madeForKids": metadata.made_for_kids,
                "selfDeclaredMadeForKids": metadata.made_for_kids,
            }
        });

        let init_url = format!(
            "{}?uploadType=resumable&part=snippet,status",
            YOUTUBE_UPLOAD_BASE
        );

        let response = self
            .http_client
            .post(&init_url)
            .bearer_auth(&access_token)
            .header("Content-Type", "application/json; charset=UTF-8")
            .header("X-Upload-Content-Length", file_size.to_string())
            .header("X-Upload-Content-Type", "video/*")
            .json(&video_resource)
            .send()
            .await
            .context("Failed to initialize resumable upload")?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("Failed to initialize resumable upload: {}", error_text);
            return Err(anyhow::anyhow!(
                "Failed to initialize resumable upload: {}",
                error_text
            ));
        }

        // Get the upload URL from the Location header
        let upload_url = response
            .headers()
            .get("location")
            .context("No upload URL in response")?
            .to_str()
            .context("Invalid upload URL")?
            .to_string();

        debug!("Resumable upload URL: {}", upload_url);
        Ok(upload_url)
    }

    /// Upload file chunks with progress tracking and retry logic
    async fn upload_chunks(
        &self,
        video_path: &Path,
        upload_url: &str,
        file_size: u64,
    ) -> Result<String> {
        let mut file = File::open(video_path)
            .await
            .context("Failed to open video file")?;

        let mut bytes_uploaded: u64 = 0;
        let mut retry_count = 0;

        // Update status to uploading
        self.update_progress(UploadProgress {
            bytes_uploaded: 0,
            total_bytes: file_size,
            percentage: 0.0,
            status: UploadStatus::Uploading,
            video_id: None,
            error: None,
        })
        .await;

        loop {
            // Calculate chunk range
            let chunk_start = bytes_uploaded;
            let chunk_end = std::cmp::min(chunk_start + CHUNK_SIZE, file_size);
            let chunk_size = chunk_end - chunk_start;

            if chunk_size == 0 {
                break;
            }

            // Seek to the correct position
            file.seek(std::io::SeekFrom::Start(chunk_start))
                .await
                .context("Failed to seek in file")?;

            // Read chunk
            let mut chunk_data = vec![0u8; chunk_size as usize];
            file.read_exact(&mut chunk_data)
                .await
                .context("Failed to read chunk")?;

            // Get fresh access token (may need refresh for long uploads)
            let access_token = self
                .oauth_client
                .get_valid_token()
                .await
                .context("Failed to get valid access token")?;

            // Upload chunk
            let content_range = format!("bytes {}-{}/{}", chunk_start, chunk_end - 1, file_size);
            debug!("Uploading chunk: {}", content_range);

            let response = self
                .http_client
                .put(upload_url)
                .bearer_auth(&access_token)
                .header("Content-Length", chunk_size.to_string())
                .header("Content-Range", &content_range)
                .header("Content-Type", "video/*")
                .body(chunk_data)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    match status {
                        // Upload complete (200 or 201)
                        StatusCode::OK | StatusCode::CREATED => {
                            let upload_response: serde_json::Value = resp
                                .json()
                                .await
                                .context("Failed to parse upload response")?;

                            let video_id = upload_response["id"]
                                .as_str()
                                .context("No video ID in response")?
                                .to_string();

                            return Ok(video_id);
                        }
                        // Chunk accepted, continue (308 Resume Incomplete)
                        StatusCode::PERMANENT_REDIRECT => {
                            // Extract bytes uploaded from Range header
                            if let Some(range) = resp.headers().get("range") {
                                let range_str = range.to_str().unwrap_or("");
                                if let Some(end) = range_str.strip_prefix("bytes=0-") {
                                    if let Ok(end_byte) = end.parse::<u64>() {
                                        bytes_uploaded = end_byte + 1;
                                    }
                                }
                            } else {
                                bytes_uploaded = chunk_end;
                            }

                            // Update progress
                            let percentage = (bytes_uploaded as f64 / file_size as f64) * 100.0;
                            self.update_progress(UploadProgress {
                                bytes_uploaded,
                                total_bytes: file_size,
                                percentage,
                                status: UploadStatus::Uploading,
                                video_id: None,
                                error: None,
                            })
                            .await;

                            debug!(
                                "Chunk uploaded: {} / {} ({:.1}%)",
                                bytes_uploaded, file_size, percentage
                            );
                            retry_count = 0; // Reset retry count on success
                        }
                        // Client error - don't retry
                        StatusCode::BAD_REQUEST
                        | StatusCode::UNAUTHORIZED
                        | StatusCode::FORBIDDEN => {
                            let error_text = resp
                                .text()
                                .await
                                .unwrap_or_else(|_| "Unknown error".to_string());
                            error!("Upload failed with client error: {}", error_text);

                            self.update_progress(UploadProgress {
                                bytes_uploaded,
                                total_bytes: file_size,
                                percentage: (bytes_uploaded as f64 / file_size as f64) * 100.0,
                                status: UploadStatus::Failed,
                                video_id: None,
                                error: Some(error_text.clone()),
                            })
                            .await;

                            return Err(anyhow::anyhow!("Upload failed: {}", error_text));
                        }
                        // Server error or rate limit - retry
                        _ => {
                            warn!("Upload chunk failed with status {}, retrying...", status);
                            retry_count += 1;

                            if retry_count >= MAX_RETRIES {
                                let error_msg =
                                    format!("Upload failed after {} retries", MAX_RETRIES);
                                self.update_progress(UploadProgress {
                                    bytes_uploaded,
                                    total_bytes: file_size,
                                    percentage: (bytes_uploaded as f64 / file_size as f64) * 100.0,
                                    status: UploadStatus::Failed,
                                    video_id: None,
                                    error: Some(error_msg.clone()),
                                })
                                .await;
                                return Err(anyhow::anyhow!("{}", error_msg));
                            }

                            // Exponential backoff
                            let delay = RETRY_DELAY_MS * 2u64.pow(retry_count - 1);
                            warn!(
                                "Retrying in {}ms (attempt {}/{})",
                                delay, retry_count, MAX_RETRIES
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;

                            // Query upload status to find where to resume
                            if let Some(resume_byte) = self.query_upload_status(upload_url).await {
                                bytes_uploaded = resume_byte;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Network error during upload: {}", e);
                    retry_count += 1;

                    if retry_count >= MAX_RETRIES {
                        let error_msg =
                            format!("Upload failed after {} retries: {}", MAX_RETRIES, e);
                        self.update_progress(UploadProgress {
                            bytes_uploaded,
                            total_bytes: file_size,
                            percentage: (bytes_uploaded as f64 / file_size as f64) * 100.0,
                            status: UploadStatus::Failed,
                            video_id: None,
                            error: Some(error_msg.clone()),
                        })
                        .await;
                        return Err(anyhow::anyhow!("{}", error_msg));
                    }

                    // Exponential backoff
                    let delay = RETRY_DELAY_MS * 2u64.pow(retry_count - 1);
                    warn!(
                        "Retrying in {}ms (attempt {}/{})",
                        delay, retry_count, MAX_RETRIES
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;

                    // Query upload status to find where to resume
                    if let Some(resume_byte) = self.query_upload_status(upload_url).await {
                        bytes_uploaded = resume_byte;
                    }
                }
            }
        }

        Err(anyhow::anyhow!("Upload completed without video ID"))
    }

    /// Query the upload status to determine how many bytes were uploaded
    async fn query_upload_status(&self, upload_url: &str) -> Option<u64> {
        let access_token = match self.oauth_client.get_valid_token().await {
            Ok(token) => token,
            Err(_) => return None,
        };

        let response = self
            .http_client
            .put(upload_url)
            .bearer_auth(&access_token)
            .header("Content-Length", "0")
            .header("Content-Range", "bytes */*")
            .send()
            .await;

        match response {
            Ok(resp) => {
                if let Some(range) = resp.headers().get("range") {
                    let range_str = range.to_str().unwrap_or("");
                    if let Some(end) = range_str.strip_prefix("bytes=0-") {
                        if let Ok(end_byte) = end.parse::<u64>() {
                            return Some(end_byte + 1);
                        }
                    }
                }
                None
            }
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_metadata_creation() {
        let metadata = VideoMetadata {
            title: "Test Video".to_string(),
            description: "Test Description".to_string(),
            tags: vec!["gaming".to_string(), "lol".to_string()],
            category_id: "20".to_string(),
            privacy_status: PrivacyStatus::Private,
            made_for_kids: false,
        };

        assert_eq!(metadata.title, "Test Video");
        assert_eq!(metadata.tags.len(), 2);
    }

    fn base_metadata() -> VideoMetadata {
        VideoMetadata {
            title: "Pentakill montage".to_string(),
            description: "Insane pentakill from ranked solo queue".to_string(),
            tags: vec!["gaming".to_string(), "lol".to_string()],
            category_id: "20".to_string(),
            privacy_status: PrivacyStatus::Private,
            made_for_kids: false,
        }
    }

    #[test]
    fn ensure_shorts_tag_appends_hashtag_and_tag_when_absent() {
        let mut metadata = base_metadata();
        metadata.ensure_shorts_tag();

        assert!(metadata.description.ends_with("\n#Shorts"));
        assert!(metadata.tags.iter().any(|t| t == "Shorts"));
        assert!(metadata.validate().is_ok());
    }

    #[test]
    fn ensure_shorts_tag_is_noop_when_title_already_mentions_shorts() {
        let mut metadata = base_metadata();
        metadata.title = "Pentakill Shorts".to_string();
        let description_before = metadata.description.clone();
        let tags_before = metadata.tags.clone();

        metadata.ensure_shorts_tag();

        assert_eq!(metadata.description, description_before);
        assert_eq!(metadata.tags, tags_before);
    }

    #[test]
    fn ensure_shorts_tag_is_noop_when_description_already_mentions_shorts_case_insensitive() {
        let mut metadata = base_metadata();
        metadata.description = "Check out my #SHORTS clip".to_string();
        let tags_before = metadata.tags.clone();

        metadata.ensure_shorts_tag();

        assert_eq!(metadata.tags, tags_before);
    }

    #[test]
    fn ensure_shorts_tag_is_noop_when_tag_already_present() {
        let mut metadata = base_metadata();
        metadata.tags.push("shorts".to_string());
        let description_before = metadata.description.clone();

        metadata.ensure_shorts_tag();

        assert_eq!(metadata.description, description_before);
    }

    #[test]
    fn ensure_shorts_tag_skips_description_append_when_it_would_exceed_limit() {
        let mut metadata = base_metadata();
        metadata.description = "x".repeat(MAX_DESCRIPTION_LENGTH);
        let description_before = metadata.description.clone();

        metadata.ensure_shorts_tag();

        // Description untouched (would exceed limit), but tag can still be added.
        assert_eq!(metadata.description, description_before);
        assert!(metadata.tags.iter().any(|t| t == "Shorts"));
        assert!(metadata.validate().is_ok());
    }

    #[test]
    fn ensure_shorts_tag_skips_tag_append_when_total_tags_would_exceed_limit() {
        let mut metadata = base_metadata();
        metadata.tags = vec!["x".repeat(MAX_TOTAL_TAGS_LENGTH)];
        let tags_before = metadata.tags.clone();

        metadata.ensure_shorts_tag();

        assert_eq!(metadata.tags, tags_before);
        assert!(metadata.description.ends_with("\n#Shorts"));
    }

    #[test]
    fn test_upload_progress_percentage() {
        let progress = UploadProgress {
            bytes_uploaded: 5000,
            total_bytes: 10000,
            percentage: 50.0,
            status: UploadStatus::Uploading,
            video_id: None,
            error: None,
        };

        assert_eq!(progress.percentage, 50.0);
        assert_eq!(progress.status, UploadStatus::Uploading);
    }

    #[test]
    fn test_privacy_status_serialization() {
        let json = serde_json::to_string(&PrivacyStatus::Public).unwrap();
        assert_eq!(json, "\"public\"");

        let json = serde_json::to_string(&PrivacyStatus::Unlisted).unwrap();
        assert_eq!(json, "\"unlisted\"");

        let json = serde_json::to_string(&PrivacyStatus::Private).unwrap();
        assert_eq!(json, "\"private\"");
    }
}
