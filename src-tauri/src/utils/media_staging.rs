use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::utils::security;
use crate::AppState;
use tauri_plugin_dialog::DialogExt;

const MAX_VIDEO_BYTES: u64 = 50 * 1024 * 1024 * 1024;
const MAX_AUDIO_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StagedMediaKind {
    Video,
    Audio,
    Image,
}

#[derive(Debug, Clone, Serialize)]
pub struct StagedMedia {
    pub path: String,
    pub size_bytes: u64,
    pub reused_app_owned_file: bool,
    pub original_file_name: String,
}

fn validate_source(source_path: &str, kind: StagedMediaKind) -> Result<PathBuf, String> {
    let validated = match kind {
        StagedMediaKind::Video => security::validate_video_input_path(source_path),
        StagedMediaKind::Audio => security::validate_audio_path(source_path),
        StagedMediaKind::Image => security::validate_image_path(source_path),
    };
    validated
        .map_err(|_| "MEDIA_STAGE_INVALID_SOURCE".to_string())?
        .canonicalize()
        .map_err(|_| "MEDIA_STAGE_SOURCE_UNAVAILABLE".to_string())
}

fn size_limit(kind: StagedMediaKind) -> u64 {
    match kind {
        StagedMediaKind::Video => MAX_VIDEO_BYTES,
        StagedMediaKind::Audio => MAX_AUDIO_BYTES,
        StagedMediaKind::Image => MAX_IMAGE_BYTES,
    }
}

async fn stage_selected_media(
    state: &AppState,
    source_path: String,
    kind: StagedMediaKind,
) -> Result<StagedMedia, String> {
    let source = validate_source(&source_path, kind)?;
    let metadata = tokio::fs::metadata(&source)
        .await
        .map_err(|_| "MEDIA_STAGE_SOURCE_UNAVAILABLE".to_string())?;
    if !metadata.is_file() {
        return Err("MEDIA_STAGE_SOURCE_NOT_FILE".to_string());
    }
    if metadata.len() > size_limit(kind) {
        return Err("MEDIA_STAGE_SOURCE_TOO_LARGE".to_string());
    }
    let original_file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "MEDIA_STAGE_INVALID_SOURCE".to_string())?
        .to_string();

    let app_root = state
        .storage
        .base_path()
        .canonicalize()
        .unwrap_or_else(|_| state.storage.base_path().to_path_buf());
    if source.starts_with(&app_root) {
        return Ok(StagedMedia {
            path: source.to_string_lossy().to_string(),
            size_bytes: metadata.len(),
            reused_app_owned_file: true,
            original_file_name,
        });
    }

    let staging_dir = app_root.join("staging").join("imports");
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|_| "MEDIA_STAGE_DIRECTORY_FAILED".to_string())?;

    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "MEDIA_STAGE_INVALID_SOURCE".to_string())?
        .to_ascii_lowercase();
    let id = uuid::Uuid::new_v4();
    let destination = staging_dir.join(format!("{id}.{extension}"));
    let partial = staging_dir.join(format!("{id}.{extension}.partial"));

    let copied = match tokio::fs::copy(&source, &partial).await {
        Ok(copied) => copied,
        Err(error) => {
            let _ = tokio::fs::remove_file(&partial).await;
            tracing::warn!(error = %error, "Failed to copy selected media into app staging");
            return Err("MEDIA_STAGE_COPY_FAILED".to_string());
        }
    };
    if copied != metadata.len() {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err("MEDIA_STAGE_COPY_INCOMPLETE".to_string());
    }
    if tokio::fs::rename(&partial, &destination).await.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err("MEDIA_STAGE_FINALIZE_FAILED".to_string());
    }

    Ok(StagedMedia {
        path: destination.to_string_lossy().to_string(),
        size_bytes: copied,
        reused_app_owned_file: false,
        original_file_name,
    })
}

/// Select and stage on the native side so the renderer cannot submit an
/// arbitrary local path and turn staging into a general file-read primitive.
#[tauri::command]
pub async fn select_and_stage_external_media(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    kind: StagedMediaKind,
) -> Result<Option<StagedMedia>, String> {
    let (title, filter_name, extensions): (&str, &str, &[&str]) = match kind {
        StagedMediaKind::Video => (
            "Select a video",
            "Video files",
            &["mp4", "mov", "mkv", "avi", "webm", "m4v"],
        ),
        StagedMediaKind::Audio => (
            "Select audio",
            "Audio files",
            &["mp3", "wav", "ogg", "flac", "m4a", "aac", "wma"],
        ),
        StagedMediaKind::Image => (
            "Select an image",
            "Image files",
            &["jpg", "jpeg", "png", "webp"],
        ),
    };
    let selection = app
        .dialog()
        .file()
        .set_title(title)
        .add_filter(filter_name, extensions)
        .blocking_pick_file();
    let Some(selection) = selection else {
        return Ok(None);
    };
    let source = selection
        .into_path()
        .map_err(|_| "MEDIA_STAGE_INVALID_SOURCE".to_string())?;
    let staged = stage_selected_media(&state, source.to_string_lossy().to_string(), kind).await?;
    Ok(Some(staged))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_kinds_have_bounded_sizes() {
        assert!(size_limit(StagedMediaKind::Image) < size_limit(StagedMediaKind::Audio));
        assert!(size_limit(StagedMediaKind::Audio) < size_limit(StagedMediaKind::Video));
    }

    #[test]
    fn validation_rejects_a_mismatched_extension() {
        let path = if cfg!(windows) {
            r"C:\selected\payload.exe"
        } else {
            "/selected/payload.exe"
        };
        assert_eq!(
            validate_source(path, StagedMediaKind::Video).unwrap_err(),
            "MEDIA_STAGE_INVALID_SOURCE"
        );
    }

    #[test]
    fn app_owned_containment_uses_path_components() {
        let root = std::path::Path::new(if cfg!(windows) {
            r"C:\Users\tester\AppData\Roaming\lolshorts"
        } else {
            "/data/lolshorts"
        });
        assert!(root.join("recordings/clip.mp4").starts_with(root));
        assert!(!root
            .with_extension("-other")
            .join("clip.mp4")
            .starts_with(root));
    }
}
