use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::{Mutex, MutexGuard, RwLock};
use tracing::{error, info, warn};

use chrono::Utc;
use chrono_tz::America::Los_Angeles;

use super::callback_server::CallbackServer;
use super::models::{
    AuthStatus, QuotaInfo, ScheduleUploadParams, ScheduledUpload, ScheduledUploadEvent,
    ScheduledUploadStatus, UploadHistoryEntry, UploadSchedule,
};
use super::oauth::{YouTubeCredentials, YouTubeOAuthClient};
use super::upload::{
    PrivacyStatus, UploadProgress, VideoMetadata, YouTubeUploadClient, YouTubeVideo,
};
use crate::auth::command_policy;
use crate::auth::middleware::require_tier;
use crate::auth::{AuthManager, SubscriptionTier, User};
use crate::error::{AppError, AppResult};
use crate::storage::{Storage, StorageError};
use crate::utils::security;
use crate::AppState;

/// Retry an async operation with exponential backoff.
///
/// Retries up to `max_retries` times, doubling the delay each time starting at 1 second.
/// Returns the first successful result, or the last error if all attempts fail.
async fn retry_with_backoff<F, Fut, T>(
    max_retries: u32,
    operation_name: &str,
    mut operation: F,
) -> Result<T, crate::error::AppError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, crate::error::AppError>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < max_retries => {
                let delay = std::time::Duration::from_secs(1 << attempt);
                tracing::warn!(
                    "{}: retry {}/{} after {:?}: {}",
                    operation_name,
                    attempt + 1,
                    max_retries,
                    delay,
                    e
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(e) => {
                crate::utils::telemetry::capture_operational_error(
                    "youtube",
                    "youtube_operation_failed",
                );
                return Err(e);
            }
        }
    }
}

const YOUTUBE_CREDENTIALS_LEGACY_KEY: &str = "youtube_credentials";
const YOUTUBE_CREDENTIALS_KEY_PREFIX: &str = "youtube_credentials";
const YOUTUBE_UPLOAD_HISTORY_KEY_PREFIX: &str = "youtube_upload_history";
const YOUTUBE_UPLOAD_QUEUE_KEY_PREFIX: &str = "youtube_upload_queue";
const YOUTUBE_QUOTA_KEY_PREFIX: &str = "youtube_quota";

fn is_missing_setting(error: &StorageError) -> bool {
    matches!(error, StorageError::Io(io_error) if io_error.kind() == std::io::ErrorKind::NotFound)
}

fn encode_user_id_for_setting_key(user_id: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut encoded = String::with_capacity(user_id.len() * 2);
    for byte in user_id.as_bytes() {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }

    encoded
}

fn scoped_youtube_setting_key(prefix: &str, user_id: &str) -> String {
    format!("{}_{}", prefix, encode_user_id_for_setting_key(user_id))
}

fn youtube_credentials_key(user_id: &str) -> String {
    scoped_youtube_setting_key(YOUTUBE_CREDENTIALS_KEY_PREFIX, user_id)
}

/// OS keyring "service" name credentials are stored under.
const KEYRING_SERVICE: &str = "LoLShorts";

/// YouTube `refresh_token`/`access_token` are long-lived secrets. Prefer the OS
/// keyring (Windows Credential Manager / macOS Keychain via the `keyring` crate)
/// over the plaintext SQLite `settings` table. SQLite remains a fallback for
/// environments where no OS keyring backend is available (e.g. some headless
/// CI/test environments), and as a one-time migration source for credentials
/// saved before this change.
fn keyring_entry_for_credentials(credentials_key: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, credentials_key)
}

/// Store `creds_json` in the OS keyring under `credentials_key`.
/// Returns `true` if the keyring write succeeded, `false` if the OS keyring is
/// unavailable/failed and the caller should fall back to local storage.
fn try_store_credentials_in_keyring(credentials_key: &str, creds_json: &str) -> bool {
    match keyring_entry_for_credentials(credentials_key) {
        Ok(entry) => match entry.set_password(creds_json) {
            Ok(()) => true,
            Err(e) => {
                warn!(
                    "Failed to store YouTube credentials in OS keyring, falling back to local storage: {}",
                    e
                );
                false
            }
        },
        Err(e) => {
            warn!(
                "OS keyring unavailable, falling back to local storage for YouTube credentials: {}",
                e
            );
            false
        }
    }
}

/// Read credentials JSON from the OS keyring, if present.
/// Returns `None` both when there is no entry and when the keyring backend
/// itself is unavailable (caller should fall back to local storage).
fn try_load_credentials_from_keyring(credentials_key: &str) -> Option<String> {
    match keyring_entry_for_credentials(credentials_key) {
        Ok(entry) => match entry.get_password() {
            Ok(json) => Some(json),
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                warn!("Failed to read YouTube credentials from OS keyring: {}", e);
                None
            }
        },
        Err(e) => {
            warn!(
                "OS keyring unavailable, cannot read YouTube credentials from it: {}",
                e
            );
            None
        }
    }
}

/// Best-effort deletion of any OS keyring entry for `credentials_key`.
fn delete_credentials_from_keyring(credentials_key: &str) {
    if let Ok(entry) = keyring_entry_for_credentials(credentials_key) {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => warn!(
                "Failed to delete YouTube credentials from OS keyring: {}",
                e
            ),
        }
    }
}

fn youtube_upload_history_key(user_id: &str) -> String {
    scoped_youtube_setting_key(YOUTUBE_UPLOAD_HISTORY_KEY_PREFIX, user_id)
}

fn youtube_upload_queue_key(user_id: &str) -> String {
    scoped_youtube_setting_key(YOUTUBE_UPLOAD_QUEUE_KEY_PREFIX, user_id)
}

fn youtube_quota_key(user_id: &str, date: &str) -> String {
    format!(
        "{}_{}_{}",
        YOUTUBE_QUOTA_KEY_PREFIX,
        encode_user_id_for_setting_key(user_id),
        date
    )
}

/// Verify the current app user has PRO access to invoke `cmd_name`.
///
/// Routes through [`command_policy::require_command_access`] — the single source
/// of truth for command gating — instead of calling `require_tier` directly, so
/// this module cannot drift from its `COMMAND_POLICIES` declaration.
///
/// The YouTube surface is no longer uniformly paid: signing in and uploading a
/// video are `AuthRequired` (getting your own clip onto YouTube is part of the
/// free product), while scheduled/batch upload stays `ProRequired`. The tier for
/// each command therefore comes from the table, not from this function.
///
/// The `debug_assert!` still catches the one case the table cannot express: a
/// call site passing a `cmd_name` that is missing from (or misspelled in)
/// `COMMAND_POLICIES`. An unregistered command falls back to `AuthRequired` in
/// `require_command_access`, which for a paid command would be a silent
/// downgrade — see the fail-closed fallback below.
fn require_youtube_access(auth: &Arc<AuthManager>, cmd_name: &str) -> AppResult<User> {
    debug_assert!(
        command_policy::command_access(cmd_name).is_some(),
        "YouTube command '{}' must be registered in COMMAND_POLICIES",
        cmd_name
    );

    require_youtube_access_fail_closed(auth, cmd_name)
}

/// Fail-closed core of [`require_youtube_access`], split out so it can be
/// exercised directly in tests without tripping the `debug_assert_eq!` above
/// (which is compiled out of release builds — the exact case this fallback
/// guards against).
///
/// `command_policy::require_command_access` treats a policy-table miss as
/// `CommandAccess::AuthRequired` (`unwrap_or(CommandAccess::AuthRequired)`).
/// For a command that is supposed to be paid, trusting that default on a table
/// miss would be a silent downgrade, and a miss is exactly the situation where
/// we cannot tell which kind it was. So a miss enforces the PRO tier directly
/// rather than the table's permissive default — wrongly asking a free user to
/// upgrade is recoverable, silently giving away a paid feature is not.
fn require_youtube_access_fail_closed(auth: &Arc<AuthManager>, cmd_name: &str) -> AppResult<User> {
    if command_policy::command_access(cmd_name).is_none() {
        warn!(
            "YouTube command '{}' missing from COMMAND_POLICIES; falling back to a hard PRO-tier check",
            cmd_name
        );
        return require_tier(auth, SubscriptionTier::Pro)
            .map_err(|e| AppError::Auth(e.to_string()));
    }

    command_policy::require_command_access(auth, cmd_name)
        .map_err(|e| AppError::Auth(e.to_string()))?
        .ok_or_else(|| AppError::Auth("YouTube commands require authentication".to_string()))
}

fn require_same_youtube_pro_user(
    auth: &Arc<AuthManager>,
    cmd_name: &str,
    expected_user_id: &str,
) -> AppResult<User> {
    let user = require_youtube_access(auth, cmd_name)?;

    if user.id != expected_user_id {
        return Err(AppError::Auth(
            "Authenticated user changed during YouTube authentication".to_string(),
        ));
    }

    Ok(user)
}

async fn require_active_pending_youtube_oauth_user(
    auth: &Arc<AuthManager>,
    cmd_name: &str,
    manager: &YouTubeManager,
    expected_user_id: &str,
) -> AppResult<User> {
    let user = require_same_youtube_pro_user(auth, cmd_name, expected_user_id)?;
    manager
        .require_pending_oauth_owner_for_user(expected_user_id)
        .await?;
    Ok(user)
}

async fn load_upload_queue(storage: &Storage, queue_key: &str) -> AppResult<Vec<ScheduledUpload>> {
    let queue_json = match storage.get_setting(queue_key).await {
        Ok(json) => json,
        Err(error) if is_missing_setting(&error) => return Ok(Vec::new()),
        Err(error) => return Err(AppError::Database(error.to_string())),
    };

    serde_json::from_str(&queue_json)
        .map_err(|error| AppError::Internal(format!("Invalid upload queue data: {}", error)))
}

async fn save_upload_queue(
    storage: &Storage,
    queue_key: &str,
    queue: &[ScheduledUpload],
) -> AppResult<()> {
    let queue_json = serde_json::to_string(queue).map_err(|e| AppError::Internal(e.to_string()))?;
    storage
        .set_setting(queue_key, &queue_json)
        .await
        .map_err(|e| AppError::Database(e.to_string()))
}

fn parse_privacy_status(privacy_status: &str) -> AppResult<PrivacyStatus> {
    match privacy_status.to_lowercase().as_str() {
        "public" => Ok(PrivacyStatus::Public),
        "unlisted" => Ok(PrivacyStatus::Unlisted),
        "private" => Ok(PrivacyStatus::Private),
        _ => Err(AppError::Validation(
            "Invalid privacy status. Must be: public, unlisted, or private".to_string(),
        )),
    }
}

fn scheduled_upload_due_at(upload: &ScheduledUpload) -> AppResult<Option<chrono::DateTime<Utc>>> {
    upload
        .schedule
        .scheduled_at
        .as_deref()
        .map(|scheduled_at| {
            chrono::DateTime::parse_from_rfc3339(scheduled_at)
                .map(|time| time.with_timezone(&Utc))
                .map_err(|error| {
                    AppError::Validation(format!(
                        "Invalid scheduled_at for upload {}: {}",
                        upload.id, error
                    ))
                })
        })
        .transpose()
}

fn is_scheduled_upload_due(
    upload: &ScheduledUpload,
    now: chrono::DateTime<Utc>,
) -> AppResult<bool> {
    if upload.status != ScheduledUploadStatus::Pending || upload.status.is_terminal() {
        return Ok(false);
    }

    Ok(match scheduled_upload_due_at(upload)? {
        Some(scheduled_at) => scheduled_at <= now,
        None => true,
    })
}

fn mark_due_scheduled_uploads_running(
    queue: &mut [ScheduledUpload],
    now: chrono::DateTime<Utc>,
) -> (Vec<ScheduledUpload>, Vec<ScheduledUploadEvent>) {
    let mut due_uploads = Vec::new();
    let mut failed_events = Vec::new();

    for upload in queue.iter_mut() {
        if upload.status != ScheduledUploadStatus::Pending || upload.status.is_terminal() {
            continue;
        }

        match is_scheduled_upload_due(upload, now) {
            Ok(true) => {
                upload.status = ScheduledUploadStatus::Running;
                upload.error_message = None;
                upload.attempts = upload.attempts.saturating_add(1);
                upload.updated_at = Some(now.timestamp());
                due_uploads.push(upload.clone());
            }
            Ok(false) => {}
            Err(error) => {
                upload.status = ScheduledUploadStatus::Failed;
                upload.error_message = Some(error.to_string());
                upload.updated_at = Some(now.timestamp());
                failed_events.push(ScheduledUploadEvent {
                    upload_id: upload.id.clone(),
                    status: ScheduledUploadStatus::Failed,
                    video_id: None,
                    title: upload.title.clone(),
                    attempts: upload.attempts,
                    error_message: upload.error_message.clone(),
                });
            }
        }
    }

    (due_uploads, failed_events)
}

async fn load_youtube_credentials_for_user(
    manager: &YouTubeManager,
    user_id: &str,
) -> AppResult<Option<YouTubeCredentials>> {
    manager
        .load_credentials_for_user(user_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))
}

async fn read_stored_youtube_credentials_for_user(
    manager: &YouTubeManager,
    user_id: &str,
) -> AppResult<Option<YouTubeCredentials>> {
    let credentials_key = youtube_credentials_key(user_id);

    if let Some(creds_json) = try_load_credentials_from_keyring(&credentials_key) {
        return serde_json::from_str::<YouTubeCredentials>(&creds_json)
            .map(Some)
            .map_err(|error| {
                AppError::Internal(format!("Invalid YouTube credentials: {}", error))
            });
    }

    let creds_json = match manager.storage.get_setting(&credentials_key).await {
        Ok(json) => json,
        Err(error) if is_missing_setting(&error) => return Ok(None),
        Err(error) => return Err(AppError::Database(error.to_string())),
    };

    serde_json::from_str::<YouTubeCredentials>(&creds_json)
        .map(Some)
        .map_err(|error| AppError::Internal(format!("Invalid YouTube credentials: {}", error)))
}

async fn require_youtube_credentials_for_user(
    manager: &YouTubeManager,
    user_id: &str,
) -> AppResult<YouTubeCredentials> {
    load_youtube_credentials_for_user(manager, user_id)
        .await?
        .ok_or_else(|| AppError::Auth("YouTube account not authenticated".to_string()))
}

async fn record_youtube_quota_usage(
    storage: &Storage,
    user_id: &str,
    quota_units: u64,
) -> AppResult<()> {
    let today_pst = Utc::now()
        .with_timezone(&Los_Angeles)
        .format("%Y-%m-%d")
        .to_string();
    let quota_key = youtube_quota_key(user_id, &today_pst);
    let current: u64 = storage
        .get_setting(&quota_key)
        .await
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    storage
        .set_setting(&quota_key, &(current + quota_units).to_string())
        .await
        .map_err(|e| AppError::Database(e.to_string()))
}

async fn record_youtube_upload_history(
    storage: &Storage,
    user_id: &str,
    video: &YouTubeVideo,
) -> AppResult<()> {
    let history_key = youtube_upload_history_key(user_id);
    let entry = UploadHistoryEntry {
        video_id: video.id.clone(),
        title: video.title.clone(),
        uploaded_at: chrono::Utc::now().timestamp(),
        privacy_status: video.privacy_status.clone(),
        thumbnail_url: video.thumbnail_url.clone(),
        view_count: video.view_count,
    };

    let mut history: Vec<UploadHistoryEntry> = storage
        .get_setting(&history_key)
        .await
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();

    history.insert(0, entry);
    history.truncate(100);

    let history_json =
        serde_json::to_string(&history).map_err(|e| AppError::Internal(e.to_string()))?;
    storage
        .set_setting(&history_key, &history_json)
        .await
        .map_err(|e| AppError::Database(e.to_string()))
}

async fn upload_scheduled_video_for_user(
    manager: &YouTubeManager,
    user_id: &str,
    scheduled_upload: &ScheduledUpload,
) -> AppResult<YouTubeVideo> {
    security::validate_video_input_path(&scheduled_upload.video_path).map_err(|e| {
        error!("Invalid video path: {}", e);
        AppError::Validation(format!("Invalid video path: {}", e))
    })?;

    let video_path = PathBuf::from(&scheduled_upload.video_path);
    if !video_path.exists() {
        return Err(AppError::NotFound("Video file not found".to_string()));
    }

    let thumbnail_path = if let Some(ref thumb) = scheduled_upload.thumbnail_path {
        security::validate_thumbnail_path(thumb).map_err(|e| {
            error!("Invalid thumbnail path: {}", e);
            AppError::Validation(format!("Invalid thumbnail path: {}", e))
        })?;

        let thumb_path = PathBuf::from(thumb);
        if !thumb_path.exists() {
            return Err(AppError::NotFound("Thumbnail file not found".to_string()));
        }
        Some(thumb_path)
    } else {
        None
    };

    let privacy = parse_privacy_status(&scheduled_upload.privacy_status)?;
    let mut metadata = VideoMetadata {
        title: scheduled_upload.title.clone(),
        description: scheduled_upload.description.clone(),
        tags: scheduled_upload.tags.clone(),
        category_id: "20".to_string(),
        privacy_status: privacy,
        made_for_kids: false,
    };
    metadata.ensure_shorts_tag();

    let upload_client = {
        let credentials = require_youtube_credentials_for_user(manager, user_id).await?;
        let isolated_oauth_client = Arc::new(
            manager
                .oauth_client
                .clone_with_credentials(credentials)
                .await,
        );
        Arc::new(
            manager
                .upload_client
                .with_oauth_client(isolated_oauth_client),
        )
    };

    let metadata_clone = metadata.clone();
    let video_path_clone = video_path.clone();
    let thumbnail_path_clone = thumbnail_path.clone();
    retry_with_backoff(3, "scheduled_youtube_upload", || {
        let client = Arc::clone(&upload_client);
        let path = video_path_clone.clone();
        let meta = metadata_clone.clone();
        let thumb = thumbnail_path_clone.clone();
        async move {
            match client
                .upload_video_resumable(&path, meta.clone(), thumb.as_deref())
                .await
            {
                Ok(v) => Ok(v),
                Err(e) => {
                    warn!(
                        "Scheduled resumable upload failed, falling back to multipart: {}",
                        e
                    );
                    client
                        .upload_video(&path, meta, thumb.as_deref())
                        .await
                        .map_err(|e2| {
                            error!("Scheduled video upload failed: {}", e2);
                            AppError::Network(format!("Upload failed: {}", e2))
                        })
                }
            }
        }
    })
    .await
}

/// YouTube manager state
#[derive(Clone)]
pub struct YouTubeManager {
    pub oauth_client: Arc<YouTubeOAuthClient>,
    pub upload_client: Arc<YouTubeUploadClient>,
    pub storage: Arc<Storage>,
    /// The configured OAuth2 redirect URI (SSOT for where the local callback
    /// server must listen — see `CallbackServer::from_redirect_uri`).
    pub redirect_uri: String,
    pending_oauth_owner: Arc<RwLock<Option<String>>>,
    credential_operation_lock: Arc<Mutex<()>>,
}

impl YouTubeManager {
    pub fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        storage: Arc<Storage>,
    ) -> anyhow::Result<Self> {
        let oauth_client = Arc::new(YouTubeOAuthClient::new(
            client_id,
            client_secret,
            redirect_uri.clone(),
        )?);
        let upload_client = Arc::new(YouTubeUploadClient::new(Arc::clone(&oauth_client))?);

        Ok(Self {
            oauth_client,
            upload_client,
            storage,
            redirect_uri,
            pending_oauth_owner: Arc::new(RwLock::new(None)),
            credential_operation_lock: Arc::new(Mutex::new(())),
        })
    }

    async fn set_pending_oauth_owner_for_user(&self, owner_user_id: &str) {
        let mut pending_owner = self.pending_oauth_owner.write().await;
        *pending_owner = Some(owner_user_id.to_string());
    }

    async fn require_pending_oauth_owner_for_user(&self, current_user_id: &str) -> AppResult<()> {
        let pending_owner = self.pending_oauth_owner.read().await;

        match pending_owner.as_deref() {
            Some(owner_user_id) if owner_user_id == current_user_id => Ok(()),
            Some(_) => Err(AppError::Auth(
                "Authenticated user changed during YouTube authentication".to_string(),
            )),
            None => Err(AppError::Auth(
                "No pending YouTube authentication flow for current user".to_string(),
            )),
        }
    }

    pub async fn clear_pending_oauth_owner(&self) {
        let mut pending_owner = self.pending_oauth_owner.write().await;
        *pending_owner = None;
    }

    pub async fn lock_credential_operations(&self) -> MutexGuard<'_, ()> {
        self.credential_operation_lock.lock().await
    }

    pub async fn clear_app_auth_oauth_state(&self) {
        self.oauth_client.clear_credentials().await;
        self.clear_pending_oauth_owner().await;
    }

    async fn clear_pending_oauth_owner_if_matches(&self, owner_user_id: &str) {
        let mut pending_owner = self.pending_oauth_owner.write().await;
        if pending_owner.as_deref() == Some(owner_user_id) {
            *pending_owner = None;
        }
    }

    #[cfg(test)]
    async fn pending_oauth_owner(&self) -> Option<String> {
        self.pending_oauth_owner.read().await.clone()
    }

    async fn clear_legacy_credentials(&self) -> anyhow::Result<()> {
        match self
            .storage
            .get_setting(YOUTUBE_CREDENTIALS_LEGACY_KEY)
            .await
        {
            Ok(_) => {
                self.storage
                    .remove_setting(YOUTUBE_CREDENTIALS_LEGACY_KEY)
                    .await?;
                self.oauth_client.clear_credentials().await;
                warn!("Cleared unowned legacy YouTube credentials from storage");
            }
            Err(error) if is_missing_setting(&error) => {}
            Err(error) => return Err(error.into()),
        }

        Ok(())
    }

    /// Startup-safe credential loading.
    ///
    /// Credentials are user-owned, so startup cannot load them until an app user
    /// is authenticated. Any legacy global value is cleared instead of exposed.
    pub async fn load_credentials(&self) -> anyhow::Result<()> {
        self.clear_legacy_credentials().await?;
        self.oauth_client.clear_credentials().await;
        Ok(())
    }

    /// Load stored credentials for the authenticated app user.
    ///
    /// Prefers the OS keyring (encrypted at rest); falls back to the local
    /// SQLite `settings` table when the keyring is unavailable. Credentials
    /// found only in SQLite (pre-migration, or keyring-unavailable saves) are
    /// opportunistically migrated into the keyring on successful load.
    pub async fn load_credentials_for_user(
        &self,
        owner_user_id: &str,
    ) -> anyhow::Result<Option<YouTubeCredentials>> {
        self.clear_legacy_credentials().await?;

        let credentials_key = youtube_credentials_key(owner_user_id);

        if let Some(creds_json) = try_load_credentials_from_keyring(&credentials_key) {
            return match serde_json::from_str::<YouTubeCredentials>(&creds_json) {
                Ok(credentials) => {
                    self.oauth_client.set_credentials(credentials.clone()).await;
                    info!("YouTube credentials loaded from OS keyring for authenticated app user");
                    Ok(Some(credentials))
                }
                Err(error) => {
                    self.oauth_client.clear_credentials().await;
                    delete_credentials_from_keyring(&credentials_key);
                    warn!("Cleared invalid YouTube credentials from OS keyring for authenticated app user");
                    Err(error.into())
                }
            };
        }

        let creds_json = match self.storage.get_setting(&credentials_key).await {
            Ok(json) => json,
            Err(error) if is_missing_setting(&error) => {
                self.oauth_client.clear_credentials().await;
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };

        match serde_json::from_str::<YouTubeCredentials>(&creds_json) {
            Ok(credentials) => {
                self.oauth_client.set_credentials(credentials.clone()).await;
                info!("YouTube credentials loaded for authenticated app user");

                // One-time migration: move plaintext credentials into the OS keyring.
                if try_store_credentials_in_keyring(&credentials_key, &creds_json) {
                    if let Err(e) = self.storage.remove_setting(&credentials_key).await {
                        warn!(
                            "Failed to remove migrated plaintext YouTube credentials: {}",
                            e
                        );
                    } else {
                        info!("Migrated YouTube credentials from local storage to OS keyring");
                    }
                }

                Ok(Some(credentials))
            }
            Err(error) => {
                self.oauth_client.clear_credentials().await;
                self.storage.remove_setting(&credentials_key).await?;
                warn!("Cleared invalid YouTube credentials for authenticated app user");
                Err(error.into())
            }
        }
    }

    /// Save credentials for the authenticated app user.
    ///
    /// Prefers the OS keyring; falls back to the local SQLite `settings`
    /// table (with a warning) when the keyring is unavailable.
    pub async fn save_credentials_for_user(
        &self,
        owner_user_id: &str,
        credentials: &YouTubeCredentials,
    ) -> anyhow::Result<()> {
        self.clear_legacy_credentials().await?;

        let creds_json = serde_json::to_string(credentials)?;
        let credentials_key = youtube_credentials_key(owner_user_id);

        if try_store_credentials_in_keyring(&credentials_key, &creds_json) {
            // Stored securely; remove any stale plaintext copy from a previous fallback save.
            if let Err(e) = self.storage.remove_setting(&credentials_key).await {
                warn!(
                    "Failed to remove stale plaintext YouTube credentials after keyring save: {}",
                    e
                );
            }
            info!("YouTube credentials saved to OS keyring for authenticated app user");
        } else {
            self.storage
                .set_setting(&credentials_key, &creds_json)
                .await?;
            warn!(
                "YouTube credentials saved to local storage (OS keyring unavailable) for authenticated app user"
            );
        }

        Ok(())
    }

    /// Clear credentials for the authenticated app user (both OS keyring and local storage).
    pub async fn clear_credentials_for_user(&self, owner_user_id: &str) -> anyhow::Result<()> {
        self.oauth_client.clear_credentials().await;
        let credentials_key = youtube_credentials_key(owner_user_id);
        delete_credentials_from_keyring(&credentials_key);
        self.storage.remove_setting(&credentials_key).await?;
        self.clear_legacy_credentials().await?;
        Ok(())
    }
}

/// Start YouTube OAuth2 authentication flow (manual completion variant).
///
/// NOT used by the frontend, which exclusively calls
/// [`youtube_start_auth_with_server`] for automatic callback handling. This
/// command exists for headless/test flows that will call
/// [`youtube_complete_auth`] manually with a code/state pair obtained out of
/// band (e.g. by copy-pasting the redirect URL).
///
/// Returns the authorization URL that should be opened in a browser
#[tauri::command]
pub async fn youtube_start_auth(state: State<'_, AppState>) -> AppResult<String> {
    let user = require_youtube_access(&state.auth, "youtube_start_auth")?;
    {
        let _credential_guard = state.youtube_manager.credential_operation_lock.lock().await;
        state
            .youtube_manager
            .load_credentials_for_user(&user.id)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
    }

    info!("Starting YouTube OAuth2 flow");

    let auth_url = state
        .youtube_manager
        .oauth_client
        .generate_auth_url()
        .await
        .map_err(|e| {
            error!("Failed to generate auth URL: {}", e);
            AppError::Auth(format!("Failed to start authentication: {}", e))
        })?;

    state
        .youtube_manager
        .set_pending_oauth_owner_for_user(&user.id)
        .await;

    Ok(auth_url)
}

/// Start YouTube OAuth2 authentication with automatic callback handling
///
/// This command:
/// 1. Starts a local callback server whose port/path are derived from `YOUTUBE_REDIRECT_URI`
///    (see [`CallbackServer::from_redirect_uri`] — single source of truth).
/// 2. Generates the OAuth authorization URL
/// 3. Returns the URL for the frontend to open in a browser
/// 4. Waits for the callback in the background
/// 5. Automatically completes authentication when callback is received
///
/// Emits `youtube-auth-completed` on success or `youtube-auth-failed`
/// (payload: `{ "error": string }`) on failure/timeout so the frontend does
/// not have to guess when the background flow finishes. If the user declines
/// consent, Google redirects with `?error=access_denied` (no `code`/`state`);
/// `CallbackServer` resolves that immediately (see `callback_server.rs`), so
/// `youtube-auth-failed` fires right away here too instead of only after the
/// 120-second callback timeout.
///
/// Returns the authorization URL that should be opened in a browser
#[tauri::command]
pub async fn youtube_start_auth_with_server(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> AppResult<String> {
    let user = require_youtube_access(&state.auth, "youtube_start_auth_with_server")?;
    {
        let _credential_guard = state.youtube_manager.credential_operation_lock.lock().await;
        state
            .youtube_manager
            .load_credentials_for_user(&user.id)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
    }

    info!("Starting YouTube OAuth2 flow with automatic callback handling");

    // Fail fast with a clear error if YOUTUBE_REDIRECT_URI is misconfigured, rather than
    // silently timing out in the background task 2 minutes later.
    let callback_server = CallbackServer::from_redirect_uri(&state.youtube_manager.redirect_uri)
        .map_err(|e| {
            error!("Invalid YOUTUBE_REDIRECT_URI configuration: {}", e);
            AppError::Internal(format!("Invalid OAuth redirect URI configuration: {}", e))
        })?;

    // Generate auth URL first
    let auth_url = state
        .youtube_manager
        .oauth_client
        .generate_auth_url()
        .await
        .map_err(|e| {
            error!("Failed to generate auth URL: {}", e);
            AppError::Auth(format!("Failed to start authentication: {}", e))
        })?;

    state
        .youtube_manager
        .set_pending_oauth_owner_for_user(&user.id)
        .await;

    // Start callback server in background
    let youtube_clone = Arc::clone(&state.youtube_manager);
    let auth_clone = Arc::clone(&state.auth);
    let starting_user_id = user.id.clone();
    tokio::spawn(async move {
        use tauri::Emitter;

        let emit_failed = |error: String| {
            if let Err(e) =
                app_handle.emit("youtube-auth-failed", serde_json::json!({ "error": error }))
            {
                warn!("Failed to emit youtube-auth-failed event: {}", e);
            }
        };

        match callback_server.start_and_wait().await {
            Ok(callback) => {
                info!("Received OAuth callback, completing authentication automatically");

                if let Err(e) = require_active_pending_youtube_oauth_user(
                    &auth_clone,
                    "youtube_start_auth_with_server",
                    &youtube_clone,
                    &starting_user_id,
                )
                .await
                {
                    warn!(
                        "Skipping YouTube OAuth auto-complete because app auth changed: {}",
                        e
                    );
                    youtube_clone.oauth_client.clear_credentials().await;
                    youtube_clone
                        .clear_pending_oauth_owner_if_matches(&starting_user_id)
                        .await;
                    emit_failed(e.to_string());
                    return;
                }

                // Automatically complete authentication
                let _credential_guard = youtube_clone.lock_credential_operations().await;
                if let Err(e) = require_active_pending_youtube_oauth_user(
                    &auth_clone,
                    "youtube_start_auth_with_server",
                    &youtube_clone,
                    &starting_user_id,
                )
                .await
                {
                    warn!(
                        "Skipping YouTube OAuth exchange because app auth changed before token exchange: {}",
                        e
                    );
                    youtube_clone.clear_app_auth_oauth_state().await;
                    youtube_clone
                        .clear_pending_oauth_owner_if_matches(&starting_user_id)
                        .await;
                    emit_failed(e.to_string());
                    return;
                }
                match youtube_clone
                    .oauth_client
                    .exchange_code(callback.code, callback.state)
                    .await
                {
                    Ok(credentials) => {
                        if let Err(e) = require_active_pending_youtube_oauth_user(
                            &auth_clone,
                            "youtube_start_auth_with_server",
                            &youtube_clone,
                            &starting_user_id,
                        )
                        .await
                        {
                            warn!(
                                "Skipping YouTube OAuth credential save because app auth changed after token exchange: {}",
                                e
                            );
                            youtube_clone.oauth_client.clear_credentials().await;
                            youtube_clone
                                .clear_pending_oauth_owner_if_matches(&starting_user_id)
                                .await;
                            emit_failed(e.to_string());
                            return;
                        }

                        // Save credentials
                        if let Err(e) = youtube_clone
                            .save_credentials_for_user(&starting_user_id, &credentials)
                            .await
                        {
                            youtube_clone
                                .clear_pending_oauth_owner_if_matches(&starting_user_id)
                                .await;
                            error!("Failed to save credentials after auto-complete: {}", e);
                            emit_failed(e.to_string());
                        } else {
                            youtube_clone
                                .clear_pending_oauth_owner_if_matches(&starting_user_id)
                                .await;
                            info!("YouTube authentication auto-completed successfully");
                            if let Err(e) = app_handle.emit("youtube-auth-completed", ()) {
                                warn!("Failed to emit youtube-auth-completed event: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        youtube_clone
                            .clear_pending_oauth_owner_if_matches(&starting_user_id)
                            .await;
                        error!(
                            "Failed to exchange authorization code in auto-complete: {}",
                            e
                        );
                        emit_failed(e.to_string());
                    }
                }
            }
            Err(e) => {
                error!("OAuth callback server error: {}", e);
                emit_failed(e.to_string());
            }
        }
    });

    info!("Callback server started, returning auth URL");
    Ok(auth_url)
}

/// Complete YouTube OAuth2 authentication (manual completion variant).
///
/// NOT used by the frontend — pairs with [`youtube_start_auth`] for
/// headless/test flows only. The frontend's automatic flow
/// ([`youtube_start_auth_with_server`]) completes authentication itself in
/// its background callback task and never calls this command.
///
/// # Arguments
/// * `code` - Authorization code from OAuth2 callback
/// * `state` - CSRF state token from OAuth2 callback
#[tauri::command]
pub async fn youtube_complete_auth(
    app_state: State<'_, AppState>,
    code: String,
    state: String,
) -> AppResult<()> {
    let user = require_youtube_access(&app_state.auth, "youtube_complete_auth")?;

    info!("Completing YouTube OAuth2 flow");

    // Validate inputs
    if code.is_empty() || state.is_empty() {
        return Err(AppError::Validation(
            "Invalid authorization code or state".to_string(),
        ));
    }

    require_active_pending_youtube_oauth_user(
        &app_state.auth,
        "youtube_complete_auth",
        &app_state.youtube_manager,
        &user.id,
    )
    .await?;

    let _credential_guard = app_state.youtube_manager.lock_credential_operations().await;

    require_active_pending_youtube_oauth_user(
        &app_state.auth,
        "youtube_complete_auth",
        &app_state.youtube_manager,
        &user.id,
    )
    .await?;

    // Exchange code for credentials
    let credentials = match app_state
        .youtube_manager
        .oauth_client
        .exchange_code(code, state)
        .await
    {
        Ok(credentials) => credentials,
        Err(e) => {
            app_state
                .youtube_manager
                .clear_pending_oauth_owner_if_matches(&user.id)
                .await;
            error!("Failed to exchange authorization code: {}", e);
            return Err(AppError::Auth(format!("Authentication failed: {}", e)));
        }
    };

    if let Err(e) = require_active_pending_youtube_oauth_user(
        &app_state.auth,
        "youtube_complete_auth",
        &app_state.youtube_manager,
        &user.id,
    )
    .await
    {
        app_state
            .youtube_manager
            .oauth_client
            .clear_credentials()
            .await;
        app_state
            .youtube_manager
            .clear_pending_oauth_owner_if_matches(&user.id)
            .await;
        warn!(
            "Rejecting YouTube OAuth credential save because app auth changed after token exchange: {}",
            e
        );
        return Err(e);
    }

    // Save credentials
    if let Err(e) = app_state
        .youtube_manager
        .save_credentials_for_user(&user.id, &credentials)
        .await
    {
        app_state
            .youtube_manager
            .clear_pending_oauth_owner_if_matches(&user.id)
            .await;
        error!("Failed to save credentials: {}", e);
        return Err(AppError::Auth("Failed to save credentials".to_string()));
    }

    app_state
        .youtube_manager
        .clear_pending_oauth_owner_if_matches(&user.id)
        .await;

    info!("YouTube authentication completed successfully");
    Ok(())
}

/// Check YouTube authentication status
#[tauri::command]
pub async fn youtube_get_auth_status(state: State<'_, AppState>) -> AppResult<AuthStatus> {
    let user = require_youtube_access(&state.auth, "youtube_get_auth_status")?;

    let _credential_guard = state.youtube_manager.credential_operation_lock.lock().await;
    let credentials = load_youtube_credentials_for_user(&state.youtube_manager, &user.id).await?;

    Ok(AuthStatus {
        authenticated: credentials.is_some(),
        expires_at: credentials.as_ref().and_then(|c| c.expires_at),
        has_refresh_token: credentials
            .as_ref()
            .and_then(|c| c.refresh_token.as_ref())
            .is_some(),
    })
}

/// Upload video to YouTube
///
/// # Arguments
/// * `video_path` - Absolute path to video file
/// * `title` - Video title
/// * `description` - Video description
/// * `tags` - Array of video tags
/// * `privacy_status` - Privacy status (public, unlisted, private)
/// * `thumbnail_path` - Optional path to custom thumbnail
#[tauri::command]
pub async fn youtube_upload_video(
    state: State<'_, AppState>,
    video_path: String,
    title: String,
    description: String,
    tags: Vec<String>,
    privacy_status: String,
    thumbnail_path: Option<String>,
) -> AppResult<YouTubeVideo> {
    let user = require_youtube_access(&state.auth, "youtube_upload_video")?;
    let upload_client = {
        let _credential_guard = state.youtube_manager.credential_operation_lock.lock().await;
        let credentials =
            require_youtube_credentials_for_user(&state.youtube_manager, &user.id).await?;
        let isolated_oauth_client = Arc::new(
            state
                .youtube_manager
                .oauth_client
                .clone_with_credentials(credentials)
                .await,
        );

        Arc::new(
            state
                .youtube_manager
                .upload_client
                .with_oauth_client(isolated_oauth_client),
        )
    };

    info!("Starting YouTube video upload: {}", video_path);

    // Validate video path
    security::validate_video_input_path(&video_path).map_err(|e| {
        error!("Invalid video path: {}", e);
        AppError::Validation(format!("Invalid video path: {}", e))
    })?;

    let video_path = PathBuf::from(&video_path);
    if !video_path.exists() {
        return Err(AppError::NotFound("Video file not found".to_string()));
    }

    // Validate thumbnail path if provided
    let thumbnail_path = if let Some(thumb) = thumbnail_path {
        security::validate_thumbnail_path(&thumb).map_err(|e| {
            error!("Invalid thumbnail path: {}", e);
            AppError::Validation(format!("Invalid thumbnail path: {}", e))
        })?;

        let thumb_path = PathBuf::from(&thumb);
        if !thumb_path.exists() {
            return Err(AppError::NotFound("Thumbnail file not found".to_string()));
        }
        Some(thumb_path)
    } else {
        None
    };

    // Parse privacy status
    let privacy = parse_privacy_status(&privacy_status)?;

    // Create metadata
    let mut metadata = VideoMetadata {
        title,
        description,
        tags,
        category_id: "20".to_string(), // Gaming category
        privacy_status: privacy,
        made_for_kids: false,
    };
    metadata.ensure_shorts_tag();

    // Upload video (resumable with multipart fallback), with exponential backoff retry
    let metadata_clone = metadata.clone();
    let video_path_clone = video_path.clone();
    let thumbnail_path_clone = thumbnail_path.clone();
    let video = retry_with_backoff(3, "youtube_upload_video", || {
        let client = Arc::clone(&upload_client);
        let path = video_path_clone.clone();
        let meta = metadata.clone();
        let thumb = thumbnail_path_clone.clone();
        async move {
            match client
                .upload_video_resumable(&path, meta.clone(), thumb.as_deref())
                .await
            {
                Ok(v) => Ok(v),
                Err(e) => {
                    warn!("Resumable upload failed, falling back to multipart: {}", e);
                    client
                        .upload_video(&path, meta, thumb.as_deref())
                        .await
                        .map_err(|e2| {
                            error!("Video upload failed: {}", e2);
                            AppError::Network(format!("Upload failed: {}", e2))
                        })
                }
            }
        }
    })
    .await?;
    // suppress unused variable warning for metadata_clone (kept for ABI compatibility)
    let _ = metadata_clone;

    // Track quota usage after successful upload (1600 units per video.insert)
    let _ = record_youtube_quota_usage(&state.youtube_manager.storage, &user.id, 1600).await;

    Ok(video)
}

/// Get current upload progress
#[tauri::command]
pub async fn youtube_get_upload_progress(
    state: State<'_, AppState>,
) -> AppResult<Option<UploadProgress>> {
    let user = require_youtube_access(&state.auth, "youtube_get_upload_progress")?;
    read_stored_youtube_credentials_for_user(&state.youtube_manager, &user.id)
        .await?
        .ok_or_else(|| AppError::Auth("YouTube account not authenticated".to_string()))?;

    Ok(state.youtube_manager.upload_client.get_progress().await)
}

/// Get video details from YouTube
#[tauri::command]
pub async fn youtube_get_video_details(
    state: State<'_, AppState>,
    video_id: String,
) -> AppResult<YouTubeVideo> {
    let user = require_youtube_access(&state.auth, "youtube_get_video_details")?;
    let _credential_guard = state.youtube_manager.credential_operation_lock.lock().await;
    require_youtube_credentials_for_user(&state.youtube_manager, &user.id).await?;

    // Validate video ID
    if video_id.is_empty() || video_id.len() > 50 {
        return Err(AppError::Validation("Invalid video ID".to_string()));
    }

    state
        .youtube_manager
        .upload_client
        .get_video_details(&video_id)
        .await
        .map_err(|e| {
            error!("Failed to get video details: {}", e);
            AppError::Network(format!("Failed to get video details: {}", e))
        })
}

/// Get upload history from storage
#[tauri::command]
pub async fn youtube_get_upload_history(
    state: State<'_, AppState>,
) -> AppResult<Vec<UploadHistoryEntry>> {
    let user = require_youtube_access(&state.auth, "youtube_get_upload_history")?;
    let history_key = youtube_upload_history_key(&user.id);

    Ok(state
        .youtube_manager
        .storage
        .get_setting(&history_key)
        .await
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

/// Add upload to history
#[tauri::command]
pub async fn youtube_add_to_history(
    state: State<'_, AppState>,
    video: YouTubeVideo,
) -> AppResult<()> {
    let user = require_youtube_access(&state.auth, "youtube_add_to_history")?;
    let history_key = youtube_upload_history_key(&user.id);

    let entry = UploadHistoryEntry {
        video_id: video.id,
        title: video.title,
        uploaded_at: chrono::Utc::now().timestamp(),
        privacy_status: video.privacy_status,
        thumbnail_url: video.thumbnail_url,
        view_count: video.view_count,
    };

    // Load existing history
    let mut history: Vec<UploadHistoryEntry> = state
        .youtube_manager
        .storage
        .get_setting(&history_key)
        .await
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();

    // Add new entry
    history.insert(0, entry);

    // Keep only last 100 entries
    history.truncate(100);

    // Save updated history
    let history_json =
        serde_json::to_string(&history).map_err(|e| AppError::Internal(e.to_string()))?;
    state
        .youtube_manager
        .storage
        .set_setting(&history_key, &history_json)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

/// Get YouTube API quota information (resets daily at midnight PST)
#[tauri::command]
pub async fn youtube_get_quota_info(state: State<'_, AppState>) -> AppResult<QuotaInfo> {
    let user = require_youtube_access(&state.auth, "youtube_get_quota_info")?;

    // Load used quota from storage (tracked locally, keyed by PST date)
    let today_pst = Utc::now()
        .with_timezone(&Los_Angeles)
        .format("%Y-%m-%d")
        .to_string();
    let quota_key = youtube_quota_key(&user.id, &today_pst);
    let used: u64 = state
        .youtube_manager
        .storage
        .get_setting(&quota_key)
        .await
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    Ok(QuotaInfo::new(used))
}

/// Log out from YouTube (clear credentials)
#[tauri::command]
pub async fn youtube_logout(state: State<'_, AppState>) -> AppResult<()> {
    let user = require_youtube_access(&state.auth, "youtube_logout")?;

    info!("Logging out from YouTube");

    let _credential_guard = state.youtube_manager.lock_credential_operations().await;
    state.youtube_manager.clear_pending_oauth_owner().await;
    state
        .youtube_manager
        .clear_credentials_for_user(&user.id)
        .await
        .map_err(|e| {
            error!("Failed to clear credentials: {}", e);
            AppError::Database("Failed to clear credentials".to_string())
        })?;

    info!("YouTube logout completed");
    Ok(())
}

/// Schedule a video upload for later processing
///
/// Saves upload metadata to a storage queue. Queued uploads are executed by
/// the backend's [`start_upload_scheduler`] background loop, which polls the
/// queue every 60 seconds and fires the real upload once an entry's
/// `scheduled_at` has passed — this requires the app to be running at (or
/// after) the scheduled time; it is not YouTube's native `publishAt`
/// scheduled-publish feature. See [`UploadSchedule`] for details.
#[tauri::command]
pub async fn youtube_schedule_upload(
    state: State<'_, AppState>,
    params: ScheduleUploadParams,
) -> AppResult<ScheduledUpload> {
    let user = require_youtube_access(&state.auth, "youtube_schedule_upload")?;
    let _credential_guard = state.youtube_manager.credential_operation_lock.lock().await;
    require_youtube_credentials_for_user(&state.youtube_manager, &user.id).await?;
    let queue_key = youtube_upload_queue_key(&user.id);

    let ScheduleUploadParams {
        video_path,
        title,
        description,
        tags,
        privacy_status,
        thumbnail_path,
        scheduled_at,
    } = params;

    info!("Scheduling YouTube upload: {}", title);

    // Validate video path
    security::validate_video_input_path(&video_path).map_err(|e| {
        error!("Invalid video path: {}", e);
        AppError::Validation(format!("Invalid video path: {}", e))
    })?;

    if !std::path::Path::new(&video_path).exists() {
        return Err(AppError::NotFound("Video file not found".to_string()));
    }

    // Validate thumbnail if provided
    if let Some(ref thumb) = thumbnail_path {
        security::validate_thumbnail_path(thumb).map_err(|e| {
            error!("Invalid thumbnail path: {}", e);
            AppError::Validation(format!("Invalid thumbnail path: {}", e))
        })?;
    }

    // Load existing queue
    let mut queue = load_upload_queue(&state.youtube_manager.storage, &queue_key).await?;

    let queue_position = queue.len() as u32;
    let id = format!("{}", chrono::Utc::now().timestamp_millis());

    let entry = ScheduledUpload {
        id: id.clone(),
        video_path,
        title,
        description,
        tags,
        privacy_status,
        thumbnail_path,
        schedule: UploadSchedule {
            scheduled_at,
            queue_position: Some(queue_position),
        },
        created_at: chrono::Utc::now().timestamp(),
        status: ScheduledUploadStatus::Pending,
        error_message: None,
        attempts: 0,
        completed_video_id: None,
        updated_at: None,
    };

    queue.push(entry.clone());

    save_upload_queue(&state.youtube_manager.storage, &queue_key, &queue).await?;

    info!("Upload scheduled with id: {}", id);
    Ok(entry)
}

/// Get all pending scheduled uploads
#[tauri::command]
pub async fn youtube_get_upload_queue(
    state: State<'_, AppState>,
) -> AppResult<Vec<ScheduledUpload>> {
    let user = require_youtube_access(&state.auth, "youtube_get_upload_queue")?;
    let _credential_guard = state.youtube_manager.credential_operation_lock.lock().await;
    require_youtube_credentials_for_user(&state.youtube_manager, &user.id).await?;
    let queue_key = youtube_upload_queue_key(&user.id);

    load_upload_queue(&state.youtube_manager.storage, &queue_key).await
}

/// Cancel a scheduled upload by ID
#[tauri::command]
pub async fn youtube_cancel_scheduled_upload(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    let user = require_youtube_access(&state.auth, "youtube_cancel_scheduled_upload")?;
    let _credential_guard = state.youtube_manager.credential_operation_lock.lock().await;
    require_youtube_credentials_for_user(&state.youtube_manager, &user.id).await?;
    let queue_key = youtube_upload_queue_key(&user.id);

    info!("Cancelling scheduled upload: {}", id);

    let mut queue = load_upload_queue(&state.youtube_manager.storage, &queue_key).await?;

    let now = chrono::Utc::now().timestamp();
    let mut found = false;
    for entry in queue.iter_mut() {
        if entry.id == id {
            found = true;
            if entry.status == ScheduledUploadStatus::Running {
                return Err(AppError::Validation(
                    "Cannot cancel a scheduled upload that is already running".to_string(),
                ));
            }
            if !entry.status.is_terminal() {
                entry.status = ScheduledUploadStatus::Cancelled;
                entry.error_message = None;
                entry.updated_at = Some(now);
            }
        }
    }

    if !found {
        return Err(AppError::NotFound(format!(
            "Scheduled upload not found: {}",
            id
        )));
    }

    // Re-assign queue positions for remaining non-terminal entries.
    let mut next_position = 0;
    for entry in queue.iter_mut() {
        if entry.status.is_terminal() {
            entry.schedule.queue_position = None;
        } else {
            entry.schedule.queue_position = Some(next_position);
            next_position += 1;
        }
    }

    save_upload_queue(&state.youtube_manager.storage, &queue_key, &queue).await?;

    info!("Scheduled upload cancelled: {}", id);
    Ok(())
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::auth::{SubscriptionTier, User};

    /// OS keyring(Windows Credential Manager)은 프로세스 전역 외부 자원이라,
    /// 실제 항목을 읽고 쓰는 테스트들이 병렬로 돌면 간헐적으로 서로 간섭해
    /// 실패한다(플레이키). 해당 테스트들만 이 락으로 직렬화한다.
    static KEYRING_TEST_LOCK: once_cell::sync::Lazy<tokio::sync::Mutex<()>> =
        once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(()));

    fn test_user(tier: SubscriptionTier) -> User {
        test_user_with_id("test-user", tier)
    }

    fn test_user_with_id(id: &str, tier: SubscriptionTier) -> User {
        User {
            id: id.to_string(),
            email: "test@example.com".to_string(),
            tier,
            access_token: "access-token".to_string(),
            refresh_token: "refresh-token".to_string(),
            expires_at: i64::MAX,
        }
    }

    fn test_credentials(access_token: &str) -> YouTubeCredentials {
        YouTubeCredentials {
            access_token: access_token.to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Some(chrono::Utc::now().timestamp() + 3600),
            token_type: "Bearer".to_string(),
        }
    }

    fn test_youtube_manager(storage: Arc<Storage>) -> YouTubeManager {
        YouTubeManager::new(
            "test_client_id".to_string(),
            "test_client_secret".to_string(),
            "http://localhost:8080/callback".to_string(),
            storage,
        )
        .expect("test YouTube manager should initialize")
    }

    #[test]
    fn user_scoped_setting_keys_hex_encode_raw_user_id_bytes() {
        assert_eq!(encode_user_id_for_setting_key("a/b"), "612f62");
        assert_eq!(encode_user_id_for_setting_key("a_b"), "615f62");
        assert_ne!(
            youtube_credentials_key("a/b"),
            youtube_credentials_key("a_b")
        );

        let non_ascii_key = youtube_credentials_key("유저");
        assert_ne!(non_ascii_key, youtube_credentials_key("__"));
        assert!(non_ascii_key.starts_with("youtube_credentials_"));
        assert!(non_ascii_key
            .trim_start_matches("youtube_credentials_")
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn youtube_pro_access_allows_pro_users() {
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user(SubscriptionTier::Pro)).unwrap();

        assert!(require_youtube_access(&auth, "youtube_start_auth").is_ok());
    }

    #[test]
    fn youtube_scheduling_blocks_free_users() {
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user(SubscriptionTier::Free)).unwrap();

        let error = require_youtube_access(&auth, "youtube_schedule_upload").unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("PRO subscription required")
        ));
    }

    /// Signing in and uploading a single video are part of the free product;
    /// only scheduled/batch upload is paid. See
    /// `command_policy::local_export_stays_free_for_signed_in_users` for the
    /// reasoning behind the split.
    #[test]
    fn youtube_upload_is_free_for_signed_in_users() {
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user(SubscriptionTier::Free)).unwrap();

        for cmd_name in [
            "youtube_start_auth",
            "youtube_complete_auth",
            "youtube_upload_video",
        ] {
            assert!(
                require_youtube_access(&auth, cmd_name).is_ok(),
                "{cmd_name} must be usable by a signed-in FREE user"
            );
        }
    }

    #[test]
    fn youtube_pro_access_requires_authentication() {
        let auth = Arc::new(AuthManager::new());

        let error = require_youtube_access(&auth, "youtube_start_auth").unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("not authenticated")
        ));
    }

    #[test]
    fn youtube_pro_access_fail_closed_blocks_free_users_on_policy_miss() {
        // Regression guard: a `cmd_name` missing from `COMMAND_POLICIES` must
        // still hard-enforce PRO, not silently fall back to `AuthRequired`
        // (release builds compile out the `debug_assert_eq!` that would
        // otherwise catch this at the call site).
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user(SubscriptionTier::Free)).unwrap();

        let error =
            require_youtube_access_fail_closed(&auth, "totally_unregistered_command").unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("PRO subscription required")
        ));
    }

    #[test]
    fn youtube_pro_access_fail_closed_allows_pro_users_on_policy_miss() {
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user(SubscriptionTier::Pro)).unwrap();

        assert!(require_youtube_access_fail_closed(&auth, "totally_unregistered_command").is_ok());
    }

    #[test]
    fn youtube_commands_are_all_registered_and_split_correctly() {
        // The gate falls back to `AuthRequired` for an unregistered command, so a
        // missing entry would silently downgrade a paid command. Every name this
        // module gates must be in the table, and the paid half must stay paid.
        const PAID: [&str; 3] = [
            "youtube_schedule_upload",
            "youtube_get_upload_queue",
            "youtube_cancel_scheduled_upload",
        ];
        const FREE_FOR_SIGNED_IN: [&str; 11] = [
            "youtube_start_auth",
            "youtube_start_auth_with_server",
            "youtube_complete_auth",
            "youtube_get_auth_status",
            "youtube_upload_video",
            "youtube_get_upload_progress",
            "youtube_get_video_details",
            "youtube_get_upload_history",
            "youtube_add_to_history",
            "youtube_get_quota_info",
            "youtube_logout",
        ];

        for cmd_name in PAID {
            assert_eq!(
                command_policy::command_access(cmd_name),
                Some(command_policy::CommandAccess::ProRequired),
                "{cmd_name} must stay PRO-gated"
            );
        }
        for cmd_name in FREE_FOR_SIGNED_IN {
            assert_eq!(
                command_policy::command_access(cmd_name),
                Some(command_policy::CommandAccess::AuthRequired),
                "{cmd_name} must be usable without PRO"
            );
        }
    }

    #[test]
    fn youtube_callback_validation_rejects_different_active_user() {
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user_with_id("starting-user", SubscriptionTier::Pro))
            .unwrap();

        assert!(
            require_same_youtube_pro_user(&auth, "youtube_complete_auth", "starting-user").is_ok()
        );

        auth.login(test_user_with_id("different-user", SubscriptionTier::Pro))
            .unwrap();

        let error = require_same_youtube_pro_user(&auth, "youtube_complete_auth", "starting-user")
            .unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("changed")
        ));
    }

    #[test]
    fn youtube_callback_validation_accepts_same_free_user() {
        // `youtube_complete_auth` is free now, so a FREE user finishing their own
        // sign-in must succeed. The identity check below is what this guard is for.
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user_with_id("starting-user", SubscriptionTier::Free))
            .unwrap();

        assert!(
            require_same_youtube_pro_user(&auth, "youtube_complete_auth", "starting-user").is_ok()
        );

        let error = require_same_youtube_pro_user(&auth, "youtube_complete_auth", "someone-else")
            .unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("Authenticated user changed")
        ));
    }

    #[tokio::test]
    async fn unowned_legacy_credentials_are_cleared_not_loaded() {
        let _keyring_guard = KEYRING_TEST_LOCK.lock().await;
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(Arc::clone(&storage));
        let legacy_credentials = test_credentials("legacy-access-token");
        let legacy_json = serde_json::to_string(&legacy_credentials)
            .expect("credentials should serialize for test setup");

        storage
            .set_setting(YOUTUBE_CREDENTIALS_LEGACY_KEY, &legacy_json)
            .await
            .expect("legacy credentials should be written");

        let loaded = manager
            .load_credentials_for_user("current-user")
            .await
            .expect("legacy credential cleanup should succeed");

        assert!(loaded.is_none());
        assert!(manager.oauth_client.get_credentials().await.is_none());
        assert!(matches!(
            storage.get_setting(YOUTUBE_CREDENTIALS_LEGACY_KEY).await,
            Err(StorageError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound
        ));
    }

    #[tokio::test]
    async fn collision_prone_user_ids_do_not_share_credentials() {
        let _keyring_guard = KEYRING_TEST_LOCK.lock().await;
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(Arc::clone(&storage));
        let slash_user_credentials = test_credentials("slash-user-access-token");
        let slash_user_json = serde_json::to_string(&slash_user_credentials)
            .expect("credentials should serialize for test setup");

        storage
            .set_setting(&youtube_credentials_key("a/b"), &slash_user_json)
            .await
            .expect("slash user credentials should be written");

        let loaded_for_underscore_user = manager
            .load_credentials_for_user("a_b")
            .await
            .expect("missing credentials for colliding old key should be handled");

        assert!(loaded_for_underscore_user.is_none());
        assert!(manager.oauth_client.get_credentials().await.is_none());

        let loaded_for_slash_user = manager
            .load_credentials_for_user("a/b")
            .await
            .expect("slash user credentials should load")
            .expect("slash user credentials should exist");

        assert_eq!(
            loaded_for_slash_user.access_token,
            "slash-user-access-token"
        );

        // The load above may have migrated the plaintext SQLite row into the OS
        // keyring; clean it up so repeated local test runs don't accumulate entries.
        delete_credentials_from_keyring(&youtube_credentials_key("a/b"));
    }

    #[tokio::test]
    async fn credentials_for_one_user_are_unavailable_to_another_user() {
        let _keyring_guard = KEYRING_TEST_LOCK.lock().await;
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(Arc::clone(&storage));
        let user_a_credentials = test_credentials("user-a-access-token");
        let user_a_json = serde_json::to_string(&user_a_credentials)
            .expect("credentials should serialize for test setup");

        storage
            .set_setting(&youtube_credentials_key("user-a"), &user_a_json)
            .await
            .expect("user-scoped credentials should be written");

        let loaded_for_user_b = manager
            .load_credentials_for_user("user-b")
            .await
            .expect("missing credentials for other user should be handled");

        assert!(loaded_for_user_b.is_none());
        assert!(manager.oauth_client.get_credentials().await.is_none());

        let loaded_for_user_a = manager
            .load_credentials_for_user("user-a")
            .await
            .expect("owner credentials should load")
            .expect("owner credentials should exist");

        assert_eq!(loaded_for_user_a.access_token, "user-a-access-token");

        // The load above may have migrated the plaintext SQLite row into the OS
        // keyring; clean it up so repeated local test runs don't accumulate entries.
        delete_credentials_from_keyring(&youtube_credentials_key("user-a"));
    }

    #[tokio::test]
    async fn saved_credentials_round_trip_and_clear_via_manager() {
        let _keyring_guard = KEYRING_TEST_LOCK.lock().await;
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(Arc::clone(&storage));
        let creds = test_credentials("round-trip-access-token");

        manager
            .save_credentials_for_user("round-trip-user", &creds)
            .await
            .expect("credentials should save");

        let loaded = manager
            .load_credentials_for_user("round-trip-user")
            .await
            .expect("credentials should load")
            .expect("credentials should exist");
        assert_eq!(loaded.access_token, "round-trip-access-token");
        assert_eq!(loaded.refresh_token, creds.refresh_token);

        manager
            .clear_credentials_for_user("round-trip-user")
            .await
            .expect("credentials should clear");

        let cleared = manager
            .load_credentials_for_user("round-trip-user")
            .await
            .expect("load after clear should succeed");
        assert!(cleared.is_none());
    }

    #[tokio::test]
    async fn pending_oauth_owner_allows_same_user_completion() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(storage);

        manager
            .set_pending_oauth_owner_for_user("starting-user")
            .await;

        assert!(manager
            .require_pending_oauth_owner_for_user("starting-user")
            .await
            .is_ok());
        assert_eq!(
            manager.pending_oauth_owner().await,
            Some("starting-user".to_string())
        );

        manager
            .clear_pending_oauth_owner_if_matches("starting-user")
            .await;

        assert!(manager.pending_oauth_owner().await.is_none());
    }

    #[tokio::test]
    async fn pending_oauth_owner_rejects_different_user_completion() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(storage);

        manager
            .set_pending_oauth_owner_for_user("starting-user")
            .await;

        let error = manager
            .require_pending_oauth_owner_for_user("different-user")
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("changed")
        ));
        assert_eq!(
            manager.pending_oauth_owner().await,
            Some("starting-user".to_string())
        );
    }

    #[tokio::test]
    async fn pending_oauth_owner_requires_started_flow() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(storage);

        let error = manager
            .require_pending_oauth_owner_for_user("current-user")
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("No pending")
        ));
    }

    #[tokio::test]
    async fn active_pending_oauth_user_allows_same_pro_user() {
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user_with_id("starting-user", SubscriptionTier::Pro))
            .unwrap();
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(storage);

        manager
            .set_pending_oauth_owner_for_user("starting-user")
            .await;

        assert!(require_active_pending_youtube_oauth_user(
            &auth,
            "youtube_complete_auth",
            &manager,
            "starting-user"
        )
        .await
        .is_ok());
    }

    #[tokio::test]
    async fn active_pending_oauth_user_rejects_switched_user() {
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user_with_id("starting-user", SubscriptionTier::Pro))
            .unwrap();
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(storage);

        manager
            .set_pending_oauth_owner_for_user("starting-user")
            .await;
        auth.login(test_user_with_id("different-user", SubscriptionTier::Pro))
            .unwrap();

        let error = require_active_pending_youtube_oauth_user(
            &auth,
            "youtube_complete_auth",
            &manager,
            "starting-user",
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("changed")
        ));
    }

    #[tokio::test]
    async fn active_pending_oauth_user_rejects_after_pending_owner_cleared() {
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user_with_id("starting-user", SubscriptionTier::Pro))
            .unwrap();
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(storage);

        manager
            .set_pending_oauth_owner_for_user("starting-user")
            .await;
        manager.clear_pending_oauth_owner().await;

        let error = require_active_pending_youtube_oauth_user(
            &auth,
            "youtube_complete_auth",
            &manager,
            "starting-user",
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("No pending")
        ));
        assert!(manager.pending_oauth_owner().await.is_none());
    }

    #[tokio::test]
    async fn clearing_app_auth_oauth_state_cancels_pending_completion_and_memory_credentials() {
        let auth = Arc::new(AuthManager::new());
        auth.login(test_user_with_id("starting-user", SubscriptionTier::Pro))
            .unwrap();
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let storage = Arc::new(Storage::new(temp_dir.path()).expect("storage should initialize"));
        let manager = test_youtube_manager(storage);

        manager
            .set_pending_oauth_owner_for_user("starting-user")
            .await;
        manager
            .oauth_client
            .set_credentials(test_credentials("stale-token"))
            .await;
        manager.clear_app_auth_oauth_state().await;

        let error = require_active_pending_youtube_oauth_user(
            &auth,
            "youtube_complete_auth",
            &manager,
            "starting-user",
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            AppError::Auth(message) if message.contains("No pending")
        ));
        assert!(manager.pending_oauth_owner().await.is_none());
        assert!(manager.oauth_client.get_credentials().await.is_none());
    }

    fn test_scheduled_upload(
        id: &str,
        scheduled_at: Option<String>,
        status: ScheduledUploadStatus,
    ) -> ScheduledUpload {
        ScheduledUpload {
            id: id.to_string(),
            video_path: "C:/videos/test.mp4".to_string(),
            title: format!("Test upload {}", id),
            description: "description".to_string(),
            tags: vec!["lol".to_string()],
            privacy_status: "private".to_string(),
            thumbnail_path: None,
            schedule: UploadSchedule {
                scheduled_at,
                queue_position: Some(0),
            },
            created_at: 1,
            status,
            error_message: None,
            attempts: 0,
            completed_video_id: None,
            updated_at: None,
        }
    }

    #[test]
    fn due_scheduler_marks_only_pending_due_uploads_running() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-04-25T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut queue = vec![
            test_scheduled_upload(
                "due",
                Some("2026-04-25T11:59:59Z".to_string()),
                ScheduledUploadStatus::Pending,
            ),
            test_scheduled_upload(
                "future",
                Some("2026-04-25T12:01:00Z".to_string()),
                ScheduledUploadStatus::Pending,
            ),
            test_scheduled_upload(
                "running",
                Some("2026-04-25T11:00:00Z".to_string()),
                ScheduledUploadStatus::Running,
            ),
            test_scheduled_upload(
                "completed",
                Some("2026-04-25T11:00:00Z".to_string()),
                ScheduledUploadStatus::Completed,
            ),
        ];

        let (due_uploads, failed_events) = mark_due_scheduled_uploads_running(&mut queue, now);

        assert_eq!(due_uploads.len(), 1);
        assert_eq!(due_uploads[0].id, "due");
        assert_eq!(queue[0].status, ScheduledUploadStatus::Running);
        assert_eq!(queue[0].attempts, 1);
        assert_eq!(queue[1].status, ScheduledUploadStatus::Pending);
        assert_eq!(queue[2].status, ScheduledUploadStatus::Running);
        assert_eq!(queue[3].status, ScheduledUploadStatus::Completed);
        assert!(failed_events.is_empty());
    }

    #[test]
    fn invalid_schedule_is_marked_failed_without_upload_attempt() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-04-25T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut queue = vec![test_scheduled_upload(
            "invalid",
            Some("not-a-date".to_string()),
            ScheduledUploadStatus::Pending,
        )];

        let (due_uploads, failed_events) = mark_due_scheduled_uploads_running(&mut queue, now);

        assert!(due_uploads.is_empty());
        assert_eq!(failed_events.len(), 1);
        assert_eq!(queue[0].status, ScheduledUploadStatus::Failed);
        assert_eq!(queue[0].attempts, 0);
        assert!(queue[0]
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("Invalid scheduled_at"));
    }
}

/// Background scheduler that checks for pending scheduled uploads every 60 seconds
/// and executes them when their scheduled time has arrived.
pub async fn start_upload_scheduler(app_handle: tauri::AppHandle) {
    use tauri::Emitter;
    use tauri::Manager;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        let state = app_handle.state::<crate::AppState>();
        let youtube = Arc::clone(&state.youtube_manager);
        // Not itself a `#[tauri::command]` entry point — this background loop resumes
        // the scheduled-upload queue created by `youtube_schedule_upload`, so it's
        // gated as that command for COMMAND_POLICIES purposes.
        let user = match require_youtube_access(&state.auth, "youtube_schedule_upload") {
            Ok(user) => user,
            Err(_) => continue,
        };
        let queue_key = youtube_upload_queue_key(&user.id);

        let now = chrono::Utc::now();
        let mut queue = match load_upload_queue(&youtube.storage, &queue_key).await {
            Ok(queue) => queue,
            Err(error) => {
                warn!("Failed to load scheduled upload queue: {}", error);
                continue;
            }
        };

        let (due_uploads, failed_events) = mark_due_scheduled_uploads_running(&mut queue, now);
        if due_uploads.is_empty() && failed_events.is_empty() {
            continue;
        }

        if let Err(error) = save_upload_queue(&youtube.storage, &queue_key, &queue).await {
            warn!(
                "Failed to persist scheduled upload running state: {}",
                error
            );
            continue;
        }

        for event in failed_events {
            if let Err(error) = app_handle.emit("scheduled-upload-failed", &event) {
                warn!("Failed to emit scheduled-upload-failed event: {}", error);
            }
        }

        for due_upload in due_uploads {
            let result = {
                let _credential_guard = youtube.credential_operation_lock.lock().await;
                upload_scheduled_video_for_user(&youtube, &user.id, &due_upload).await
            };

            let mut queue = match load_upload_queue(&youtube.storage, &queue_key).await {
                Ok(queue) => queue,
                Err(error) => {
                    warn!("Failed to reload scheduled upload queue: {}", error);
                    continue;
                }
            };

            let event = match result {
                Ok(video) => {
                    if let Err(error) =
                        record_youtube_upload_history(&youtube.storage, &user.id, &video).await
                    {
                        warn!("Failed to record scheduled upload history: {}", error);
                    }
                    if let Err(error) =
                        record_youtube_quota_usage(&youtube.storage, &user.id, 1600).await
                    {
                        warn!("Failed to record scheduled upload quota: {}", error);
                    }

                    let mut attempts = due_upload.attempts;
                    if let Some(queue_entry) =
                        queue.iter_mut().find(|entry| entry.id == due_upload.id)
                    {
                        queue_entry.status = ScheduledUploadStatus::Completed;
                        queue_entry.error_message = None;
                        queue_entry.completed_video_id = Some(video.id.clone());
                        queue_entry.updated_at = Some(chrono::Utc::now().timestamp());
                        queue_entry.schedule.queue_position = None;
                        attempts = queue_entry.attempts;
                    }

                    ScheduledUploadEvent {
                        upload_id: due_upload.id.clone(),
                        status: ScheduledUploadStatus::Completed,
                        video_id: Some(video.id),
                        title: due_upload.title.clone(),
                        attempts,
                        error_message: None,
                    }
                }
                Err(error) => {
                    let error_message = error.to_string();
                    let mut attempts = due_upload.attempts;
                    if let Some(queue_entry) =
                        queue.iter_mut().find(|entry| entry.id == due_upload.id)
                    {
                        queue_entry.status = ScheduledUploadStatus::Failed;
                        queue_entry.error_message = Some(error_message.clone());
                        queue_entry.updated_at = Some(chrono::Utc::now().timestamp());
                        queue_entry.schedule.queue_position = None;
                        attempts = queue_entry.attempts;
                    }

                    ScheduledUploadEvent {
                        upload_id: due_upload.id.clone(),
                        status: ScheduledUploadStatus::Failed,
                        video_id: None,
                        title: due_upload.title.clone(),
                        attempts,
                        error_message: Some(error_message),
                    }
                }
            };

            if let Err(error) = save_upload_queue(&youtube.storage, &queue_key, &queue).await {
                warn!("Failed to persist scheduled upload result: {}", error);
            }

            let event_name = match event.status {
                ScheduledUploadStatus::Completed => "scheduled-upload-completed",
                ScheduledUploadStatus::Failed => "scheduled-upload-failed",
                _ => continue,
            };
            if let Err(error) = app_handle.emit(event_name, &event) {
                warn!("Failed to emit {} event: {}", event_name, error);
            }
        }
    }
}
