pub mod commands;
pub mod models;

use crate::utils::security;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use thiserror::Error;

// Re-export public types
pub use models::{
    AutoEditResultMetadata, AutoEditUsage, ClipMetadata, EventData, GameMetadata, StorageStats,
    UploadStatus, YouTubeUploadStatus,
};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Storage lock error: {0}")]
    Lock(String),
    #[error("Game not found: {0}")]
    GameNotFound(String),
    #[error("Security validation error: {0}")]
    Security(#[from] crate::utils::security::SecurityError),
}

pub type Result<T> = std::result::Result<T, StorageError>;

const SAFE_DELETE_MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "avi", "mkv", "mov", "flv", "webm", "m4v", "jpg", "jpeg", "png", "gif", "webp",
];
const JSON_TO_SQLITE_MIGRATION: &str = "json_to_sqlite_v1";

/// SQLite-backed local storage for app-owned metadata.
///
/// This database stores only local app data such as games, clips, app settings,
/// and auto-edit results. Authentication, billing, and PRO entitlement remain
/// authoritative in Supabase and must not be trusted from this local database.
pub struct Storage {
    base_path: PathBuf,
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageHealth {
    pub database_path: String,
    pub database_size_bytes: u64,
    pub integrity_ok: bool,
    pub integrity_message: String,
}

impl Storage {
    /// Create a new storage instance.
    pub fn new(base_path: impl AsRef<Path>) -> Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();

        fs::create_dir_all(&base_path)?;
        fs::create_dir_all(base_path.join("clips"))?;
        fs::create_dir_all(base_path.join("recordings"))?;
        fs::create_dir_all(base_path.join("replays"))?;

        let db_path = base_path.join("lolshorts.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS local_migrations (
                name TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS games (
                game_id TEXT PRIMARY KEY,
                metadata_json TEXT NOT NULL,
                champion TEXT NOT NULL,
                game_mode TEXT NOT NULL,
                start_time TEXT NOT NULL,
                end_time TEXT,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS events (
                game_id TEXT PRIMARY KEY,
                events_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS clips (
                game_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                event_time REAL NOT NULL,
                priority INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (game_id, file_path)
            );

            CREATE INDEX IF NOT EXISTS idx_games_start_time ON games(start_time DESC);
            CREATE INDEX IF NOT EXISTS idx_clips_game_id ON clips(game_id);
            CREATE INDEX IF NOT EXISTS idx_clips_priority ON clips(priority DESC);
            CREATE INDEX IF NOT EXISTS idx_clips_event_time ON clips(event_time);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS auto_edit_usage (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                usage_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS auto_edit_results (
                result_id TEXT PRIMARY KEY,
                metadata_json TEXT NOT NULL,
                output_path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_auto_edit_results_created_at
                ON auto_edit_results(created_at DESC);
            "#,
        )?;

        let storage = Self {
            base_path,
            conn: Mutex::new(conn),
        };

        if let Err(err) = storage.migrate_json_files_to_sqlite() {
            tracing::warn!(
                "SQLite storage initialized but JSON migration was incomplete; legacy files were preserved: {}",
                err
            );
        }

        tracing::info!(
            "SQLite storage initialized at: {}",
            storage.database_path().display()
        );

        Ok(storage)
    }

    /// Get the base storage path.
    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    /// Get the SQLite database path.
    pub fn database_path(&self) -> PathBuf {
        self.base_path.join("lolshorts.db")
    }

    /// Return a safe diagnostic summary of local SQLite health.
    pub fn health_check(&self) -> Result<StorageHealth> {
        let db_path = self.database_path();
        let database_size_bytes = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
        let integrity_message: String =
            self.conn()?
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let integrity_ok = integrity_message.eq_ignore_ascii_case("ok");

        Ok(StorageHealth {
            database_path: db_path.display().to_string(),
            database_size_bytes,
            integrity_ok,
            integrity_message,
        })
    }

    /// Return setting keys only, never setting values, for support diagnostics.
    pub fn diagnostic_setting_keys(&self) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT key FROM settings ORDER BY key ASC")?;
        let keys = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(keys
            .into_iter()
            .map(|key| {
                let lower = key.to_ascii_lowercase();
                if lower.contains("credential")
                    || lower.contains("token")
                    || lower.contains("secret")
                    || lower.contains("payment")
                    || lower.contains("toss")
                    || lower.contains("supabase")
                {
                    format!("{}:[redacted]", key)
                } else {
                    key
                }
            })
            .collect())
    }

    /// Get path for a specific game.
    pub fn game_path(&self, game_id: &str) -> PathBuf {
        self.base_path.join("clips").join(game_id)
    }

    /// App-owned roots where media/result files may be deleted from metadata.
    pub fn safe_delete_roots(&self) -> Vec<PathBuf> {
        vec![
            self.base_path.join("clips"),
            self.base_path.join("recordings"),
            self.base_path.join("replays"),
            std::env::temp_dir().join("lolshorts_auto_edit"),
        ]
    }

    /// Safely delete an app-owned media/result file referenced by local metadata.
    pub fn safe_delete_media_file(
        &self,
        file_path: impl AsRef<Path>,
    ) -> Result<security::SafeDeleteOutcome> {
        let file_path = file_path.as_ref();
        validate_safe_delete_media_extension(file_path)?;
        security::safe_delete_file_within_roots(file_path, &self.safe_delete_roots())
            .map_err(StorageError::from)
    }

    /// Create a new game directory and metadata row.
    pub fn create_game(&self, game_id: &str, metadata: &GameMetadata) -> Result<()> {
        fs::create_dir_all(self.game_path(game_id))?;
        self.save_game_metadata(game_id, metadata)?;
        tracing::info!("Created game storage entry: {}", game_id);
        Ok(())
    }

    /// Save game metadata.
    pub fn save_game_metadata(&self, game_id: &str, metadata: &GameMetadata) -> Result<()> {
        fs::create_dir_all(self.game_path(game_id))?;

        let json = serde_json::to_string(metadata)?;
        let now = chrono::Utc::now().to_rfc3339();
        self.conn()?.execute(
            r#"
            INSERT INTO games (game_id, metadata_json, champion, game_mode, start_time, end_time, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(game_id) DO UPDATE SET
                metadata_json = excluded.metadata_json,
                champion = excluded.champion,
                game_mode = excluded.game_mode,
                start_time = excluded.start_time,
                end_time = excluded.end_time,
                updated_at = excluded.updated_at
            "#,
            params![
                game_id,
                &json,
                &metadata.champion,
                &metadata.game_mode,
                metadata.start_time.to_rfc3339(),
                metadata.end_time.map(|dt| dt.to_rfc3339()),
                &now,
            ],
        )?;

        Ok(())
    }

    /// Save game metadata asynchronously.
    pub async fn save_game_metadata_async(
        &self,
        game_id: &str,
        metadata: &GameMetadata,
    ) -> Result<()> {
        self.save_game_metadata(game_id, metadata)?;
        tracing::info!("Saved game metadata asynchronously: {}", game_id);
        Ok(())
    }

    /// Load game metadata.
    pub fn load_game_metadata(&self, game_id: &str) -> Result<GameMetadata> {
        let json: Option<String> = self
            .conn()?
            .query_row(
                "SELECT metadata_json FROM games WHERE game_id = ?1",
                params![game_id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(json) = json {
            return Ok(serde_json::from_str(&json)?);
        }

        let legacy = self.game_path(game_id).join("metadata.json");
        if legacy.exists() {
            let metadata: GameMetadata = read_json_file(&legacy)?;
            self.save_game_metadata(game_id, &metadata)?;
            return Ok(metadata);
        }

        Err(StorageError::GameNotFound(game_id.to_string()))
    }

    /// Save events for a game.
    pub fn save_events(&self, game_id: &str, events: &[EventData]) -> Result<()> {
        fs::create_dir_all(self.game_path(game_id))?;
        let json = serde_json::to_string(events)?;
        let now = chrono::Utc::now().to_rfc3339();

        self.conn()?.execute(
            r#"
            INSERT INTO events (game_id, events_json, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(game_id) DO UPDATE SET
                events_json = excluded.events_json,
                updated_at = excluded.updated_at
            "#,
            params![game_id, json, now],
        )?;

        tracing::debug!("Saved {} events for game {}", events.len(), game_id);
        Ok(())
    }

    /// Load events for a game.
    pub fn load_events(&self, game_id: &str) -> Result<Vec<EventData>> {
        let json: Option<String> = self
            .conn()?
            .query_row(
                "SELECT events_json FROM events WHERE game_id = ?1",
                params![game_id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(json) = json {
            return Ok(serde_json::from_str(&json)?);
        }

        let legacy = self.game_path(game_id).join("events.json");
        if legacy.exists() {
            let events: Vec<EventData> = read_json_file(&legacy)?;
            self.save_events(game_id, &events)?;
            return Ok(events);
        }

        Ok(Vec::new())
    }

    /// Save clip metadata.
    pub fn save_clip_metadata(&self, game_id: &str, clip: &ClipMetadata) -> Result<()> {
        fs::create_dir_all(self.game_path(game_id))?;

        let json = serde_json::to_string(clip)?;
        let now = chrono::Utc::now().to_rfc3339();
        self.conn()?.execute(
            r#"
            INSERT INTO clips (game_id, file_path, metadata_json, event_time, priority, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(game_id, file_path) DO UPDATE SET
                metadata_json = excluded.metadata_json,
                event_time = excluded.event_time,
                priority = excluded.priority,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at
            "#,
            params![
                game_id,
                &clip.file_path,
                &json,
                clip.event_time,
                clip.priority,
                clip.created_at.to_rfc3339(),
                &now,
            ],
        )?;

        Ok(())
    }

    /// Load all clip metadata for a game.
    pub fn load_clip_metadata(&self, game_id: &str) -> Result<Vec<ClipMetadata>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT metadata_json FROM clips WHERE game_id = ?1 ORDER BY created_at DESC, rowid DESC",
        )?;
        let clips = stmt
            .query_map(params![game_id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);

        if !clips.is_empty() {
            return clips
                .into_iter()
                .map(|json| serde_json::from_str(&json).map_err(StorageError::from))
                .collect();
        }

        let legacy = self.game_path(game_id).join("clips.json");
        if legacy.exists() {
            let clips: Vec<ClipMetadata> = read_json_file(&legacy)?;
            for clip in &clips {
                self.save_clip_metadata(game_id, clip)?;
            }
            return Ok(clips);
        }

        Ok(Vec::new())
    }

    /// Get all games, sorted by most recent.
    pub fn list_games(&self) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT game_id FROM games ORDER BY start_time DESC")?;
        let games = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);

        if !games.is_empty() {
            return Ok(games);
        }

        self.list_legacy_game_dirs()
    }

    /// Delete a game and all its local metadata.
    pub fn delete_game(&self, game_id: &str) -> Result<()> {
        {
            let conn = self.conn()?;
            conn.execute("DELETE FROM clips WHERE game_id = ?1", params![game_id])?;
            conn.execute("DELETE FROM events WHERE game_id = ?1", params![game_id])?;
            conn.execute("DELETE FROM games WHERE game_id = ?1", params![game_id])?;
        }

        let game_path = self.game_path(game_id);
        if game_path.exists() {
            fs::remove_dir_all(game_path)?;
        }

        tracing::info!("Deleted game: {}", game_id);
        Ok(())
    }

    /// Delete a specific clip's metadata from storage.
    pub fn delete_clip_metadata(&self, game_id: &str, file_path: &str) -> Result<()> {
        let affected = self.conn()?.execute(
            "DELETE FROM clips WHERE game_id = ?1 AND file_path = ?2",
            params![game_id, file_path],
        )?;

        if affected == 0 {
            tracing::warn!("Clip not found in metadata: {}", file_path);
        } else {
            tracing::info!("Removed clip from metadata: {}", file_path);
        }

        Ok(())
    }

    /// Get storage statistics.
    pub fn get_stats(&self) -> Result<StorageStats> {
        let total_games: usize =
            self.conn()?
                .query_row("SELECT COUNT(*) FROM games", [], |row| {
                    row.get::<_, i64>(0).map(|v| v as usize)
                })?;

        let clips = self.all_clip_metadata()?;
        let total_clips = clips.len();
        let mut total_size = 0u64;

        for clip in clips {
            if let Ok(metadata) = fs::metadata(&clip.file_path) {
                total_size += metadata.len();
            }
        }

        Ok(StorageStats {
            total_games,
            total_clips,
            total_size_bytes: total_size,
        })
    }

    // ========================================================================
    // Canvas Template Storage
    // ========================================================================

    /// Save a canvas template to the template library.
    ///
    /// Templates remain as local files because they are user-editable assets, not
    /// authoritative billing/auth state.
    pub fn save_canvas_template(&self, template: &crate::video::CanvasTemplate) -> Result<()> {
        let templates_dir = self.base_path.join("templates");
        fs::create_dir_all(&templates_dir)?;

        let template_path = templates_dir.join(format!("{}.json", template.id));
        let json = serde_json::to_string_pretty(template)?;
        fs::write(template_path, json)?;

        tracing::info!("Saved canvas template: {} ({})", template.name, template.id);
        Ok(())
    }

    /// Load a canvas template by ID.
    pub fn load_canvas_template(&self, template_id: &str) -> Result<crate::video::CanvasTemplate> {
        let template_path = self
            .base_path
            .join("templates")
            .join(format!("{}.json", template_id));

        if !template_path.exists() {
            return Err(StorageError::GameNotFound(format!(
                "Template not found: {}",
                template_id
            )));
        }

        read_json_file(&template_path)
    }

    /// List all available canvas templates.
    pub fn list_canvas_templates(&self) -> Result<Vec<CanvasTemplateInfo>> {
        let templates_dir = self.base_path.join("templates");

        if !templates_dir.exists() {
            return Ok(Vec::new());
        }

        let mut templates = Vec::new();

        for entry in fs::read_dir(templates_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(template) = read_json_file::<crate::video::CanvasTemplate>(&path) {
                    templates.push(CanvasTemplateInfo {
                        id: template.id.clone(),
                        name: template.name.clone(),
                        element_count: template.elements.len(),
                    });
                }
            }
        }

        templates.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(templates)
    }

    /// Delete a canvas template.
    pub fn delete_canvas_template(&self, template_id: &str) -> Result<()> {
        let template_path = self
            .base_path
            .join("templates")
            .join(format!("{}.json", template_id));

        if template_path.exists() {
            fs::remove_file(template_path)?;
            tracing::info!("Deleted canvas template: {}", template_id);
        }

        Ok(())
    }

    // ========================================================================
    // Generic Settings Storage
    // ========================================================================

    /// Get a setting value by key.
    pub async fn get_setting(&self, key: &str) -> Result<String> {
        let value: Option<String> = self
            .conn()?
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;

        value.ok_or_else(|| {
            StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Setting not found: {}", key),
            ))
        })
    }

    /// Set a setting value by key.
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn()?.execute(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            "#,
            params![key, value, now],
        )?;
        Ok(())
    }

    /// Remove a setting by key.
    pub async fn remove_setting(&self, key: &str) -> Result<()> {
        self.conn()?
            .execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(())
    }

    // ========================================================================
    // Auto-Edit Usage Tracking (Quota System)
    // ========================================================================

    /// Load auto-edit usage for current month.
    pub fn load_auto_edit_usage(&self) -> Result<AutoEditUsage> {
        let json: Option<String> = self
            .conn()?
            .query_row(
                "SELECT usage_json FROM auto_edit_usage WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let mut usage = if let Some(json) = json {
            serde_json::from_str(&json)?
        } else {
            let legacy = self.base_path.join("auto_edit_usage.json");
            if legacy.exists() {
                let usage: AutoEditUsage = read_json_file(&legacy)?;
                self.save_auto_edit_usage(&usage)?;
                usage
            } else {
                AutoEditUsage::default()
            }
        };

        if !usage.is_current_month() {
            tracing::info!(
                "Resetting auto-edit usage for new month: {} -> {}",
                usage.month,
                AutoEditUsage::current_month()
            );
            usage = AutoEditUsage::reset_for_month(AutoEditUsage::current_month());
            self.save_auto_edit_usage(&usage)?;
        }

        Ok(usage)
    }

    /// Save auto-edit usage.
    fn save_auto_edit_usage(&self, usage: &AutoEditUsage) -> Result<()> {
        let json = serde_json::to_string(usage)?;
        let now = chrono::Utc::now().to_rfc3339();
        self.conn()?.execute(
            r#"
            INSERT INTO auto_edit_usage (id, usage_json, updated_at)
            VALUES (1, ?1, ?2)
            ON CONFLICT(id) DO UPDATE SET
                usage_json = excluded.usage_json,
                updated_at = excluded.updated_at
            "#,
            params![json, now],
        )?;

        tracing::debug!(
            "Saved auto-edit usage: month={}, count={}",
            usage.month,
            usage.usage_count
        );
        Ok(())
    }

    /// Increment auto-edit usage counter.
    pub fn increment_auto_edit_usage(&self) -> Result<u32> {
        let mut usage = self.load_auto_edit_usage()?;

        usage.usage_count += 1;
        usage.last_updated = chrono::Utc::now();

        self.save_auto_edit_usage(&usage)?;

        tracing::info!(
            "Auto-edit usage incremented: {}/{} (month: {})",
            usage.usage_count,
            "∞",
            usage.month
        );

        Ok(usage.usage_count)
    }

    /// Check if user can perform auto-edit based on quota.
    pub fn check_auto_edit_quota(&self, is_pro: bool) -> Result<u32> {
        if is_pro {
            return Ok(u32::MAX);
        }

        const FREE_TIER_LIMIT: u32 = 5;

        let usage = self.load_auto_edit_usage()?;

        if usage.usage_count >= FREE_TIER_LIMIT {
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "Monthly auto-edit quota exceeded ({}/{}). Upgrade to PRO for unlimited usage.",
                    usage.usage_count, FREE_TIER_LIMIT
                ),
            )));
        }

        Ok(FREE_TIER_LIMIT - usage.usage_count)
    }

    // ========================================================================
    // Auto-Edit Result Storage
    // ========================================================================

    /// Save auto-edit result metadata.
    pub fn save_auto_edit_result(&self, result: &models::AutoEditResultMetadata) -> Result<()> {
        let json = serde_json::to_string(result)?;
        let now = chrono::Utc::now().to_rfc3339();
        self.conn()?.execute(
            r#"
            INSERT INTO auto_edit_results (result_id, metadata_json, output_path, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(result_id) DO UPDATE SET
                metadata_json = excluded.metadata_json,
                output_path = excluded.output_path,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at
            "#,
            params![
                &result.result_id,
                &json,
                &result.output_path,
                result.created_at.to_rfc3339(),
                &now,
            ],
        )?;

        tracing::info!(
            "Saved auto-edit result: {} (duration: {:.1}s, clips: {})",
            result.result_id,
            result.duration,
            result.clip_count
        );

        Ok(())
    }

    /// Load all auto-edit results, sorted by most recent first.
    pub fn load_auto_edit_results(&self) -> Result<Vec<models::AutoEditResultMetadata>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT metadata_json FROM auto_edit_results ORDER BY created_at DESC")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);

        if !rows.is_empty() {
            return rows
                .into_iter()
                .map(|json| serde_json::from_str(&json).map_err(StorageError::from))
                .collect();
        }

        let legacy = self.base_path.join("auto_edit_results.json");
        if legacy.exists() {
            let results: Vec<models::AutoEditResultMetadata> = read_json_file(&legacy)?;
            for result in &results {
                self.save_auto_edit_result(result)?;
            }
            return Ok(results);
        }

        Ok(Vec::new())
    }

    /// Load a specific auto-edit result by ID.
    pub fn load_auto_edit_result(&self, result_id: &str) -> Result<models::AutoEditResultMetadata> {
        let json: Option<String> = self
            .conn()?
            .query_row(
                "SELECT metadata_json FROM auto_edit_results WHERE result_id = ?1",
                params![result_id],
                |row| row.get(0),
            )
            .optional()?;

        json.map(|value| serde_json::from_str(&value).map_err(StorageError::from))
            .transpose()?
            .ok_or_else(|| {
                StorageError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Auto-edit result not found: {}", result_id),
                ))
            })
    }

    /// Delete an auto-edit result and optionally its video file.
    pub fn delete_auto_edit_result(&self, result_id: &str, delete_file: bool) -> Result<()> {
        let result = self.load_auto_edit_result(result_id)?;

        if delete_file {
            let mut deleted_paths = vec![result.output_path.clone()];
            if let Some(thumb_path) = &result.thumbnail_path {
                deleted_paths.push(thumb_path.clone());
            }

            for file_path in deleted_paths {
                match self.safe_delete_media_file(&file_path)? {
                    security::SafeDeleteOutcome::Deleted(path) => {
                        tracing::info!("Deleted auto-edit media file: {:?}", path);
                    }
                    security::SafeDeleteOutcome::Missing(path) => {
                        tracing::warn!("Auto-edit media file already missing: {:?}", path);
                    }
                }
            }
        }

        self.conn()?.execute(
            "DELETE FROM auto_edit_results WHERE result_id = ?1",
            params![result_id],
        )?;

        tracing::info!("Deleted auto-edit result: {}", result_id);
        Ok(())
    }

    /// Update YouTube upload status for an auto-edit result.
    pub fn update_auto_edit_youtube_status(
        &self,
        result_id: &str,
        status: models::YouTubeUploadStatus,
    ) -> Result<()> {
        let mut result = self.load_auto_edit_result(result_id)?;
        result.youtube_status = Some(status.clone());
        self.save_auto_edit_result(&result)?;

        tracing::info!(
            "Updated YouTube status for result {}: {:?}",
            result_id,
            status.status
        );

        Ok(())
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|err| StorageError::Lock(err.to_string()))
    }

    fn all_clip_metadata(&self) -> Result<Vec<ClipMetadata>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT metadata_json FROM clips")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);

        rows.into_iter()
            .map(|json| serde_json::from_str(&json).map_err(StorageError::from))
            .collect()
    }

    fn list_legacy_game_dirs(&self) -> Result<Vec<String>> {
        let clips_dir = self.base_path.join("clips");

        if !clips_dir.exists() {
            return Ok(Vec::new());
        }

        let mut games = Vec::new();

        for entry in fs::read_dir(clips_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    games.push(name.to_string());
                }
            }
        }

        games.sort_by(|a, b| {
            let a_time = fs::metadata(self.game_path(a))
                .and_then(|m| m.modified())
                .ok();
            let b_time = fs::metadata(self.game_path(b))
                .and_then(|m| m.modified())
                .ok();
            b_time.cmp(&a_time)
        });

        Ok(games)
    }

    fn migrate_json_files_to_sqlite(&self) -> Result<()> {
        if self.migration_applied(JSON_TO_SQLITE_MIGRATION)? {
            return Ok(());
        }

        self.migrate_games_from_json()?;
        self.migrate_settings_from_json()?;
        self.migrate_auto_edit_usage_from_json()?;
        self.migrate_auto_edit_results_from_json()?;
        self.mark_migration_applied(JSON_TO_SQLITE_MIGRATION)?;
        Ok(())
    }

    fn migration_applied(&self, name: &str) -> Result<bool> {
        let exists: Option<String> = self
            .conn()?
            .query_row(
                "SELECT name FROM local_migrations WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(exists.is_some())
    }

    fn mark_migration_applied(&self, name: &str) -> Result<()> {
        self.conn()?.execute(
            "INSERT OR REPLACE INTO local_migrations (name, applied_at) VALUES (?1, ?2)",
            params![name, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    fn migrate_games_from_json(&self) -> Result<()> {
        let clips_dir = self.base_path.join("clips");
        if !clips_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(clips_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let Some(game_id) = entry.file_name().to_str().map(|value| value.to_string()) else {
                continue;
            };
            let game_path = entry.path();

            let metadata_path = game_path.join("metadata.json");
            if metadata_path.exists() {
                match read_json_file::<GameMetadata>(&metadata_path) {
                    Ok(metadata) => self.save_game_metadata(&game_id, &metadata)?,
                    Err(err) => tracing::warn!(
                        "Failed to migrate legacy game metadata {}: {}",
                        metadata_path.display(),
                        err
                    ),
                }
            }

            let events_path = game_path.join("events.json");
            if events_path.exists() {
                match read_json_file::<Vec<EventData>>(&events_path) {
                    Ok(events) => self.save_events(&game_id, &events)?,
                    Err(err) => tracing::warn!(
                        "Failed to migrate legacy game events {}: {}",
                        events_path.display(),
                        err
                    ),
                }
            }

            let clips_path = game_path.join("clips.json");
            if clips_path.exists() {
                match read_json_file::<Vec<ClipMetadata>>(&clips_path) {
                    Ok(clips) => {
                        for clip in clips {
                            self.save_clip_metadata(&game_id, &clip)?;
                        }
                    }
                    Err(err) => tracing::warn!(
                        "Failed to migrate legacy clips {}: {}",
                        clips_path.display(),
                        err
                    ),
                }
            }
        }

        Ok(())
    }

    fn migrate_settings_from_json(&self) -> Result<()> {
        let settings_path = self.base_path.join("settings.json");
        if !settings_path.exists() {
            return Ok(());
        }

        let settings: serde_json::Map<String, serde_json::Value> =
            match read_json_file(&settings_path) {
                Ok(settings) => settings,
                Err(err) => {
                    tracing::warn!(
                        "Failed to migrate legacy settings {}: {}",
                        settings_path.display(),
                        err
                    );
                    return Ok(());
                }
            };

        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn()?;
        for (key, value) in settings {
            let value = value
                .as_str()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| value.to_string());
            conn.execute(
                r#"
                INSERT INTO settings (key, value, updated_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(key) DO NOTHING
                "#,
                params![key, value, &now],
            )?;
        }

        Ok(())
    }

    fn migrate_auto_edit_usage_from_json(&self) -> Result<()> {
        let usage_path = self.base_path.join("auto_edit_usage.json");
        if !usage_path.exists() {
            return Ok(());
        }

        match read_json_file::<AutoEditUsage>(&usage_path) {
            Ok(usage) => self.save_auto_edit_usage(&usage)?,
            Err(err) => tracing::warn!(
                "Failed to migrate legacy auto-edit usage {}: {}",
                usage_path.display(),
                err
            ),
        }

        Ok(())
    }

    fn migrate_auto_edit_results_from_json(&self) -> Result<()> {
        let results_path = self.base_path.join("auto_edit_results.json");
        if !results_path.exists() {
            return Ok(());
        }

        match read_json_file::<Vec<AutoEditResultMetadata>>(&results_path) {
            Ok(results) => {
                for result in results {
                    self.save_auto_edit_result(&result)?;
                }
            }
            Err(err) => tracing::warn!(
                "Failed to migrate legacy auto-edit results {}: {}",
                results_path.display(),
                err
            ),
        }

        Ok(())
    }
}

fn validate_safe_delete_media_extension(file_path: &Path) -> Result<()> {
    let Some(extension) = file_path.extension().and_then(|ext| ext.to_str()) else {
        return Err(StorageError::Security(
            security::SecurityError::InvalidPath {
                reason: format!(
                    "Safe-delete target has no media extension: {}",
                    file_path.display()
                ),
            },
        ));
    };

    if !SAFE_DELETE_MEDIA_EXTENSIONS
        .iter()
        .any(|allowed| extension.eq_ignore_ascii_case(allowed))
    {
        return Err(StorageError::Security(
            security::SecurityError::InvalidExtension {
                ext: extension.to_lowercase(),
                allowed: SAFE_DELETE_MEDIA_EXTENSIONS
                    .iter()
                    .map(|ext| ext.to_string())
                    .collect(),
            },
        ));
    }

    Ok(())
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let json = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json)?)
}

/// Canvas template metadata for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasTemplateInfo {
    pub id: String,
    pub name: String,
    pub element_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_storage_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path());
        assert!(storage.is_ok());
        assert!(temp_dir.path().join("lolshorts.db").exists());
    }

    #[test]
    fn test_game_metadata() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let metadata = GameMetadata {
            game_id: "12345".to_string(),
            champion: "Yasuo".to_string(),
            game_mode: "Ranked".to_string(),
            start_time: Utc::now(),
            end_time: None,
            result: None,
            kda: None,
        };

        storage.save_game_metadata("12345", &metadata).unwrap();
        let loaded = storage.load_game_metadata("12345").unwrap();

        assert_eq!(loaded.game_id, "12345");
        assert_eq!(loaded.champion, "Yasuo");
        assert_eq!(storage.list_games().unwrap(), vec!["12345".to_string()]);
    }

    #[test]
    fn migrates_legacy_json_without_deleting_original_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let game_dir = temp_dir.path().join("clips").join("legacy-game");
        fs::create_dir_all(&game_dir).unwrap();

        let metadata = GameMetadata {
            game_id: "legacy-game".to_string(),
            champion: "Ahri".to_string(),
            game_mode: "Normal".to_string(),
            start_time: Utc::now(),
            end_time: None,
            result: None,
            kda: None,
        };
        fs::write(
            game_dir.join("metadata.json"),
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        let clip = ClipMetadata {
            file_path: temp_dir
                .path()
                .join("recordings")
                .join("legacy.mp4")
                .to_string_lossy()
                .to_string(),
            thumbnail_path: None,
            event_type: models::EventType::ChampionKill,
            event_time: 42.0,
            priority: 3,
            duration: 15.0,
            created_at: Utc::now(),
            usage_count: 0,
        };
        fs::write(
            game_dir.join("clips.json"),
            serde_json::to_string_pretty(&vec![clip.clone()]).unwrap(),
        )
        .unwrap();

        let storage = Storage::new(temp_dir.path()).unwrap();

        assert!(game_dir.join("metadata.json").exists());
        assert!(game_dir.join("clips.json").exists());
        assert_eq!(
            storage.load_game_metadata("legacy-game").unwrap().champion,
            "Ahri"
        );
        assert_eq!(storage.load_clip_metadata("legacy-game").unwrap().len(), 1);
    }

    #[test]
    fn json_to_sqlite_migration_is_idempotent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let game_dir = temp_dir.path().join("clips").join("game-1");
        fs::create_dir_all(&game_dir).unwrap();

        let metadata = GameMetadata {
            game_id: "game-1".to_string(),
            champion: "Lux".to_string(),
            game_mode: "Ranked".to_string(),
            start_time: Utc::now(),
            end_time: None,
            result: None,
            kda: None,
        };
        fs::write(
            game_dir.join("metadata.json"),
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        let first = Storage::new(temp_dir.path()).unwrap();
        assert_eq!(first.list_games().unwrap(), vec!["game-1".to_string()]);
        drop(first);

        let second = Storage::new(temp_dir.path()).unwrap();
        assert_eq!(second.list_games().unwrap(), vec!["game-1".to_string()]);
    }

    #[test]
    fn storage_health_check_reports_sqlite_integrity() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let health = storage.health_check().unwrap();

        assert!(health.integrity_ok);
        assert_eq!(health.integrity_message, "ok");
        assert!(health.database_path.ends_with("lolshorts.db"));
    }

    #[tokio::test]
    async fn diagnostic_setting_keys_never_return_secret_values() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        storage
            .set_setting("youtube_credentials_user", "super-secret-token")
            .await
            .unwrap();

        let keys = storage.diagnostic_setting_keys().unwrap();

        assert_eq!(keys, vec!["youtube_credentials_user:[redacted]"]);
        assert!(!keys.join(" ").contains("super-secret-token"));
    }

    #[test]
    fn test_safe_delete_media_file_deletes_inside_recordings_root() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let clip_path = temp_dir.path().join("recordings").join("clip.mp4");
        fs::write(&clip_path, "clip").unwrap();

        let outcome = storage
            .safe_delete_media_file(&clip_path)
            .expect("inside recordings root should delete");

        assert!(matches!(outcome, security::SafeDeleteOutcome::Deleted(_)));
        assert!(!clip_path.exists());
    }

    #[test]
    fn test_safe_delete_media_file_missing_inside_root_is_noop() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let clip_path = temp_dir.path().join("recordings").join("missing.mp4");

        let outcome = storage
            .safe_delete_media_file(&clip_path)
            .expect("missing file inside recordings root should be a no-op");

        assert!(matches!(outcome, security::SafeDeleteOutcome::Missing(_)));
    }

    #[test]
    fn test_safe_delete_media_file_rejects_outside_root() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let outside_dir = tempfile::tempdir().unwrap();
        let outside_file = outside_dir.path().join("clip.mp4");
        fs::write(&outside_file, "clip").unwrap();

        let result = storage.safe_delete_media_file(&outside_file);

        assert!(matches!(
            result.unwrap_err(),
            StorageError::Security(security::SecurityError::PathOutsideAllowedRoots { .. })
        ));
        assert!(outside_file.exists());
    }

    #[test]
    fn test_delete_auto_edit_result_deletes_inside_result_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let result_dir = std::env::temp_dir()
            .join("lolshorts_auto_edit")
            .join(format!(
                "storage_test_{}",
                Utc::now().timestamp_nanos_opt().unwrap()
            ));
        fs::create_dir_all(&result_dir).unwrap();
        let output_path = result_dir.join("result.mp4");
        let thumbnail_path = result_dir.join("result.jpg");
        fs::write(&output_path, "video").unwrap();
        fs::write(&thumbnail_path, "thumb").unwrap();

        storage
            .save_auto_edit_result(&AutoEditResultMetadata {
                result_id: "result_1".to_string(),
                job_id: "job_1".to_string(),
                output_path: output_path.to_string_lossy().to_string(),
                thumbnail_path: Some(thumbnail_path.to_string_lossy().to_string()),
                created_at: Utc::now(),
                duration: 10.0,
                clip_count: 1,
                game_ids: vec!["game_1".to_string()],
                target_duration: 60,
                canvas_template_name: None,
                has_background_music: false,
                youtube_status: Some(YouTubeUploadStatus {
                    video_id: None,
                    status: UploadStatus::NotUploaded,
                    upload_started_at: None,
                    upload_completed_at: None,
                    progress: 0.0,
                    error: None,
                }),
                file_size_bytes: 5,
            })
            .unwrap();

        storage.delete_auto_edit_result("result_1", true).unwrap();

        assert!(!output_path.exists());
        assert!(!thumbnail_path.exists());
        assert!(storage.load_auto_edit_result("result_1").is_err());
        let _ = fs::remove_dir_all(result_dir);
    }

    #[test]
    fn test_delete_auto_edit_result_rejects_outside_path_and_keeps_metadata() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let outside_dir = tempfile::tempdir().unwrap();
        let outside_file = outside_dir.path().join("result.mp4");
        fs::write(&outside_file, "video").unwrap();

        storage
            .save_auto_edit_result(&AutoEditResultMetadata {
                result_id: "result_2".to_string(),
                job_id: "job_2".to_string(),
                output_path: outside_file.to_string_lossy().to_string(),
                thumbnail_path: None,
                created_at: Utc::now(),
                duration: 10.0,
                clip_count: 1,
                game_ids: vec!["game_1".to_string()],
                target_duration: 60,
                canvas_template_name: None,
                has_background_music: false,
                youtube_status: None,
                file_size_bytes: 5,
            })
            .unwrap();

        let result = storage.delete_auto_edit_result("result_2", true);

        assert!(matches!(
            result.unwrap_err(),
            StorageError::Security(security::SecurityError::PathOutsideAllowedRoots { .. })
        ));
        assert!(outside_file.exists());
        assert!(storage.load_auto_edit_result("result_2").is_ok());
    }
}
