pub mod commands;
pub mod models;

use crate::utils::security;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use thiserror::Error;

// Re-export public types
pub use models::{
    AutoEditResultGroup, AutoEditResultMetadata, AutoEditUsage, ClipMetadata, ClipVaultGameGroup,
    ClipVaultPage, ClipVaultPageInput, ClipVaultSort, EventData, GameMetadata, MediaJobKind,
    MediaJobPart, MediaJobSnapshot, MediaJobStatus, PlatformExportMetadata, StorageStats,
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
const AUTO_EDIT_USAGE_USER_SCOPED_MIGRATION: &str = "auto_edit_usage_user_scoped_v1";

#[derive(Serialize, Deserialize)]
struct ClipVaultCursor {
    sort: ClipVaultSort,
    game_id: String,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    game_mode: Option<String>,
}

/// SQLite-backed local storage for app-owned metadata.
///
/// This database stores only local app data such as games, clips, app settings,
/// and auto-edit results. Authentication, billing, and PRO entitlement remain
/// authoritative in Supabase and must not be trusted from this local database.
///
/// Storage location note: this SQLite DB (and the `recordings/`, `clips/`,
/// `replays/`, `exports/` media directories) live under `dirs::data_dir()`
/// (`base_path`, set by `main.rs`). `RecordingSettings` (recording/audio/UI
/// preferences) is a *separate* JSON file under `dirs::config_dir()` --
/// see `settings::storage` module doc for why these two roots are kept
/// distinct rather than merged.
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

            -- Legacy single-row (id=1) quota counter, shared by every local
            -- user of the app. Superseded by auto_edit_usage_by_user below;
            -- kept only as a one-time migration read source (see
            -- migrate_auto_edit_usage_to_user_scoped). No code writes to this
            -- table anymore.
            CREATE TABLE IF NOT EXISTS auto_edit_usage (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                usage_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            -- Per-user auto-edit quota counters. user_id is the Supabase
            -- auth user id, or "anonymous"/"legacy" for unauthenticated /
            -- pre-migration usage. NOTE: this is a local CACHE / OFFLINE
            -- FALLBACK only and is NOT authoritative -- it can be reset by
            -- deleting/editing the local DB file. The authoritative counter
            -- lives server-side in Supabase (the `quota` edge function +
            -- public.auto_edit_usage); video::commands::start_auto_edit
            -- consults the server first and only falls back to this table when
            -- the server is unreachable (offline/timeout).
            CREATE TABLE IF NOT EXISTS auto_edit_usage_by_user (
                user_id TEXT PRIMARY KEY,
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

            CREATE TABLE IF NOT EXISTS media_jobs (
                job_id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                config_json TEXT NOT NULL,
                current_stage TEXT NOT NULL,
                progress_percentage REAL NOT NULL DEFAULT 0,
                error_code TEXT,
                error_message TEXT,
                retry_count INTEGER NOT NULL DEFAULT 0,
                quota_sync_pending INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_media_jobs_status_updated
                ON media_jobs(status, updated_at DESC);

            CREATE TABLE IF NOT EXISTS media_job_parts (
                job_id TEXT NOT NULL,
                part_index INTEGER NOT NULL,
                part_count INTEGER NOT NULL,
                status TEXT NOT NULL,
                progress_percentage REAL NOT NULL DEFAULT 0,
                trim_json TEXT NOT NULL,
                partial_path TEXT,
                output_path TEXT,
                validation_json TEXT,
                file_fingerprint TEXT,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(job_id, part_index),
                FOREIGN KEY(job_id) REFERENCES media_jobs(job_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS platform_exports (
                export_id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                result_id TEXT NOT NULL,
                preset TEXT NOT NULL,
                output_path TEXT NOT NULL,
                passthrough INTEGER NOT NULL,
                owns_file INTEGER NOT NULL,
                validation_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(result_id) REFERENCES auto_edit_results(result_id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_platform_exports_result
                ON platform_exports(result_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS quota_job_consumptions (
                user_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                month TEXT NOT NULL,
                server_synced INTEGER NOT NULL DEFAULT 0,
                consumed_at TEXT NOT NULL,
                PRIMARY KEY(user_id, job_id)
            );

            INSERT OR IGNORE INTO local_migrations(name, applied_at)
                VALUES('media_jobs_v1', CURRENT_TIMESTAMP);
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

        if let Err(err) = storage.migrate_auto_edit_usage_to_user_scoped() {
            tracing::warn!(
                "Failed to migrate legacy single-row auto-edit usage counter to the per-user table: {}",
                err
            );
        }

        if let Err(err) = storage.recover_interrupted_media_jobs() {
            tracing::warn!("Failed to mark interrupted media jobs recoverable: {}", err);
        }
        if let Err(err) = storage.expire_media_job_artifacts() {
            tracing::warn!("Failed to expire old recoverable media artifacts: {}", err);
        }

        // Clear rows left behind by older builds that persisted a `pending/<id>.mp4`
        // placeholder whenever clip extraction failed (see `is_ghost_clip_path`). The
        // retention sweep cannot reach them, so they would otherwise haunt the library
        // forever as unplayable entries.
        match storage.sweep_ghost_clip_metadata() {
            Ok(0) => {}
            Ok(count) => tracing::info!(
                "Startup sweep removed {} clip row(s) with an unusable placeholder path",
                count
            ),
            Err(err) => tracing::warn!("Startup ghost-clip sweep failed: {}", err),
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

    /// Legacy per-game directory (`base_path/clips/<game_id>`).
    ///
    /// NOTE: despite the name, this is **not** where clip media files live.
    /// It exists solely so `load_game_metadata`/`load_events`/
    /// `load_clip_metadata` can still read old `metadata.json`/`events.json`/
    /// `clips.json` sidecars from installs that predate the SQLite store.
    /// Real clip video files live in a flat layout at
    /// [`Self::recordings_clips_dir`]. This method must stay read-only-safe
    /// (callers must not assume the directory exists) -- `create_game`/
    /// `save_*` no longer eagerly create it.
    pub fn game_path(&self, game_id: &str) -> PathBuf {
        self.base_path.join("clips").join(game_id)
    }

    /// Directory holding extracted highlight-clip media files (flat, one
    /// level, not per-game). This is the real location `ClipMetadata::file_path`
    /// points into for clips created by the recording pipeline -- distinct
    /// from the legacy per-game [`Self::game_path`] directory.
    pub fn recordings_clips_dir(&self) -> PathBuf {
        self.base_path.join("recordings").join("clips")
    }

    /// Directory holding the rolling-buffer recording segments: segment
    /// mp4s, the WASAPI loopback WAV, and the ffmpeg concat list. Rewritten
    /// continuously while recording; safe to clear when idle.
    pub fn recordings_segments_dir(&self) -> PathBuf {
        self.base_path.join("recordings").join("segments")
    }

    /// Directory holding auto-edit render output (intermediate stages +
    /// final rendered Shorts + thumbnails), when `AutoComposer::set_output_root`
    /// has been wired to `base_path/exports/auto_edit` (see `main.rs`).
    pub fn exports_dir(&self) -> PathBuf {
        self.base_path.join("exports")
    }

    /// App-owned roots where media/result files may be deleted from metadata.
    pub fn safe_delete_roots(&self) -> Vec<PathBuf> {
        vec![
            self.base_path.join("clips"),
            self.base_path.join("recordings"),
            self.base_path.join("replays"),
            self.exports_dir(),
            // Legacy auto-edit output location, kept for installs that still
            // have results pointing at %TEMP% from before `exports/` existed.
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

    /// Create a new game metadata row.
    pub fn create_game(&self, game_id: &str, metadata: &GameMetadata) -> Result<()> {
        self.save_game_metadata(game_id, metadata)?;
        tracing::info!("Created game storage entry: {}", game_id);
        Ok(())
    }

    /// Save game metadata.
    pub fn save_game_metadata(&self, game_id: &str, metadata: &GameMetadata) -> Result<()> {
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

    /// Load a page of non-empty games for the clip vault without allowing one
    /// malformed metadata row to make the whole library unavailable.
    pub fn list_clip_vault_page(
        &self,
        sort: ClipVaultSort,
        cursor: Option<&str>,
        game_limit: usize,
        query: Option<&str>,
        game_mode: Option<&str>,
    ) -> Result<ClipVaultPage> {
        if !(1..=12).contains(&game_limit) {
            return Err(StorageError::Lock(
                "game_limit must be between 1 and 12".to_string(),
            ));
        }
        let query = normalize_clip_vault_query(query);
        let game_mode = normalize_clip_vault_game_mode(game_mode);
        let after_game_id = match cursor {
            Some(cursor) => {
                let bytes = URL_SAFE_NO_PAD
                    .decode(cursor)
                    .map_err(|_| StorageError::Lock("invalid clip vault cursor".to_string()))?;
                let decoded: ClipVaultCursor = serde_json::from_slice(&bytes)
                    .map_err(|_| StorageError::Lock("invalid clip vault cursor".to_string()))?;
                if decoded.sort != sort {
                    return Err(StorageError::Lock(
                        "clip vault cursor sort does not match request".to_string(),
                    ));
                }
                if decoded.query != query || decoded.game_mode != game_mode {
                    return Err(StorageError::Lock(
                        "clip vault cursor filters do not match request".to_string(),
                    ));
                }
                Some(decoded.game_id)
            }
            None => None,
        };

        let rows: Vec<(String, String, String)> = {
            let conn = self.conn()?;
            let mut stmt = conn.prepare(
                "SELECT g.game_id, g.metadata_json, c.metadata_json FROM games g JOIN clips c ON c.game_id = g.game_id",
            )?;
            let mapped = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            mapped
        };
        let mut skipped = 0;
        let mut corrupt_games = std::collections::HashSet::new();
        let mut grouped =
            std::collections::BTreeMap::<String, (Option<GameMetadata>, Vec<ClipMetadata>)>::new();
        for (game_id, game_json, clip_json) in rows {
            let game = match serde_json::from_str::<GameMetadata>(&game_json) {
                Ok(game) => game,
                Err(_) => {
                    if corrupt_games.insert(game_id.clone()) {
                        skipped += 1;
                    }
                    let entry = grouped.entry(game_id).or_insert_with(|| (None, Vec::new()));
                    match serde_json::from_str::<ClipMetadata>(&clip_json) {
                        Ok(clip) => entry.1.push(clip),
                        Err(_) => skipped += 1,
                    }
                    continue;
                }
            };
            let clip = match serde_json::from_str::<ClipMetadata>(&clip_json) {
                Ok(clip) => clip,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let entry = grouped
                .entry(game_id)
                .or_insert_with(|| (Some(game), Vec::new()));
            entry.1.push(clip);
        }
        let mut games: Vec<ClipVaultGameGroup> = grouped
            .into_iter()
            .filter(|(_, (_, clips))| !clips.is_empty())
            .filter(|(game_id, (game, _))| {
                clip_vault_game_matches_filters(
                    game_id,
                    game.as_ref(),
                    query.as_deref(),
                    game_mode.as_deref(),
                )
            })
            .map(|(game_id, (game, mut clips))| {
                clips.sort_by(|a, b| match sort {
                    ClipVaultSort::Best => clip_score(b)
                        .total_cmp(&clip_score(a))
                        .then_with(|| b.created_at.cmp(&a.created_at))
                        .then_with(|| a.file_path.cmp(&b.file_path)),
                    ClipVaultSort::Newest => b
                        .created_at
                        .cmp(&a.created_at)
                        .then_with(|| a.file_path.cmp(&b.file_path)),
                });
                ClipVaultGameGroup {
                    game_id,
                    game,
                    clip_count: clips.len(),
                    clips,
                }
            })
            .collect();
        games.sort_by(|a, b| match sort {
            ClipVaultSort::Best => game_best_score(b)
                .total_cmp(&game_best_score(a))
                .then_with(|| game_start_time(b).cmp(&game_start_time(a)))
                .then_with(|| a.game_id.cmp(&b.game_id)),
            ClipVaultSort::Newest => game_start_time(b)
                .cmp(&game_start_time(a))
                .then_with(|| a.game_id.cmp(&b.game_id)),
        });
        let start = after_game_id
            .and_then(|id| {
                games
                    .iter()
                    .position(|group| group.game_id == id)
                    .map(|i| i + 1)
            })
            .unwrap_or(0);
        let has_more = start.saturating_add(game_limit) < games.len();
        let page_games = games
            .into_iter()
            .skip(start)
            .take(game_limit)
            .collect::<Vec<_>>();
        let next_cursor = if has_more {
            page_games.last().map(|group| {
                URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(&ClipVaultCursor {
                        sort,
                        game_id: group.game_id.clone(),
                        query: query.clone(),
                        game_mode: game_mode.clone(),
                    })
                    .expect("cursor serialization is infallible"),
                )
            })
        } else {
            None
        };
        Ok(ClipVaultPage {
            groups: page_games,
            next_cursor,
            skipped_item_count: skipped,
        })
    }

    /// Load the exact clip row belonging to `game_id`.
    pub fn load_owned_clip_metadata(&self, game_id: &str, file_path: &str) -> Result<ClipMetadata> {
        let conn = self.conn()?;
        let json: String = conn
            .query_row(
                "SELECT metadata_json FROM clips WHERE game_id = ?1 AND file_path = ?2",
                params![game_id, file_path],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StorageError::GameNotFound(game_id.to_string()))?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Atomically persist only the thumbnail field from the current database row.
    pub fn update_owned_clip_thumbnail(
        &self,
        game_id: &str,
        file_path: &str,
        thumbnail_path: &str,
    ) -> Result<ClipMetadata> {
        let conn = self.conn()?;
        let json: String = conn
            .query_row(
                "SELECT metadata_json FROM clips WHERE game_id = ?1 AND file_path = ?2",
                params![game_id, file_path],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StorageError::GameNotFound(game_id.to_string()))?;
        let mut clip: ClipMetadata = serde_json::from_str(&json)?;
        clip.thumbnail_path = Some(thumbnail_path.to_string());
        let updated = serde_json::to_string(&clip)?;
        let changed = conn.execute("UPDATE clips SET metadata_json = ?1, updated_at = ?2 WHERE game_id = ?3 AND file_path = ?4", params![updated, chrono::Utc::now().to_rfc3339(), game_id, file_path])?;
        if changed != 1 {
            return Err(StorageError::GameNotFound(game_id.to_string()));
        }
        Ok(clip)
    }

    /// Load clip metadata for a game the way the editor should: like
    /// [`Storage::load_clip_metadata`], but any legacy row whose `duration`
    /// was never recorded (`duration <= 0.0`) is measured with ffprobe
    /// (reusing [`crate::video::processor::pipeline::VideoProcessor::get_duration`])
    /// and the result is written back to storage, once, so future loads
    /// don't need to re-probe.
    ///
    /// A `duration <= 0.0` made the editor's trim UI silently treat every
    /// trim on such a clip as a no-op (trim range clamped against a
    /// zero-length clip). This is best-effort: a clip whose file can no
    /// longer be probed (missing/corrupt) keeps its existing duration and
    /// is skipped rather than failing the whole list.
    pub async fn load_clip_metadata_with_duration_backfill(
        &self,
        game_id: &str,
    ) -> Result<Vec<ClipMetadata>> {
        let mut clips = self.load_clip_metadata(game_id)?;

        let needs_backfill: Vec<usize> = clips
            .iter()
            .enumerate()
            .filter(|(_, clip)| clip.duration <= 0.0)
            .map(|(index, _)| index)
            .collect();

        if needs_backfill.is_empty() {
            return Ok(clips);
        }

        let processor = crate::video::processor::pipeline::VideoProcessor::new_with_fallback();

        for index in needs_backfill {
            let file_path = clips[index].file_path.clone();
            match processor.get_duration(&file_path).await {
                Ok(measured) if measured > 0.0 => {
                    clips[index].duration = measured;
                    if let Err(e) = self.save_clip_metadata(game_id, &clips[index]) {
                        tracing::warn!(
                            "Failed to persist backfilled duration for clip {}: {}",
                            file_path,
                            e
                        );
                    } else {
                        tracing::info!(
                            "Backfilled duration for clip {}: {:.3}s",
                            file_path,
                            measured
                        );
                    }
                }
                Ok(non_positive) => {
                    tracing::debug!(
                        "ffprobe reported non-positive duration ({}) for clip {}, leaving as-is",
                        non_positive,
                        file_path
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to backfill duration for clip {} (file missing/corrupt?): {}",
                        file_path,
                        e
                    );
                }
            }
        }

        Ok(clips)
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

    /// Remove clip rows whose recorded path can never be backed by a real file.
    ///
    /// Complements the retention sweep in `utils::cleanup`: that one only removes rows
    /// whose file is *confirmed* missing, and its unmounted-volume guard (parent
    /// directory must exist) permanently skips a relative `pending/` path, so the ghost
    /// rows written by the old failed-extraction placeholder survived every cycle.
    ///
    /// Returns the number of rows removed.
    pub fn sweep_ghost_clip_metadata(&self) -> Result<usize> {
        let mut removed = 0usize;

        for (game_id, clip) in self.all_clip_metadata_with_game_id()? {
            if !is_ghost_clip_path(&clip.file_path) {
                continue;
            }

            match self.delete_clip_metadata(&game_id, &clip.file_path) {
                Ok(()) => removed += 1,
                Err(err) => tracing::warn!(
                    "Failed to remove ghost clip row {} (game {}): {}",
                    clip.file_path,
                    game_id,
                    err
                ),
            }
        }

        Ok(removed)
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

    /// Delete the most recently saved clip (media file + metadata row).
    ///
    /// Used by the "delete last clip" hotkey. Returns the deleted file path, or
    /// `None` if there were no clips to delete.
    pub fn delete_last_clip(&self) -> Result<Option<String>> {
        let row: Option<(String, String)> = {
            let conn = self.conn()?;
            conn.query_row(
                "SELECT game_id, file_path FROM clips ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
        };

        let (game_id, file_path) = match row {
            Some(row) => row,
            None => {
                tracing::info!("delete_last_clip: no clips to delete");
                return Ok(None);
            }
        };

        // Best-effort media deletion; a missing file should not block metadata cleanup.
        if let Err(e) = self.safe_delete_media_file(&file_path) {
            tracing::warn!(
                "delete_last_clip: failed to delete media file {}: {}",
                file_path,
                e
            );
        }

        self.delete_clip_metadata(&game_id, &file_path)?;
        tracing::info!("Deleted last clip: {} (game {})", file_path, game_id);
        Ok(Some(file_path))
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

        let recordings_dir_size_bytes = dir_size_bytes(&self.base_path.join("recordings"));
        let exports_dir_size_bytes = dir_size_bytes(&self.exports_dir());

        Ok(StorageStats {
            total_games,
            total_clips,
            total_size_bytes: total_size,
            recordings_dir_size_bytes,
            exports_dir_size_bytes,
            total_disk_usage_bytes: recordings_dir_size_bytes + exports_dir_size_bytes,
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
    //
    // NOTE: this is a *local cache / offline fallback* counter scoped by
    // `user_id` (the Supabase auth user id, or "anonymous" for unauthenticated
    // callers -- see video::commands::start_auto_edit). It is NOT authoritative:
    // a user who edits or deletes the local SQLite file can reset it. The
    // authority for the FREE-tier monthly quota is the server-side `quota` edge
    // function backed by public.auto_edit_usage. start_auto_edit checks/consumes
    // the server first and only uses these functions when the server is
    // unreachable (offline/timeout), keeping the on-device counter warm as a
    // cache so offline users are not blocked.

    /// Load auto-edit usage for current month, scoped to `user_id`.
    pub fn load_auto_edit_usage(&self, user_id: &str) -> Result<AutoEditUsage> {
        let json: Option<String> = self
            .conn()?
            .query_row(
                "SELECT usage_json FROM auto_edit_usage_by_user WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .optional()?;

        let mut usage = match json {
            Some(json) => serde_json::from_str(&json)?,
            None => AutoEditUsage::default(),
        };

        if !usage.is_current_month() {
            tracing::info!(
                "Resetting auto-edit usage for new month: {} -> {} (user: {})",
                usage.month,
                AutoEditUsage::current_month(),
                user_id
            );
            usage = AutoEditUsage::reset_for_month(AutoEditUsage::current_month());
            self.save_auto_edit_usage(user_id, &usage)?;
        }

        Ok(usage)
    }

    /// Save auto-edit usage for `user_id`.
    fn save_auto_edit_usage(&self, user_id: &str, usage: &AutoEditUsage) -> Result<()> {
        let json = serde_json::to_string(usage)?;
        let now = chrono::Utc::now().to_rfc3339();
        self.conn()?.execute(
            r#"
            INSERT INTO auto_edit_usage_by_user (user_id, usage_json, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(user_id) DO UPDATE SET
                usage_json = excluded.usage_json,
                updated_at = excluded.updated_at
            "#,
            params![user_id, json, now],
        )?;

        tracing::debug!(
            "Saved auto-edit usage: user={}, month={}, count={}",
            user_id,
            usage.month,
            usage.usage_count
        );
        Ok(())
    }

    /// Increment the local cache auto-edit usage counter for `user_id`.
    ///
    /// The authoritative consume happens server-side (`quota` edge function);
    /// this keeps the on-device cache in step so the offline fallback stays
    /// approximately correct.
    pub fn increment_auto_edit_usage(&self, user_id: &str) -> Result<u32> {
        let mut usage = self.load_auto_edit_usage(user_id)?;

        usage.usage_count += 1;
        usage.last_updated = chrono::Utc::now();

        self.save_auto_edit_usage(user_id, &usage)?;

        tracing::info!(
            "Auto-edit usage incremented: user={}, {}/{} (month: {})",
            user_id,
            usage.usage_count,
            "∞",
            usage.month
        );

        Ok(usage.usage_count)
    }

    /// Check if `user_id` can perform auto-edit based on the local cache quota.
    ///
    /// This is the OFFLINE FALLBACK path only: the authoritative check is the
    /// server `quota` edge function (see the module note above and
    /// video::commands::start_auto_edit). Returns the remaining count, or an
    /// error when the local counter says the FREE monthly limit is reached.
    pub fn check_auto_edit_quota(&self, user_id: &str, is_pro: bool) -> Result<u32> {
        if is_pro {
            return Ok(u32::MAX);
        }

        const FREE_TIER_LIMIT: u32 = 5;

        let usage = self.load_auto_edit_usage(user_id)?;

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
    // Durable media jobs and publication checkpoints
    // ========================================================================

    pub fn create_media_job(
        &self,
        job_id: &str,
        user_id: &str,
        kind: MediaJobKind,
        config_json: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn()?.execute(
            "INSERT INTO media_jobs(job_id,user_id,kind,status,config_json,current_stage,progress_percentage,created_at,updated_at) VALUES(?1,?2,?3,'queued',?4,'queued',0,?5,?5)",
            params![job_id, user_id, enum_db_value(kind)?, config_json, now],
        )?;
        Ok(())
    }

    pub fn initialize_media_job_parts(&self, job_id: &str, trims: &[String]) -> Result<()> {
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "DELETE FROM media_job_parts WHERE job_id = ?1",
            params![job_id],
        )?;
        let now = chrono::Utc::now().to_rfc3339();
        for (index, trim_json) in trims.iter().enumerate() {
            transaction.execute(
                "INSERT INTO media_job_parts(job_id,part_index,part_count,status,progress_percentage,trim_json,attempt_count,updated_at) VALUES(?1,?2,?3,'queued',0,?4,0,?5)",
                params![job_id, index + 1, trims.len(), trim_json, now],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn update_media_job_status(
        &self,
        job_id: &str,
        next: MediaJobStatus,
        stage: &str,
        progress: f64,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let current = self.load_media_job(job_id)?.status;
        if !current.can_transition_to(next) {
            return Err(StorageError::Lock(format!(
                "invalid media job transition: {:?} -> {:?}",
                current, next
            )));
        }
        let changed = self.conn()?.execute(
            "UPDATE media_jobs SET status=?1,current_stage=?2,progress_percentage=?3,error_code=?4,error_message=?5,updated_at=?6 WHERE job_id=?7",
            params![enum_db_value(next)?, stage, progress.clamp(0.0, 100.0), error_code, error_message, chrono::Utc::now().to_rfc3339(), job_id],
        )?;
        if changed == 0 {
            return Err(StorageError::Lock(format!("media job not found: {job_id}")));
        }
        Ok(())
    }

    pub fn update_media_job_part(&self, job_id: &str, part: &MediaJobPart) -> Result<()> {
        let validation_json = part
            .validation
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let changed = self.conn()?.execute(
            r#"UPDATE media_job_parts SET status=?1,progress_percentage=?2,trim_json=?3,
               partial_path=?4,output_path=?5,validation_json=?6,file_fingerprint=?7,
               attempt_count=?8,updated_at=?9 WHERE job_id=?10 AND part_index=?11"#,
            params![
                enum_db_value(part.status)?,
                part.progress_percentage.clamp(0.0, 100.0),
                part.trim_json,
                part.partial_path,
                part.output_path,
                validation_json,
                part.file_fingerprint,
                part.attempt_count,
                chrono::Utc::now().to_rfc3339(),
                job_id,
                part.part_index,
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Lock(format!(
                "media job part not found: {job_id}/{}",
                part.part_index
            )));
        }
        Ok(())
    }

    pub fn load_media_job(&self, job_id: &str) -> Result<MediaJobSnapshot> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT user_id,kind,status,config_json,current_stage,progress_percentage,error_code,error_message,retry_count,quota_sync_pending,created_at,updated_at FROM media_jobs WHERE job_id=?1",
                params![job_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, f64>(5)?,
                        row.get::<_, Option<String>>(6)?, row.get::<_, Option<String>>(7)?,
                        row.get::<_, u32>(8)?, row.get::<_, bool>(9)?, row.get::<_, String>(10)?,
                        row.get::<_, String>(11)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| StorageError::Lock(format!("media job not found: {job_id}")))?;
        let mut statement = conn.prepare(
            "SELECT part_index,part_count,status,progress_percentage,trim_json,partial_path,output_path,validation_json,file_fingerprint,attempt_count FROM media_job_parts WHERE job_id=?1 ORDER BY part_index",
        )?;
        let parts = statement
            .query_map(params![job_id], |row| {
                let status: String = row.get(2)?;
                let validation: Option<String> = row.get(7)?;
                Ok((
                    row.get::<_, usize>(0)?,
                    row.get::<_, usize>(1)?,
                    status,
                    row.get::<_, f64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    validation,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, u32>(9)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(|part| {
                Ok(MediaJobPart {
                    part_index: part.0,
                    part_count: part.1,
                    status: enum_from_db(&part.2)?,
                    progress_percentage: part.3,
                    trim_json: part.4,
                    partial_path: part.5,
                    output_path: part.6,
                    validation: part
                        .7
                        .map(|value| serde_json::from_str(&value))
                        .transpose()?,
                    file_fingerprint: part.8,
                    attempt_count: part.9,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let status: MediaJobStatus = enum_from_db(&row.2)?;
        Ok(MediaJobSnapshot {
            job_id: job_id.to_string(),
            user_id: row.0,
            kind: enum_from_db(&row.1)?,
            status,
            recoverable: status.is_recoverable(),
            config_json: row.3,
            current_stage: row.4,
            progress_percentage: row.5,
            parts,
            error_code: row.6,
            error_message: row.7,
            retry_count: row.8,
            quota_sync_pending: row.9,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.10)
                .map_err(|e| StorageError::Lock(e.to_string()))?
                .with_timezone(&chrono::Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.11)
                .map_err(|e| StorageError::Lock(e.to_string()))?
                .with_timezone(&chrono::Utc),
        })
    }

    pub fn list_recoverable_media_jobs(&self, user_id: &str) -> Result<Vec<MediaJobSnapshot>> {
        let ids = {
            let conn = self.conn()?;
            let mut statement = conn.prepare(
                "SELECT job_id FROM media_jobs WHERE user_id=?1 AND status IN ('paused','recoverable') ORDER BY updated_at DESC",
            )?;
            let rows = statement
                .query_map(params![user_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        ids.iter().map(|id| self.load_media_job(id)).collect()
    }

    /// Return whether any durable media job can still spawn or is currently
    /// running. Updater installation must wait for these jobs; checking only
    /// the legacy in-memory composer misses jobs resumed from SQLite or a
    /// platform-export task.
    pub fn has_active_media_jobs(&self) -> Result<bool> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM media_jobs WHERE status IN ('queued','running','validating'))",
            [],
            |row| row.get(0),
        )
        .map_err(StorageError::from)
    }

    fn recover_interrupted_media_jobs(&self) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn()?.execute(
            "UPDATE media_jobs SET status='recoverable',current_stage='interrupted',updated_at=?1 WHERE status IN ('running','validating')",
            params![now],
        )?;
        self.conn()?.execute(
            "UPDATE media_job_parts SET status='recoverable',updated_at=?1 WHERE status IN ('running','validating')",
            params![now],
        )?;
        Ok(())
    }

    fn expire_media_job_artifacts(&self) -> Result<()> {
        let artifact_cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let record_cutoff = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let ids = {
            let conn = self.conn()?;
            let mut statement = conn.prepare(
                "SELECT job_id FROM media_jobs WHERE status IN ('paused','recoverable','failed') AND updated_at < ?1",
            )?;
            let rows = statement
                .query_map(params![artifact_cutoff], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let internal_root = self
            .base_path
            .join("exports")
            .join("auto_edit")
            .join("intermediate");
        let canonical_root = internal_root.canonicalize().ok();
        for job_id in ids {
            if let Ok(snapshot) = self.load_media_job(&job_id) {
                for part in snapshot.parts {
                    for value in [part.partial_path, part.output_path].into_iter().flatten() {
                        let path = PathBuf::from(value);
                        if let (Some(root), Ok(candidate)) = (&canonical_root, path.canonicalize())
                        {
                            if candidate.starts_with(root) && candidate.is_file() {
                                let _ = fs::remove_file(candidate);
                            }
                        }
                    }
                }
                let job_dir = internal_root.join(&job_id);
                if job_dir.is_dir() {
                    let _ = fs::remove_dir_all(job_dir);
                }
                let _ = self.update_media_job_status(
                    &job_id,
                    MediaJobStatus::Discarded,
                    "expired",
                    snapshot.progress_percentage,
                    Some("artifact_expired"),
                    Some("Recoverable artifacts expired after 7 days"),
                );
            }
        }
        self.conn()?.execute(
            "DELETE FROM media_jobs WHERE status='discarded' AND updated_at < ?1",
            params![record_cutoff],
        )?;
        Ok(())
    }

    pub fn publish_auto_edit_series(
        &self,
        job_id: &str,
        user_id: &str,
        is_pro: bool,
        server_synced: bool,
        results: &[AutoEditResultMetadata],
        clips: &[(String, String)],
    ) -> Result<bool> {
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let status: String = transaction.query_row(
            "SELECT status FROM media_jobs WHERE job_id=?1",
            params![job_id],
            |row| row.get(0),
        )?;
        if status == "complete" {
            return Ok(false);
        }
        let now = chrono::Utc::now().to_rfc3339();
        for result in results {
            let json = serde_json::to_string(result)?;
            transaction.execute(
                "INSERT INTO auto_edit_results(result_id,metadata_json,output_path,created_at,updated_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(result_id) DO UPDATE SET metadata_json=excluded.metadata_json,output_path=excluded.output_path,updated_at=excluded.updated_at",
                params![result.result_id, json, result.output_path, result.created_at.to_rfc3339(), now],
            )?;
        }
        for (game_id, file_path) in clips {
            let json: Option<String> = transaction
                .query_row(
                    "SELECT metadata_json FROM clips WHERE game_id=?1 AND file_path=?2",
                    params![game_id, file_path],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(json) = json {
                let mut clip: ClipMetadata = serde_json::from_str(&json)?;
                clip.usage_count = clip.usage_count.saturating_add(1);
                transaction.execute(
                    "UPDATE clips SET metadata_json=?1,updated_at=?2 WHERE game_id=?3 AND file_path=?4",
                    params![serde_json::to_string(&clip)?, now, game_id, file_path],
                )?;
            }
        }
        let mut quota_pending = false;
        if !is_pro {
            let inserted = transaction.execute(
                "INSERT OR IGNORE INTO quota_job_consumptions(user_id,job_id,month,server_synced,consumed_at) VALUES(?1,?2,?3,?4,?5)",
                params![user_id, job_id, AutoEditUsage::current_month(), server_synced, now],
            )?;
            if inserted > 0 {
                let existing: Option<String> = transaction
                    .query_row(
                        "SELECT usage_json FROM auto_edit_usage_by_user WHERE user_id=?1",
                        params![user_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let mut usage: AutoEditUsage = existing
                    .map(|json| serde_json::from_str(&json))
                    .transpose()?
                    .unwrap_or_default();
                if !usage.is_current_month() {
                    usage = AutoEditUsage::reset_for_month(AutoEditUsage::current_month());
                }
                usage.usage_count = usage.usage_count.saturating_add(1);
                usage.last_updated = chrono::Utc::now();
                transaction.execute(
                    "INSERT INTO auto_edit_usage_by_user(user_id,usage_json,updated_at) VALUES(?1,?2,?3) ON CONFLICT(user_id) DO UPDATE SET usage_json=excluded.usage_json,updated_at=excluded.updated_at",
                    params![user_id, serde_json::to_string(&usage)?, now],
                )?;
            }
            quota_pending = !server_synced;
        }
        transaction.execute(
            "UPDATE media_jobs SET status='complete',current_stage='complete',progress_percentage=100,quota_sync_pending=?1,error_code=NULL,error_message=NULL,updated_at=?2 WHERE job_id=?3",
            params![quota_pending, now, job_id],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn pending_quota_job_ids(&self, user_id: &str) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let mut statement = conn.prepare(
            "SELECT job_id FROM quota_job_consumptions WHERE user_id=?1 AND server_synced=0 ORDER BY consumed_at",
        )?;
        let rows = statement
            .query_map(params![user_id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn mark_quota_job_synced(&self, user_id: &str, job_id: &str) -> Result<()> {
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "UPDATE quota_job_consumptions SET server_synced=1 WHERE user_id=?1 AND job_id=?2",
            params![user_id, job_id],
        )?;
        transaction.execute(
            "UPDATE media_jobs SET quota_sync_pending=0,updated_at=?1 WHERE job_id=?2",
            params![chrono::Utc::now().to_rfc3339(), job_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn save_platform_export(&self, export: &PlatformExportMetadata) -> Result<()> {
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        let validation_json = serde_json::to_string(&export.validation)?;
        transaction.execute(
            "INSERT INTO platform_exports(export_id,job_id,result_id,preset,output_path,passthrough,owns_file,validation_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![export.export_id, export.job_id, export.result_id, enum_db_value(export.preset)?, export.output_path, export.passthrough, export.owns_file, validation_json, export.created_at.to_rfc3339()],
        )?;
        let json: String = transaction.query_row(
            "SELECT metadata_json FROM auto_edit_results WHERE result_id=?1",
            params![export.result_id],
            |row| row.get(0),
        )?;
        let mut result: AutoEditResultMetadata = serde_json::from_str(&json)?;
        result
            .platform_exports
            .retain(|item| item.preset != export.preset);
        result.platform_exports.push(export.clone());
        transaction.execute(
            "UPDATE auto_edit_results SET metadata_json=?1,updated_at=?2 WHERE result_id=?3",
            params![
                serde_json::to_string(&result)?,
                chrono::Utc::now().to_rfc3339(),
                export.result_id
            ],
        )?;
        transaction.commit()?;
        Ok(())
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

    pub fn load_auto_edit_result_groups(&self) -> Result<Vec<AutoEditResultGroup>> {
        use crate::video::auto_composer::AutoEditOutputIntent;
        use crate::video::output_validation::OutputValidationStatus;
        let mut grouped = std::collections::BTreeMap::<String, Vec<AutoEditResultMetadata>>::new();
        for result in self.load_auto_edit_results()? {
            let key = if result.series_id.is_empty() {
                result.result_id.clone()
            } else {
                result.series_id.clone()
            };
            grouped.entry(key).or_default().push(result);
        }
        let mut groups = Vec::with_capacity(grouped.len());
        for (series_id, mut outputs) in grouped {
            outputs.sort_by_key(|result| result.part_index);
            let output_intent = outputs
                .first()
                .and_then(|result| enum_from_db::<AutoEditOutputIntent>(&result.output_intent).ok())
                .unwrap_or_default();
            let validation_status =
                outputs
                    .iter()
                    .fold(OutputValidationStatus::Valid, |state, result| {
                        let next = result
                            .validation
                            .as_ref()
                            .map(|report| report.status)
                            .unwrap_or(OutputValidationStatus::Unknown);
                        match (state, next) {
                            (OutputValidationStatus::Invalid, _)
                            | (_, OutputValidationStatus::Invalid) => {
                                OutputValidationStatus::Invalid
                            }
                            (OutputValidationStatus::Unknown, _)
                            | (_, OutputValidationStatus::Unknown) => {
                                OutputValidationStatus::Unknown
                            }
                            (OutputValidationStatus::Warning, _)
                            | (_, OutputValidationStatus::Warning) => {
                                OutputValidationStatus::Warning
                            }
                            _ => OutputValidationStatus::Valid,
                        }
                    });
            groups.push(AutoEditResultGroup {
                series_id,
                job_id: outputs
                    .first()
                    .map(|result| result.job_id.clone())
                    .unwrap_or_default(),
                output_intent,
                total_duration: outputs.iter().map(|result| result.duration).sum(),
                total_file_size_bytes: outputs.iter().map(|result| result.file_size_bytes).sum(),
                validation_status,
                outputs,
            });
        }
        groups.sort_by(|left, right| {
            right
                .outputs
                .first()
                .map(|result| result.created_at)
                .cmp(&left.outputs.first().map(|result| result.created_at))
        });
        Ok(groups)
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

    pub fn delete_auto_edit_result_group(&self, series_id: &str, delete_files: bool) -> Result<()> {
        let group = self
            .load_auto_edit_result_groups()?
            .into_iter()
            .find(|group| group.series_id == series_id)
            .ok_or_else(|| {
                StorageError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Auto-edit result group not found: {series_id}"),
                ))
            })?;
        if delete_files {
            for result in &group.outputs {
                self.safe_delete_media_file(&result.output_path)?;
                if let Some(thumbnail) = &result.thumbnail_path {
                    self.safe_delete_media_file(thumbnail)?;
                }
                for export in result
                    .platform_exports
                    .iter()
                    .filter(|export| export.owns_file)
                {
                    self.safe_delete_media_file(&export.output_path)?;
                }
            }
        }
        let mut conn = self.conn()?;
        let transaction = conn.transaction()?;
        for result in &group.outputs {
            transaction.execute(
                "DELETE FROM platform_exports WHERE result_id=?1",
                params![result.result_id],
            )?;
            transaction.execute(
                "DELETE FROM auto_edit_results WHERE result_id=?1",
                params![result.result_id],
            )?;
        }
        transaction.commit()?;
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

    /// All clip metadata rows across every game, paired with their owning
    /// `game_id`. Used by maintenance/retention tooling (see
    /// `utils::cleanup::CleanupManager::run_retention_cycle`) that needs the
    /// `game_id` to call [`Self::delete_clip_metadata`] on a specific row.
    pub fn all_clip_metadata_with_game_id(&self) -> Result<Vec<(String, ClipMetadata)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT game_id, metadata_json FROM clips")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);

        rows.into_iter()
            .map(|(game_id, json)| {
                serde_json::from_str::<ClipMetadata>(&json)
                    .map(|clip| (game_id, clip))
                    .map_err(StorageError::from)
            })
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
            // Pre-multi-user JSON file predates any user_id concept; attribute
            // it to the "legacy" pseudo-user (see migrate_auto_edit_usage_to_user_scoped).
            Ok(usage) => self.save_auto_edit_usage("legacy", &usage)?,
            Err(err) => tracing::warn!(
                "Failed to migrate legacy auto-edit usage {}: {}",
                usage_path.display(),
                err
            ),
        }

        Ok(())
    }

    /// One-time migration: copy the old single-row (`id=1`) `auto_edit_usage`
    /// counter -- shared by every local user on installs from before
    /// per-user quota scoping existed -- into `auto_edit_usage_by_user` under
    /// the "legacy" pseudo-user. Idempotent via the `local_migrations` marker;
    /// safe to call on every startup.
    fn migrate_auto_edit_usage_to_user_scoped(&self) -> Result<()> {
        if self.migration_applied(AUTO_EDIT_USAGE_USER_SCOPED_MIGRATION)? {
            return Ok(());
        }

        let legacy_json: Option<String> = self
            .conn()?
            .query_row(
                "SELECT usage_json FROM auto_edit_usage WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(json) = legacy_json {
            let now = chrono::Utc::now().to_rfc3339();
            self.conn()?.execute(
                r#"
                INSERT INTO auto_edit_usage_by_user (user_id, usage_json, updated_at)
                VALUES ('legacy', ?1, ?2)
                ON CONFLICT(user_id) DO NOTHING
                "#,
                params![json, now],
            )?;
            tracing::info!(
                "Migrated legacy single-row auto-edit usage counter into per-user table (user_id=\"legacy\")"
            );
        }

        self.mark_migration_applied(AUTO_EDIT_USAGE_USER_SCOPED_MIGRATION)?;
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

fn clip_score(clip: &ClipMetadata) -> f64 {
    clip.highlight_score
        .filter(|score| score.is_finite())
        .unwrap_or((clip.priority as f64) * 20.0)
}

fn normalize_clip_vault_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(|query| query.to_lowercase())
}

fn normalize_clip_vault_game_mode(game_mode: Option<&str>) -> Option<String> {
    game_mode
        .map(str::trim)
        .filter(|game_mode| !game_mode.is_empty())
        .map(str::to_owned)
}

fn clip_vault_game_matches_filters(
    game_id: &str,
    game: Option<&GameMetadata>,
    query: Option<&str>,
    game_mode: Option<&str>,
) -> bool {
    let Some(game) = game else {
        return query.is_none() && game_mode.is_none();
    };

    if let Some(game_mode) = game_mode {
        if game.game_mode != game_mode {
            return false;
        }
    }

    query.is_none_or(|query| {
        game_id.to_lowercase().contains(query)
            || game.champion.to_lowercase().contains(query)
            || game.game_mode.to_lowercase().contains(query)
    })
}

fn game_best_score(game: &ClipVaultGameGroup) -> f64 {
    game.clips
        .iter()
        .map(clip_score)
        .max_by(f64::total_cmp)
        .unwrap_or(f64::NEG_INFINITY)
}

fn game_start_time(game: &ClipVaultGameGroup) -> chrono::DateTime<chrono::Utc> {
    game.game
        .as_ref()
        .map(|metadata| metadata.start_time)
        .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC)
}

/// Select the event frame only when it is a finite position inside the clip.
pub(crate) fn thumbnail_offset_secs(clip: &ClipMetadata) -> f64 {
    if let Some(offset) = clip
        .event_offset_secs
        .filter(|offset| offset.is_finite() && *offset >= 0.0 && *offset <= clip.duration)
    {
        offset
    } else if clip.duration.is_finite() && clip.duration > 0.0 {
        clip.duration / 2.0
    } else {
        0.0
    }
}

pub(crate) fn thumbnail_output_path(input: &Path) -> Result<PathBuf> {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| StorageError::Lock("clip path must have a valid file name".to_string()))?;
    Ok(input.with_file_name(format!("{}_thumbnail.jpg", stem)))
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let json = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json)?)
}

fn enum_db_value<T: Serialize>(value: T) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| StorageError::Lock("enum did not serialize as a string".to_string()))
}

fn enum_from_db<T: DeserializeOwned>(value: &str) -> Result<T> {
    Ok(serde_json::from_value(serde_json::Value::String(
        value.to_string(),
    ))?)
}

/// Whether a recorded clip path can never resolve to a real file.
///
/// `AutoClipManager` used to persist a placeholder row (`pending/<clip_id>.mp4`) every
/// time clip extraction failed. Those rows are pure ghosts: the path is relative, so it
/// does not even denote a stable location, and nothing ever creates the file. Real clips
/// are always written to an absolute path under the recorder's output directory, so a
/// missing relative path (or a missing `pending/` placeholder) is unambiguously a ghost.
///
/// An existing file is never a ghost, whatever its shape — this must not delete rows for
/// clips a user can still open.
fn is_ghost_clip_path(file_path: &str) -> bool {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return true;
    }

    let path = Path::new(trimmed);
    if path.exists() {
        return false;
    }

    if !path.is_absolute() {
        return true;
    }

    path.components()
        .any(|component| component.as_os_str() == "pending")
}

/// Recursively sum file sizes under `root`. Missing directories and
/// unreadable entries are treated as zero rather than an error -- this is a
/// best-effort disk-usage figure for dashboard display, not a correctness
/// boundary.
pub(crate) fn dir_size_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => stack.push(path),
                Ok(file_type) if file_type.is_file() => {
                    total += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                }
                _ => {}
            }
        }
    }

    total
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
            event_offset_secs: None,
            created_at: Utc::now(),
            usage_count: 0,
            highlight_score: None,
            score_reasons: Vec::new(),
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
                publish_title: String::new(),
                publish_description: String::new(),
                publish_tags: Vec::new(),
                publish_privacy_status: "unlisted".to_string(),
                output_intent: String::new(),
                framing_mode: String::new(),
                platform_preset: String::new(),
                series_id: String::new(),
                part_index: 1,
                part_count: 1,
                output_kind: String::new(),
                validation: None,
                platform_exports: Vec::new(),
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
                publish_title: String::new(),
                publish_description: String::new(),
                publish_tags: Vec::new(),
                publish_privacy_status: "unlisted".to_string(),
                output_intent: String::new(),
                framing_mode: String::new(),
                platform_preset: String::new(),
                series_id: String::new(),
                part_index: 1,
                part_count: 1,
                output_kind: String::new(),
                validation: None,
                platform_exports: Vec::new(),
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

    #[test]
    fn save_game_metadata_does_not_eagerly_create_legacy_game_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let metadata = GameMetadata {
            game_id: "no-dir-game".to_string(),
            champion: "Zed".to_string(),
            game_mode: "Ranked".to_string(),
            start_time: Utc::now(),
            end_time: None,
            result: None,
            kda: None,
        };

        storage.create_game("no-dir-game", &metadata).unwrap();
        storage.save_events("no-dir-game", &[]).unwrap();
        storage
            .save_clip_metadata(
                "no-dir-game",
                &ClipMetadata {
                    file_path: storage
                        .recordings_clips_dir()
                        .join("clip.mp4")
                        .to_string_lossy()
                        .to_string(),
                    thumbnail_path: None,
                    event_type: models::EventType::ChampionKill,
                    event_time: 1.0,
                    priority: 1,
                    duration: 10.0,
                    event_offset_secs: None,
                    created_at: Utc::now(),
                    usage_count: 0,
                    highlight_score: None,
                    score_reasons: Vec::new(),
                },
            )
            .unwrap();

        assert!(
            !storage.game_path("no-dir-game").exists(),
            "legacy per-game directory should not be eagerly created by create_game/save_*"
        );

        // Reads still work fully via SQLite despite the directory never existing.
        assert_eq!(
            storage.load_game_metadata("no-dir-game").unwrap().champion,
            "Zed"
        );
        assert_eq!(storage.load_clip_metadata("no-dir-game").unwrap().len(), 1);
    }

    // ---- load_clip_metadata_with_duration_backfill ----

    #[tokio::test]
    async fn duration_backfill_skips_clips_that_already_have_a_positive_duration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let clip = ClipMetadata {
            file_path: storage
                .recordings_clips_dir()
                .join("already-measured.mp4")
                .to_string_lossy()
                .to_string(),
            thumbnail_path: None,
            event_type: models::EventType::ChampionKill,
            event_time: 1.0,
            priority: 1,
            duration: 12.5,
            event_offset_secs: None,
            created_at: Utc::now(),
            usage_count: 0,
            highlight_score: None,
            score_reasons: Vec::new(),
        };
        storage.save_clip_metadata("dur-game", &clip).unwrap();

        let clips = storage
            .load_clip_metadata_with_duration_backfill("dur-game")
            .await
            .unwrap();

        assert_eq!(clips.len(), 1);
        // Unchanged: no ffprobe should even be attempted for a clip that
        // already has a recorded duration.
        assert_eq!(clips[0].duration, 12.5);
    }

    #[tokio::test]
    async fn duration_backfill_leaves_legacy_zero_duration_clips_unchanged_when_file_is_missing() {
        // Regression guard for the "editor trim silently ignored on legacy
        // clips" bug: a `duration <= 0.0` row must be *probed*, not just
        // passed through -- but when the backing file can't be probed
        // (missing here), the clip must survive with its existing duration
        // rather than the whole list load failing.
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let clip = ClipMetadata {
            file_path: storage
                .recordings_clips_dir()
                .join("legacy-missing-file.mp4")
                .to_string_lossy()
                .to_string(),
            thumbnail_path: None,
            event_type: models::EventType::ChampionKill,
            event_time: 1.0,
            priority: 1,
            duration: 0.0, // legacy row: duration was never recorded
            event_offset_secs: None,
            created_at: Utc::now(),
            usage_count: 0,
            highlight_score: None,
            score_reasons: Vec::new(),
        };
        storage
            .save_clip_metadata("dur-game-missing", &clip)
            .unwrap();

        let clips = storage
            .load_clip_metadata_with_duration_backfill("dur-game-missing")
            .await
            .unwrap();

        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].duration, 0.0);

        // The (unchanged) duration was not rewritten to storage either.
        let reloaded = storage.load_clip_metadata("dur-game-missing").unwrap();
        assert_eq!(reloaded[0].duration, 0.0);
    }

    #[tokio::test]
    async fn duration_backfill_measures_and_persists_duration_for_a_real_file() {
        // End-to-end happy path: requires a real `ffprobe` on PATH (or
        // bundled), so skip gracefully in environments without one instead
        // of failing the build.
        if crate::utils::ffmpeg::get_ffprobe_path().is_err() {
            eprintln!("skipping: ffprobe not available in this environment");
            return;
        }
        let Ok(ffmpeg_path) = crate::utils::ffmpeg::get_ffmpeg_path() else {
            eprintln!("skipping: ffmpeg not available in this environment");
            return;
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let clip_path = storage.recordings_clips_dir().join("real-clip.mp4");
        std::fs::create_dir_all(clip_path.parent().unwrap()).unwrap();

        // Synthesize a tiny 1-second test video with real ffmpeg.
        let status = std::process::Command::new(&ffmpeg_path)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x64:d=1",
                "-t",
                "1",
                clip_path.to_str().unwrap(),
            ])
            .status();
        let Ok(status) = status else {
            eprintln!("skipping: failed to invoke ffmpeg to synthesize test clip");
            return;
        };
        if !status.success() {
            eprintln!("skipping: ffmpeg failed to synthesize test clip");
            return;
        }

        let clip = ClipMetadata {
            file_path: clip_path.to_string_lossy().to_string(),
            thumbnail_path: None,
            event_type: models::EventType::ChampionKill,
            event_time: 1.0,
            priority: 1,
            duration: 0.0, // legacy row: duration was never recorded
            event_offset_secs: None,
            created_at: Utc::now(),
            usage_count: 0,
            highlight_score: None,
            score_reasons: Vec::new(),
        };
        storage.save_clip_metadata("dur-game-real", &clip).unwrap();

        let clips = storage
            .load_clip_metadata_with_duration_backfill("dur-game-real")
            .await
            .unwrap();

        assert_eq!(clips.len(), 1);
        assert!(
            clips[0].duration > 0.0,
            "expected ffprobe to measure a positive duration, got {}",
            clips[0].duration
        );

        // Persisted (write-back), so a plain reload sees it too.
        let reloaded = storage.load_clip_metadata("dur-game-real").unwrap();
        assert!(reloaded[0].duration > 0.0);
    }

    #[test]
    fn get_stats_reports_recordings_and_exports_disk_usage() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let segments_dir = storage.recordings_segments_dir();
        fs::create_dir_all(&segments_dir).unwrap();
        fs::write(segments_dir.join("segment_000.mp4"), vec![0u8; 1024]).unwrap();

        let exports_dir = storage.exports_dir().join("auto_edit");
        fs::create_dir_all(&exports_dir).unwrap();
        fs::write(exports_dir.join("final.mp4"), vec![0u8; 2048]).unwrap();

        let stats = storage.get_stats().unwrap();

        assert_eq!(stats.recordings_dir_size_bytes, 1024);
        assert_eq!(stats.exports_dir_size_bytes, 2048);
        assert_eq!(stats.total_disk_usage_bytes, 1024 + 2048);
        // Pre-existing field semantics unchanged: no clips in the DB, so 0.
        assert_eq!(stats.total_size_bytes, 0);
    }

    #[test]
    fn auto_edit_usage_is_scoped_per_user() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        storage.increment_auto_edit_usage("user-a").unwrap();
        storage.increment_auto_edit_usage("user-a").unwrap();
        storage.increment_auto_edit_usage("user-b").unwrap();

        assert_eq!(
            storage.load_auto_edit_usage("user-a").unwrap().usage_count,
            2
        );
        assert_eq!(
            storage.load_auto_edit_usage("user-b").unwrap().usage_count,
            1
        );
        assert_eq!(
            storage.load_auto_edit_usage("user-c").unwrap().usage_count,
            0
        );

        // Exhausting user-a's FREE-tier quota (5) must not affect user-b.
        for _ in 0..3 {
            storage.increment_auto_edit_usage("user-a").unwrap();
        }
        assert!(storage.check_auto_edit_quota("user-a", false).is_err());
        assert!(storage.check_auto_edit_quota("user-b", false).is_ok());
        // PRO is always unlimited regardless of accumulated usage.
        assert_eq!(
            storage.check_auto_edit_quota("user-a", true).unwrap(),
            u32::MAX
        );
    }

    #[test]
    fn legacy_single_row_auto_edit_usage_migrates_to_user_scoped_table() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("lolshorts.db");

        // Simulate an install that predates per-user quota scoping: seed the
        // legacy single-row auto_edit_usage table directly, before Storage
        // ever opens this path.
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS auto_edit_usage (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    usage_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                "#,
            )
            .unwrap();
            let usage = AutoEditUsage {
                month: AutoEditUsage::current_month(),
                usage_count: 3,
                last_updated: Utc::now(),
                period_start: Utc::now(),
            };
            conn.execute(
                "INSERT INTO auto_edit_usage (id, usage_json, updated_at) VALUES (1, ?1, ?2)",
                params![
                    serde_json::to_string(&usage).unwrap(),
                    Utc::now().to_rfc3339()
                ],
            )
            .unwrap();
        }

        let storage = Storage::new(temp_dir.path()).unwrap();

        let migrated = storage.load_auto_edit_usage("legacy").unwrap();
        assert_eq!(migrated.usage_count, 3);

        // A brand-new user is unaffected by the migrated legacy counter.
        let fresh = storage.load_auto_edit_usage("someone-else").unwrap();
        assert_eq!(fresh.usage_count, 0);
    }

    // ---- Ghost clip rows (failed-extraction placeholders) ----

    #[test]
    fn relative_and_pending_paths_are_ghosts_but_real_files_are_not() {
        let temp_dir = tempfile::tempdir().unwrap();
        let real_clip = temp_dir.path().join("real.mp4");
        fs::write(&real_clip, b"video").unwrap();

        // The exact shape the old failed-extraction placeholder produced.
        assert!(is_ghost_clip_path("pending/ChampionKill_412.mp4"));
        assert!(is_ghost_clip_path("clips/whatever.mp4"));
        assert!(is_ghost_clip_path("   "));
        assert!(is_ghost_clip_path(""));
        // Absolute, but under a `pending/` directory that was never created.
        assert!(is_ghost_clip_path(
            &temp_dir
                .path()
                .join("pending")
                .join("merged_1_2.mp4")
                .to_string_lossy()
        ));

        // A real file is never a ghost, and neither is a plain missing absolute path
        // (that one belongs to the retention sweep's unmounted-volume logic).
        assert!(!is_ghost_clip_path(&real_clip.to_string_lossy()));
        assert!(!is_ghost_clip_path(
            &temp_dir.path().join("gone.mp4").to_string_lossy()
        ));
    }

    #[test]
    fn sweep_ghost_clip_metadata_removes_only_placeholder_rows() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let game_id = "ghost-game";
        storage
            .create_game(
                game_id,
                &GameMetadata {
                    game_id: game_id.to_string(),
                    champion: "Ahri".to_string(),
                    game_mode: "CLASSIC".to_string(),
                    start_time: Utc::now(),
                    end_time: None,
                    result: None,
                    kda: None,
                },
            )
            .unwrap();

        let real_clip = temp_dir.path().join("real.mp4");
        fs::write(&real_clip, b"video").unwrap();

        let make_clip = |path: String| ClipMetadata {
            file_path: path,
            thumbnail_path: None,
            event_type: models::EventType::ChampionKill,
            event_time: 42.0,
            priority: 3,
            duration: 12.0,
            event_offset_secs: None,
            created_at: Utc::now(),
            usage_count: 0,
            highlight_score: None,
            score_reasons: Vec::new(),
        };

        storage
            .save_clip_metadata(game_id, &make_clip(real_clip.to_string_lossy().to_string()))
            .unwrap();
        storage
            .save_clip_metadata(
                game_id,
                &make_clip("pending/ChampionKill_412.mp4".to_string()),
            )
            .unwrap();
        storage
            .save_clip_metadata(game_id, &make_clip("pending/merged_10_18.mp4".to_string()))
            .unwrap();
        assert_eq!(storage.load_clip_metadata(game_id).unwrap().len(), 3);

        assert_eq!(storage.sweep_ghost_clip_metadata().unwrap(), 2);

        let remaining = storage.load_clip_metadata(game_id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].file_path, real_clip.to_string_lossy());

        // Idempotent: a second sweep has nothing left to remove.
        assert_eq!(storage.sweep_ghost_clip_metadata().unwrap(), 0);
    }

    fn vault_game(game_id: &str, start_time: chrono::DateTime<Utc>) -> GameMetadata {
        vault_game_with_metadata(game_id, game_id, "CLASSIC", start_time)
    }

    fn vault_game_with_metadata(
        game_id: &str,
        champion: &str,
        game_mode: &str,
        start_time: chrono::DateTime<Utc>,
    ) -> GameMetadata {
        GameMetadata {
            game_id: game_id.to_string(),
            champion: champion.to_string(),
            game_mode: game_mode.to_string(),
            start_time,
            end_time: None,
            result: None,
            kda: None,
        }
    }

    fn vault_clip(
        path: &str,
        created_at: chrono::DateTime<Utc>,
        priority: u8,
        score: Option<f64>,
    ) -> ClipMetadata {
        ClipMetadata {
            file_path: path.to_string(),
            thumbnail_path: None,
            event_type: models::EventType::ChampionKill,
            event_time: 1.0,
            priority,
            duration: 10.0,
            event_offset_secs: None,
            created_at,
            usage_count: 0,
            highlight_score: score,
            score_reasons: Vec::new(),
        }
    }

    #[test]
    fn clip_vault_orders_best_and_continues_from_opaque_cursor() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        storage
            .create_game("old", &vault_game("old", now - chrono::Duration::hours(2)))
            .unwrap();
        storage.create_game("new", &vault_game("new", now)).unwrap();
        storage
            .create_game("mid", &vault_game("mid", now - chrono::Duration::hours(1)))
            .unwrap();
        storage
            .save_clip_metadata("old", &vault_clip("C:\\old.mp4", now, 5, None))
            .unwrap();
        storage
            .save_clip_metadata("new", &vault_clip("C:\\new.mp4", now, 1, Some(101.0)))
            .unwrap();
        storage
            .save_clip_metadata("mid", &vault_clip("C:\\mid.mp4", now, 4, Some(90.0)))
            .unwrap();

        let first = storage
            .list_clip_vault_page(ClipVaultSort::Best, None, 2, None, None)
            .unwrap();
        assert_eq!(
            first
                .groups
                .iter()
                .map(|g| g.game_id.as_str())
                .collect::<Vec<_>>(),
            vec!["new", "old"]
        );
        let second = storage
            .list_clip_vault_page(
                ClipVaultSort::Best,
                first.next_cursor.as_deref(),
                2,
                None,
                None,
            )
            .unwrap();
        assert_eq!(
            second
                .groups
                .iter()
                .map(|g| g.game_id.as_str())
                .collect::<Vec<_>>(),
            vec!["mid"]
        );
        let newest = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 3, None, None)
            .unwrap();
        assert_eq!(newest.groups[0].game_id, "new");
        assert!(newest.next_cursor.is_none());
    }

    #[test]
    fn clip_vault_filters_by_query_and_exact_game_mode_before_pagination() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        let games = [
            (
                "ahri-classic",
                "Ahri",
                "CLASSIC",
                now - chrono::Duration::hours(2),
            ),
            (
                "ahri-aram",
                "Ahri",
                "ARAM",
                now - chrono::Duration::hours(1),
            ),
            ("jinx-classic", "Jinx", "CLASSIC", now),
        ];
        for (game_id, champion, game_mode, start_time) in games {
            storage
                .create_game(
                    game_id,
                    &vault_game_with_metadata(game_id, champion, game_mode, start_time),
                )
                .unwrap();
            storage
                .save_clip_metadata(
                    game_id,
                    &vault_clip(&format!("C:\\{game_id}.mp4"), now, 1, None),
                )
                .unwrap();
        }

        let ahri = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 12, Some(" aHrI "), None)
            .unwrap();
        assert_eq!(
            ahri.groups
                .iter()
                .map(|group| group.game_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ahri-aram", "ahri-classic"]
        );

        let aram = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 12, Some("aram"), Some("ARAM"))
            .unwrap();
        assert_eq!(aram.groups.len(), 1);
        assert_eq!(aram.groups[0].game_id, "ahri-aram");

        let game_id_match = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 12, Some("jinx-classic"), None)
            .unwrap();
        assert_eq!(game_id_match.groups.len(), 1);
        assert_eq!(game_id_match.groups[0].game_id, "jinx-classic");

        let classic = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 12, None, Some("CLASSIC"))
            .unwrap();
        assert_eq!(
            classic
                .groups
                .iter()
                .map(|group| group.game_id.as_str())
                .collect::<Vec<_>>(),
            vec!["jinx-classic", "ahri-classic"]
        );
    }

    #[test]
    fn clip_vault_filter_cursor_preserves_order_and_rejects_different_filters() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        for (game_id, start_time) in [
            ("ahri-old", now - chrono::Duration::hours(2)),
            ("ahri-mid", now - chrono::Duration::hours(1)),
            ("ahri-new", now),
        ] {
            storage
                .create_game(
                    game_id,
                    &vault_game_with_metadata(game_id, "Ahri", "CLASSIC", start_time),
                )
                .unwrap();
            storage
                .save_clip_metadata(
                    game_id,
                    &vault_clip(&format!("C:\\{game_id}.mp4"), now, 1, None),
                )
                .unwrap();
        }

        let first = storage
            .list_clip_vault_page(
                ClipVaultSort::Newest,
                None,
                2,
                Some("ahri"),
                Some("CLASSIC"),
            )
            .unwrap();
        assert_eq!(
            first
                .groups
                .iter()
                .map(|group| group.game_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ahri-new", "ahri-mid"]
        );
        let second = storage
            .list_clip_vault_page(
                ClipVaultSort::Newest,
                first.next_cursor.as_deref(),
                2,
                Some("ahri"),
                Some("CLASSIC"),
            )
            .unwrap();
        assert_eq!(
            second
                .groups
                .iter()
                .map(|group| group.game_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ahri-old"]
        );
        assert!(storage
            .list_clip_vault_page(
                ClipVaultSort::Newest,
                first.next_cursor.as_deref(),
                2,
                Some("ahri-new"),
                Some("CLASSIC"),
            )
            .is_err());
    }

    #[test]
    fn clip_vault_blank_filters_match_unfiltered_legacy_results() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        storage
            .create_game("game", &vault_game("game", now))
            .unwrap();
        storage
            .save_clip_metadata("game", &vault_clip("C:\\game.mp4", now, 1, None))
            .unwrap();

        let unfiltered = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 1, None, None)
            .unwrap();
        let blank_filters = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 1, Some("  "), Some("  "))
            .unwrap();
        assert_eq!(unfiltered.groups.len(), blank_filters.groups.len());
        assert_eq!(
            unfiltered.groups[0].game_id,
            blank_filters.groups[0].game_id
        );
        assert_eq!(unfiltered.next_cursor, blank_filters.next_cursor);
    }

    #[test]
    fn clip_vault_sorts_clips_inside_each_game() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        storage
            .create_game("game", &vault_game("game", now))
            .unwrap();
        storage
            .save_clip_metadata(
                "game",
                &vault_clip(
                    "C:\\old-best.mp4",
                    now - chrono::Duration::minutes(1),
                    1,
                    Some(90.0),
                ),
            )
            .unwrap();
        storage
            .save_clip_metadata("game", &vault_clip("C:\\new-low.mp4", now, 1, Some(10.0)))
            .unwrap();

        let best = storage
            .list_clip_vault_page(ClipVaultSort::Best, None, 1, None, None)
            .unwrap();
        assert_eq!(best.groups[0].clips[0].file_path, "C:\\old-best.mp4");
        let newest = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 1, None, None)
            .unwrap();
        assert_eq!(newest.groups[0].clips[0].file_path, "C:\\new-low.mp4");
    }

    #[test]
    fn clip_vault_skips_a_corrupt_clip_row() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        storage
            .create_game("game", &vault_game("game", now))
            .unwrap();
        storage
            .save_clip_metadata("game", &vault_clip("C:\\valid.mp4", now, 1, None))
            .unwrap();
        storage.conn().unwrap().execute(
            "INSERT INTO clips (game_id, file_path, metadata_json, event_time, priority, created_at, updated_at) VALUES (?1, ?2, ?3, 0, 1, ?4, ?4)",
            params!["game", "C:\\corrupt.mp4", "{not-json", now.to_rfc3339()],
        ).unwrap();
        let page = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 1, None, None)
            .unwrap();
        assert_eq!(page.groups[0].clips.len(), 1);
        assert_eq!(page.skipped_item_count, 1);
    }

    #[test]
    fn clip_vault_keeps_valid_clips_when_game_metadata_is_corrupt() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        storage
            .create_game("game", &vault_game("game", now))
            .unwrap();
        storage
            .save_clip_metadata("game", &vault_clip("C:\\valid.mp4", now, 1, None))
            .unwrap();
        storage
            .conn()
            .unwrap()
            .execute(
                "UPDATE games SET metadata_json = ?1 WHERE game_id = ?2",
                params!["{not-json", "game"],
            )
            .unwrap();

        let page = storage
            .list_clip_vault_page(ClipVaultSort::Newest, None, 1, None, None)
            .unwrap();
        assert_eq!(page.groups[0].game_id, "game");
        assert!(page.groups[0].game.is_none());
        assert_eq!(page.groups[0].clip_count, 1);
        assert_eq!(page.skipped_item_count, 1);
    }

    #[test]
    fn thumbnail_helpers_and_owned_update_preserve_other_metadata() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        let now = Utc::now();
        storage
            .create_game("owner", &vault_game("owner", now))
            .unwrap();
        storage
            .create_game("other", &vault_game("other", now))
            .unwrap();
        let mut clip = vault_clip("C:\\clip.mp4", now, 3, Some(77.0));
        clip.event_offset_secs = Some(4.0);
        storage.save_clip_metadata("owner", &clip).unwrap();
        assert_eq!(thumbnail_offset_secs(&clip), 4.0);
        clip.event_offset_secs = Some(20.0);
        assert_eq!(thumbnail_offset_secs(&clip), 5.0);
        assert_eq!(
            thumbnail_output_path(Path::new("C:\\videos\\clip.mp4")).unwrap(),
            PathBuf::from("C:\\videos\\clip_thumbnail.jpg")
        );
        assert!(storage
            .load_owned_clip_metadata("other", "C:\\clip.mp4")
            .is_err());
        let updated = storage
            .update_owned_clip_thumbnail("owner", "C:\\clip.mp4", "C:\\clip.jpg")
            .unwrap();
        assert_eq!(updated.thumbnail_path.as_deref(), Some("C:\\clip.jpg"));
        assert_eq!(updated.highlight_score, Some(77.0));
    }

    #[test]
    fn media_job_publication_and_quota_are_idempotent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();
        storage
            .create_media_job("job-1", "user-1", MediaJobKind::AutoEdit, "{}")
            .unwrap();
        storage
            .update_media_job_status(
                "job-1",
                MediaJobStatus::Running,
                "rendering",
                50.0,
                None,
                None,
            )
            .unwrap();
        storage
            .update_media_job_status(
                "job-1",
                MediaJobStatus::Validating,
                "publishing",
                99.0,
                None,
                None,
            )
            .unwrap();
        assert!(storage
            .publish_auto_edit_series("job-1", "user-1", false, false, &[], &[])
            .unwrap());
        assert!(!storage
            .publish_auto_edit_series("job-1", "user-1", false, false, &[], &[])
            .unwrap());
        assert_eq!(
            storage.load_auto_edit_usage("user-1").unwrap().usage_count,
            1
        );
        let snapshot = storage.load_media_job("job-1").unwrap();
        assert_eq!(snapshot.status, MediaJobStatus::Complete);
        assert!(snapshot.quota_sync_pending);
    }

    #[test]
    fn active_media_job_probe_covers_queued_running_and_validating_states() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        assert!(!storage.has_active_media_jobs().unwrap());
        storage
            .create_media_job("job-active", "user-active", MediaJobKind::AutoEdit, "{}")
            .unwrap();
        assert!(storage.has_active_media_jobs().unwrap());

        storage
            .update_media_job_status(
                "job-active",
                MediaJobStatus::Running,
                "rendering",
                20.0,
                None,
                None,
            )
            .unwrap();
        assert!(storage.has_active_media_jobs().unwrap());

        storage
            .update_media_job_status(
                "job-active",
                MediaJobStatus::Validating,
                "validating",
                90.0,
                None,
                None,
            )
            .unwrap();
        assert!(storage.has_active_media_jobs().unwrap());

        storage
            .update_media_job_status(
                "job-active",
                MediaJobStatus::Complete,
                "complete",
                100.0,
                None,
                None,
            )
            .unwrap();
        assert!(!storage.has_active_media_jobs().unwrap());
    }

    #[test]
    fn startup_marks_interrupted_jobs_recoverable() {
        let temp_dir = tempfile::tempdir().unwrap();
        {
            let storage = Storage::new(temp_dir.path()).unwrap();
            storage
                .create_media_job("job-2", "user-2", MediaJobKind::AutoEdit, "{}")
                .unwrap();
            storage
                .update_media_job_status(
                    "job-2",
                    MediaJobStatus::Running,
                    "rendering",
                    25.0,
                    None,
                    None,
                )
                .unwrap();
        }
        let reopened = Storage::new(temp_dir.path()).unwrap();
        let snapshot = reopened.load_media_job("job-2").unwrap();
        assert_eq!(snapshot.status, MediaJobStatus::Recoverable);
        assert!(snapshot.recoverable);
    }
}
