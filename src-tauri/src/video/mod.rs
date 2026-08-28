pub mod auto_composer;
pub mod commands;
pub mod media_job_executor;
pub mod output_validation;
pub mod processor;
pub mod statistics;
pub mod thumbnail;

pub use auto_composer::{
    AutoComposer, AutoEditConfig, AutoEditJobReceipt, AutoEditPlan, AutoEditProgress,
    AutoEditResult, CanvasTemplate,
};
pub use output_validation::{
    OutputValidationIssue, OutputValidationReport, OutputValidationSeverity,
    OutputValidationStatus, OutputValidator,
};
pub use processor::VideoProcessor;
pub use statistics::get_global_stats;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

tokio::task_local! {
    static AUTO_EDIT_CANCELLATION: CancellationToken;
    static AUTO_EDIT_JOB_ID: String;
}

/// Scope all nested FFmpeg calls and temporary artifacts to one auto-edit job.
pub async fn with_auto_edit_context<F, T>(
    job_id: String,
    cancellation: CancellationToken,
    future: F,
) -> T
where
    F: std::future::Future<Output = T>,
{
    AUTO_EDIT_JOB_ID
        .scope(job_id, AUTO_EDIT_CANCELLATION.scope(cancellation, future))
        .await
}

pub(crate) fn current_auto_edit_job_id() -> Option<String> {
    AUTO_EDIT_JOB_ID.try_with(Clone::clone).ok()
}

/// Video processing errors with user-friendly messages
#[derive(Debug, Error)]
pub enum VideoError {
    // File System Errors
    #[error("Video file not found: {path}\n\nPlease check if the file exists and hasn't been moved or deleted.")]
    FileNotFound { path: String },

    #[error("Cannot read video file: {path}\n\nPossible causes:\n- File is corrupted or incomplete\n- Insufficient permissions\n- File is being used by another program")]
    FileAccessError { path: String },

    #[error("Not enough disk space to save video\n\nRequired: {required_mb} MB\nAvailable: {available_mb} MB\n\nFree up space or choose a different output location.")]
    InsufficientDiskSpace { required_mb: u64, available_mb: u64 },

    #[error("Output directory not found: {path}\n\nPlease ensure the directory exists or choose a different location.")]
    OutputDirectoryNotFound { path: String },

    // FFmpeg Errors
    #[error("FFmpeg is not installed or not found in system PATH\n\nPlease install FFmpeg from https://ffmpeg.org/download.html")]
    FfmpegNotFound,

    #[error("FFprobe is not bundled or not found in system PATH\n\nVideo probing features need FFprobe beside FFmpeg.")]
    FfprobeNotFound,

    #[error("FFmpeg process failed: {message}\n\nTechnical details: {stderr}")]
    FfmpegProcessError { message: String, stderr: String },

    #[error("Video codec not supported: {codec}\n\nSupported formats: MP4, AVI, MKV, MOV\nPlease convert your video file to a supported format.")]
    UnsupportedCodec { codec: String },

    #[error("Video file is corrupted or invalid\n\nThe video file may be damaged. Try:\n- Re-recording the game\n- Using a different video file\n- Checking if the file plays in a video player")]
    CorruptedVideo,

    // Canvas/Audio Processing Errors
    #[error("Failed to apply canvas overlay\n\nReason: {reason}\n\nPlease check your canvas template configuration.")]
    CanvasApplicationError { reason: String },

    #[error("Background music file not found: {path}\n\nPlease upload a valid audio file.")]
    BackgroundMusicNotFound { path: String },

    #[error("Audio mixing failed: {reason}\n\nCheck that:\n- Game audio exists in the clip\n- Background music file is valid\n- Audio levels are correctly configured")]
    AudioMixingError { reason: String },

    // Clip Selection Errors
    #[error("No clips found for the selected games\n\nMake sure you have:\n- Recorded some games\n- Interesting events occurred (kills, objectives, etc.)\n- Clips were successfully saved")]
    NoClipsFound,

    #[error("Not enough clips to create {target_duration}s video\n\nFound: {available_duration}s of clips\nRequired: {target_duration}s\n\nTry:\n- Selecting more games\n- Reducing target duration\n- Lowering priority threshold")]
    InsufficientClips {
        available_duration: u64,
        target_duration: u64,
    },

    // Concatenation Errors
    #[error("Failed to merge video clips\n\nReason: {reason}\n\nThis may be due to:\n- Incompatible video formats\n- Corrupted clip files\n- Insufficient system resources")]
    ConcatenationError { reason: String },

    // Resource Errors
    #[error("System resources exhausted\n\nVideo processing requires:\n- At least 2GB free RAM\n- CPU availability\n\nClose other applications and try again.")]
    ResourceExhaustion,

    #[error("Video processing timeout\n\nOperation took longer than {timeout_secs}s\n\nTry:\n- Processing fewer clips\n- Reducing video duration\n- Closing other applications")]
    Timeout { timeout_secs: u64 },

    #[error("Auto-edit cancelled")]
    Cancelled,

    // Generic fallback
    #[error("Video processing failed: {message}")]
    ProcessingError { message: String },

    #[error("Unexpected error: {0}\n\nPlease report this issue if it persists.")]
    AnyhowError(#[from] anyhow::Error),
}

impl VideoError {
    /// Convert FFmpeg stderr output to user-friendly error
    pub fn from_ffmpeg_stderr(stderr: &str) -> Self {
        // Check for common FFmpeg error patterns
        if stderr.contains("No such file or directory") {
            if let Some(path) = extract_file_path_from_stderr(stderr) {
                return Self::FileNotFound { path };
            }
        }

        if stderr.contains("Invalid data found") || stderr.contains("moov atom not found") {
            return Self::CorruptedVideo;
        }

        if stderr.contains("Codec") && stderr.contains("not currently supported") {
            if let Some(codec) = extract_codec_from_stderr(stderr) {
                return Self::UnsupportedCodec { codec };
            }
        }

        if stderr.contains("Permission denied") {
            if let Some(path) = extract_file_path_from_stderr(stderr) {
                return Self::FileAccessError { path };
            }
        }

        if stderr.contains("No space left on device") {
            return Self::InsufficientDiskSpace {
                required_mb: 0, // Will be calculated by caller
                available_mb: 0,
            };
        }

        // Generic FFmpeg error with details
        Self::FfmpegProcessError {
            message: "FFmpeg failed to process video".to_string(),
            stderr: stderr.to_string(),
        }
    }

    /// Get user-friendly recovery suggestions
    pub fn recovery_suggestions(&self) -> Vec<String> {
        match self {
            Self::FileNotFound { .. } => vec![
                "Check if the file was moved or deleted".to_string(),
                "Re-record the game if the clip is missing".to_string(),
            ],
            Self::InsufficientDiskSpace { .. } => vec![
                "Free up disk space on your drive".to_string(),
                "Change output location in settings".to_string(),
                "Delete old videos you no longer need".to_string(),
            ],
            Self::FfmpegNotFound => vec![
                "Install FFmpeg from https://ffmpeg.org".to_string(),
                "Add FFmpeg to your system PATH".to_string(),
                "Restart the application after installing".to_string(),
            ],
            Self::FfprobeNotFound => vec![
                "Bundle FFprobe beside FFmpeg".to_string(),
                "Run the FFmpeg preparation script before building installers".to_string(),
                "Restart the application after installing FFprobe".to_string(),
            ],
            Self::NoClipsFound => vec![
                "Record more games to generate clips".to_string(),
                "Check recording settings are enabled".to_string(),
                "Verify League of Legends client is running".to_string(),
            ],
            Self::InsufficientClips { .. } => vec![
                "Select more games to get more clips".to_string(),
                "Reduce target video duration".to_string(),
                "Lower clip priority threshold in settings".to_string(),
            ],
            Self::CorruptedVideo => vec![
                "Re-record the affected game".to_string(),
                "Check if the video plays in a media player".to_string(),
                "Delete and re-create the clip".to_string(),
            ],
            _ => vec![
                "Try again".to_string(),
                "Contact support if issue persists".to_string(),
            ],
        }
    }
}

/// Extract file path from FFmpeg stderr output
fn extract_file_path_from_stderr(stderr: &str) -> Option<String> {
    // Look for patterns like: "filename: No such file or directory"
    stderr
        .lines()
        .find(|line| line.contains("No such file") || line.contains("Permission denied"))
        .and_then(|line| line.split(':').next().map(|s| s.trim().to_string()))
}

/// Extract codec name from FFmpeg stderr output
fn extract_codec_from_stderr(stderr: &str) -> Option<String> {
    // Look for patterns like: "Codec 'xyz' is not currently supported"
    stderr
        .lines()
        .find(|line| line.contains("Codec"))
        .and_then(|line| line.split('\'').nth(1).map(|s| s.to_string()))
}

pub type Result<T> = std::result::Result<T, VideoError>;

const FFMPEG_PROCESS_TIMEOUT_SECS: u64 = 30 * 60;

/// Helper to execute FFmpeg command with proper error handling
pub async fn execute_ffmpeg_command(command: &mut tokio::process::Command) -> Result<()> {
    use tokio::time::{timeout, Duration};

    // Bound the number of concurrent offline FFmpeg processes. Held for the
    // duration of this encode; released on drop. Realtime recording does not go
    // through this path, so it is never throttled.
    let mut pool_permit = crate::utils::ffmpeg_pool::global_ffmpeg_pool()
        .acquire()
        .await
        .map_err(|e| VideoError::ProcessingError {
            message: format!("FFmpeg pool acquire failed: {}", e),
        })?;

    command.stderr(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::null());
    command.kill_on_drop(true);

    let child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            VideoError::FfmpegNotFound
        } else {
            VideoError::ProcessingError {
                message: format!("Failed to execute FFmpeg process: {}", e),
            }
        }
    })?;
    let pid = child.id();
    pool_permit.register_process(&child).await;
    let wait = timeout(
        Duration::from_secs(FFMPEG_PROCESS_TIMEOUT_SECS),
        child.wait_with_output(),
    );
    tokio::pin!(wait);

    let cancellation = AUTO_EDIT_CANCELLATION.try_with(Clone::clone).ok();
    let wait_result = if let Some(token) = cancellation {
        tokio::select! {
            result = &mut wait => Some(result),
            _ = token.cancelled() => {
                if let Some(pid) = pid {
                    terminate_process_tree(pid).await;
                }
                // A failed OS-level tree kill must not turn a user cancellation
                // into a 30-minute wait. Dropping the still-running wait also
                // triggers Tokio's kill_on_drop fallback for the direct child.
                let _ = tokio::time::timeout(Duration::from_secs(5), &mut wait).await;
                return Err(VideoError::Cancelled);
            }
        }
    } else {
        Some(wait.await)
    };

    let output = match wait_result {
        Some(Ok(result)) => result,
        Some(Err(_)) => {
            if let Some(pid) = pid {
                terminate_process_tree(pid).await;
            }
            return Err(VideoError::Timeout {
                timeout_secs: FFMPEG_PROCESS_TIMEOUT_SECS,
            });
        }
        None => unreachable!("cancellation branch returns immediately"),
    }
    .map_err(|e| VideoError::ProcessingError {
        message: format!("Failed to wait for FFmpeg process: {}", e),
    })?;

    if !output.status.success() {
        let stderr_output = String::from_utf8_lossy(&output.stderr);
        return Err(VideoError::from_ffmpeg_stderr(&stderr_output));
    }

    Ok(())
}

pub(crate) async fn terminate_process_tree(pid: u32) {
    #[cfg(windows)]
    {
        let mut command = tokio::process::Command::new("taskkill");
        command
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        match tokio::time::timeout(std::time::Duration::from_secs(5), command.status()).await {
            Ok(Ok(status)) if status.success() => {}
            Ok(Ok(status)) => tracing::warn!(
                "taskkill returned {} while terminating FFmpeg process tree {}",
                status,
                pid
            ),
            Ok(Err(error)) => {
                tracing::warn!("Failed to terminate FFmpeg process tree {}: {}", pid, error)
            }
            Err(_) => tracing::warn!("Timed out while terminating FFmpeg process tree {}", pid),
        }
    }
    #[cfg(not(windows))]
    {
        let mut command = tokio::process::Command::new("kill");
        command
            .args(["-TERM", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        match tokio::time::timeout(std::time::Duration::from_secs(5), command.status()).await {
            Ok(Ok(status)) if status.success() => {}
            Ok(Ok(status)) => tracing::warn!(
                "kill returned {} while terminating FFmpeg process {}",
                status,
                pid
            ),
            Ok(Err(error)) => {
                tracing::warn!("Failed to terminate FFmpeg process {}: {}", pid, error)
            }
            Err(_) => tracing::warn!("Timed out while terminating FFmpeg process {}", pid),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipInfo {
    pub id: i64,
    pub game_id: String, // Added for tracking and storage update
    pub event_type: String,
    pub event_time: f64,
    pub priority: i32,
    pub file_path: String,
    pub thumbnail_path: Option<String>,
    pub duration: Option<f64>,
    #[serde(default)]
    pub usage_count: u32, // Track usage for filtering
    /// 하이라이트 점수(`recording::highlight_score`). 없으면 `priority` 로 되돌아간다.
    ///
    /// 이 값이 붙기 전에는 선택 순서가 `priority` (1~5) 뿐이었고, 그 눈금에서는
    /// 퍼블·바론·게임종료가 전부 3점이라 어느 것이 먼저 나올지가 사실상 우연이었다.
    #[serde(default)]
    pub highlight_score: Option<f64>,
    /// 이 클립 **안에서** 하이라이트가 일어나는 지점(초). `ClipMetadata` 와 같은 값.
    ///
    /// 저장 시점엔 알고 있는 값인데(`pre_duration`) 여기까지 오지 않아서, 하류가
    /// 전부 "하이라이트 = 클립 중앙" 이라고 가정했다. 그 가정은 킬 클립(pre 10 /
    /// post 3, 중앙 6.5초 ≈ 이벤트 10초)에서는 대충 맞지만 **게임 종료 클립
    /// (pre 30 / post 10)에서는 20초를 빗나간다** — 40초를 12초로 줄이면 잘리는
    /// 구간이 14~26초라 승리 순간(30초)이 통째로 빠졌다.
    ///
    /// 예전 클립에는 없다(`None`). 소비하는 쪽이 중앙으로 되돌아간다.
    #[serde(default)]
    pub event_offset_secs: Option<f64>,
    /// 점수가 그렇게 나온 이유. 훅 자막의 둘째 줄이 되는 값이다
    /// ("혼자서 · 1v3 · 체력 8%").
    #[serde(default)]
    pub score_reasons: Vec<crate::recording::highlight_score::ScoreReason>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_processor_creation() {
        let _processor = VideoProcessor::new();
    }
}
