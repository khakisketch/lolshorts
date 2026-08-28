use crate::storage::{MediaJobPart, MediaJobStatus, Storage};
use crate::video::{VideoError, VideoError::Cancelled};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const DEFAULT_MEDIA_PROCESS_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFailureClass {
    Paused,
    Recoverable,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFailurePoint {
    Process,
    Validate,
    PartCheckpoint,
    Publish,
    QuotaSync,
}

#[derive(Debug, Clone)]
pub struct MediaProcessOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[async_trait]
pub trait MediaProcessRunner: Send + Sync {
    async fn run(
        &self,
        program: &Path,
        args: &[String],
        cancellation: CancellationToken,
    ) -> Result<MediaProcessOutput, VideoError>;
}

pub struct TokioMediaProcessRunner {
    timeout: Duration,
}

impl TokioMediaProcessRunner {
    #[cfg(test)]
    fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for TokioMediaProcessRunner {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_MEDIA_PROCESS_TIMEOUT,
        }
    }
}

#[async_trait]
impl MediaProcessRunner for TokioMediaProcessRunner {
    async fn run(
        &self,
        program: &Path,
        args: &[String],
        cancellation: CancellationToken,
    ) -> Result<MediaProcessOutput, VideoError> {
        let mut pool_permit = crate::utils::ffmpeg_pool::global_ffmpeg_pool()
            .acquire()
            .await
            .map_err(|error| VideoError::ProcessingError {
                message: format!("FFmpeg pool acquire failed: {error}"),
            })?;
        let mut command = tokio::process::Command::new(program);
        command
            .args(args)
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                VideoError::FfmpegNotFound
            } else {
                VideoError::ProcessingError {
                    message: error.to_string(),
                }
            }
        })?;
        let pid = child.id();
        pool_permit.register_process(&child).await;
        let wait = tokio::time::timeout(self.timeout, child.wait_with_output());
        tokio::pin!(wait);
        let wait_result = tokio::select! {
            output = &mut wait => Some(output),
            _ = cancellation.cancelled() => None,
        };
        let output = match wait_result {
            Some(Ok(output)) => output.map_err(|error| VideoError::ProcessingError {
                message: error.to_string(),
            })?,
            Some(Err(_)) => {
                if let Some(pid) = pid {
                    crate::video::terminate_process_tree(pid).await;
                }
                return Err(VideoError::Timeout {
                    timeout_secs: self.timeout.as_secs(),
                });
            }
            None => {
                if let Some(pid) = pid {
                    crate::video::terminate_process_tree(pid).await;
                }
                let _ = tokio::time::timeout(Duration::from_secs(5), &mut wait).await;
                return Err(Cancelled);
            }
        };
        Ok(MediaProcessOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

#[async_trait]
pub trait MediaJobRepository: Send + Sync {
    async fn update_status(
        &self,
        job_id: &str,
        status: MediaJobStatus,
        stage: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), String>;
    async fn update_part(&self, job_id: &str, part: &MediaJobPart) -> Result<(), String>;
}

#[async_trait]
impl MediaJobRepository for Storage {
    async fn update_status(
        &self,
        job_id: &str,
        status: MediaJobStatus,
        stage: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), String> {
        self.update_media_job_status(job_id, status, stage, 0.0, error_code, error_message)
            .map_err(|error| error.to_string())
    }

    async fn update_part(&self, job_id: &str, part: &MediaJobPart) -> Result<(), String> {
        self.update_media_job_part(job_id, part)
            .map_err(|error| error.to_string())
    }
}

#[async_trait]
pub trait MediaFileSystem: Send + Sync {
    async fn fingerprint(&self, path: &Path) -> Result<String, String>;
    fn validated_candidates(
        &self,
        stage_dir: &Path,
        final_dir: &Path,
        result_id: &str,
        persisted: Option<&Path>,
    ) -> Vec<PathBuf>;
}

pub struct StdMediaFileSystem;

#[async_trait]
impl MediaFileSystem for StdMediaFileSystem {
    async fn fingerprint(&self, path: &Path) -> Result<String, String> {
        crate::video::output_validation::file_fingerprint_async(path.to_path_buf())
            .await
            .map_err(|error| error.to_string())
    }

    fn validated_candidates(
        &self,
        stage_dir: &Path,
        final_dir: &Path,
        result_id: &str,
        persisted: Option<&Path>,
    ) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(path) = persisted {
            candidates.push(path.to_path_buf());
        }
        candidates.push(
            stage_dir
                .join("parts")
                .join(format!("{result_id}.validated.mp4")),
        );
        candidates.push(final_dir.join(format!("{result_id}.mp4")));
        // Keep recovery precedence stable: an explicitly persisted path wins,
        // then the deterministic stage path, then the published final path.
        // `Vec::dedup` only removes adjacent values, so preserve that order
        // while removing aliases that resolve to the same path.
        let mut unique = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if !unique.contains(&candidate) {
                unique.push(candidate);
            }
        }
        unique
    }
}

pub trait MediaFailureInjector: Send + Sync {
    fn before(&self, point: MediaFailurePoint) -> Result<(), VideoError>;
}

pub struct NoMediaFailureInjection;

impl MediaFailureInjector for NoMediaFailureInjection {
    fn before(&self, _point: MediaFailurePoint) -> Result<(), VideoError> {
        Ok(())
    }
}

/// Shared execution dependencies and deterministic failure policy. Commands
/// retain authentication/ownership checks; all media state decisions go
/// through this object, and tests can replace any dependency independently.
pub struct MediaJobExecutor {
    pub process_runner: Arc<dyn MediaProcessRunner>,
    pub repository: Arc<dyn MediaJobRepository>,
    pub file_system: Arc<dyn MediaFileSystem>,
    pub failure_injector: Arc<dyn MediaFailureInjector>,
}

impl MediaJobExecutor {
    pub fn production(storage: Arc<Storage>) -> Self {
        Self {
            process_runner: Arc::new(TokioMediaProcessRunner::default()),
            repository: storage,
            file_system: Arc::new(StdMediaFileSystem),
            failure_injector: Arc::new(NoMediaFailureInjection),
        }
    }

    pub fn before(&self, point: MediaFailurePoint) -> Result<(), VideoError> {
        self.failure_injector.before(point)
    }

    pub fn classify(&self, error: &VideoError) -> MediaFailureClass {
        classify_video_failure(error)
    }
}

pub fn classify_video_failure(error: &VideoError) -> MediaFailureClass {
    match error {
        VideoError::Cancelled => MediaFailureClass::Paused,
        VideoError::FileNotFound { .. } | VideoError::CorruptedVideo => MediaFailureClass::Failed,
        VideoError::ProcessingError { message }
            if message.starts_with("Output validation failed:") =>
        {
            MediaFailureClass::Failed
        }
        _ => MediaFailureClass::Recoverable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::VecDeque;

    struct ScriptedInjector(Mutex<VecDeque<(MediaFailurePoint, bool)>>);

    impl MediaFailureInjector for ScriptedInjector {
        fn before(&self, point: MediaFailurePoint) -> Result<(), VideoError> {
            let (expected, should_fail) = self.0.lock().pop_front().expect("unexpected call");
            assert_eq!(expected, point);
            if should_fail {
                Err(VideoError::ProcessingError {
                    message: format!("injected {point:?}"),
                })
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn scripted_failure_is_ordered_and_recoverable() {
        let injector = ScriptedInjector(Mutex::new(VecDeque::from([
            (MediaFailurePoint::Process, false),
            (MediaFailurePoint::PartCheckpoint, true),
        ])));
        assert!(injector.before(MediaFailurePoint::Process).is_ok());
        let error = injector
            .before(MediaFailurePoint::PartCheckpoint)
            .unwrap_err();
        assert_eq!(
            classify_video_failure(&error),
            MediaFailureClass::Recoverable
        );
    }

    #[test]
    fn every_failure_point_is_ordered_and_injected_failures_are_recoverable() {
        let points = [
            MediaFailurePoint::Process,
            MediaFailurePoint::Validate,
            MediaFailurePoint::PartCheckpoint,
            MediaFailurePoint::Publish,
            MediaFailurePoint::QuotaSync,
        ];

        for failure_index in 0..points.len() {
            let injector = ScriptedInjector(Mutex::new(
                points
                    .iter()
                    .enumerate()
                    .map(|(index, point)| (*point, index == failure_index))
                    .collect(),
            ));
            for (index, point) in points.iter().copied().enumerate() {
                let result = injector.before(point);
                if index == failure_index {
                    let error = result.expect_err("selected point must fail");
                    assert_eq!(
                        classify_video_failure(&error),
                        MediaFailureClass::Recoverable,
                        "{point:?} failures must leave the job resumable"
                    );
                    break;
                }
                assert!(
                    result.is_ok(),
                    "{point:?} must precede the injected failure"
                );
            }
        }
    }

    #[test]
    fn validated_candidates_preserve_persisted_stage_final_precedence_and_deduplicate() {
        let fs = StdMediaFileSystem;
        let stage = PathBuf::from("C:/media/stage");
        let final_dir = PathBuf::from("C:/media/final");
        let result_id = "result-42";
        let stage_candidate = stage.join("parts").join("result-42.validated.mp4");
        let final_candidate = final_dir.join("result-42.mp4");

        assert_eq!(
            fs.validated_candidates(&stage, &final_dir, result_id, Some(&stage_candidate)),
            vec![stage_candidate.clone(), final_candidate.clone()],
            "a persisted stage path must not be duplicated"
        );
        assert_eq!(
            fs.validated_candidates(&stage, &final_dir, result_id, Some(&final_candidate)),
            vec![final_candidate.clone(), stage_candidate.clone()],
            "the persisted final path must still take precedence"
        );
        assert_eq!(
            fs.validated_candidates(&stage, &final_dir, result_id, None),
            vec![stage_candidate, final_candidate],
            "without persisted output recovery is stage then final"
        );
    }

    #[cfg(windows)]
    fn process_command(exit_code: i32) -> (std::path::PathBuf, Vec<String>) {
        (
            std::path::PathBuf::from("cmd.exe"),
            vec![
                "/C".to_string(),
                format!("echo stdout & echo stderr 1>&2 & exit /b {exit_code}"),
            ],
        )
    }

    #[cfg(not(windows))]
    fn process_command(exit_code: i32) -> (std::path::PathBuf, Vec<String>) {
        (
            std::path::PathBuf::from("sh"),
            vec![
                "-c".to_string(),
                format!("printf stdout; printf stderr >&2; exit {exit_code}"),
            ],
        )
    }

    #[cfg(windows)]
    fn long_running_command() -> (std::path::PathBuf, Vec<String>) {
        (
            std::path::PathBuf::from("cmd.exe"),
            vec!["/C".to_string(), "ping -n 6 127.0.0.1 > NUL".to_string()],
        )
    }

    #[cfg(not(windows))]
    fn long_running_command() -> (std::path::PathBuf, Vec<String>) {
        (
            std::path::PathBuf::from("sh"),
            vec!["-c".to_string(), "sleep 5".to_string()],
        )
    }

    #[tokio::test]
    async fn tokio_runner_returns_output_for_abnormal_exit() {
        let runner = TokioMediaProcessRunner::default();
        let (program, args) = process_command(7);
        let output = runner
            .run(&program, &args, CancellationToken::new())
            .await
            .expect("an exited process should return its captured output");

        assert!(!output.success, "non-zero exit must not report success");
        assert!(String::from_utf8_lossy(&output.stdout).contains("stdout"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("stderr"));
    }

    #[tokio::test]
    async fn tokio_runner_returns_cancelled_when_token_is_cancelled() {
        let runner = TokioMediaProcessRunner::default();
        let (program, args) = long_running_command();
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        assert!(matches!(
            runner.run(&program, &args, cancellation).await,
            Err(VideoError::Cancelled)
        ));
    }

    #[tokio::test]
    async fn tokio_runner_times_out_a_stuck_process() {
        let runner = TokioMediaProcessRunner::with_timeout(Duration::from_millis(25));
        let (program, args) = long_running_command();

        assert!(matches!(
            runner.run(&program, &args, CancellationToken::new()).await,
            Err(VideoError::Timeout { .. })
        ));
    }

    #[test]
    fn failure_policy_matches_resume_contract() {
        assert_eq!(
            classify_video_failure(&VideoError::Cancelled),
            MediaFailureClass::Paused
        );
        assert_eq!(
            classify_video_failure(&VideoError::CorruptedVideo),
            MediaFailureClass::Failed
        );
        assert_eq!(
            classify_video_failure(&VideoError::FfmpegNotFound),
            MediaFailureClass::Recoverable
        );
        assert_eq!(
            classify_video_failure(&VideoError::ProcessingError {
                message: "Output validation failed: bad_dimensions".to_string(),
            }),
            MediaFailureClass::Failed
        );
    }
}
