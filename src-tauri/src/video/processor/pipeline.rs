#![allow(dead_code)]
use anyhow::Context;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command as TokioCommand;
use tracing::{info, warn};

use super::super::{execute_ffmpeg_command, Result, VideoError};
use super::types::{ClipSpec, ComposeOptions, LoudnessInfo, TransitionType, VideoEncoder};
use crate::utils::ffmpeg::{get_ffmpeg_path, get_ffprobe_path};
use crate::utils::process::command_output_with_timeout;

const VIDEO_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const ENCODER_DETECTION_TIMEOUT: Duration = Duration::from_secs(10);

/// Bitrate ceiling for offline final export / composition (1080p60 H.264).
/// A safety cap that rarely binds at CRF/CQ 23 but bounds worst-case bitrate on
/// high-motion content. NOT applied to intermediate extract passes or realtime
/// recording.
const BITRATE_CAP_ARGS: &[&str] = &["-maxrate", "16M", "-bufsize", "32M"];

/// Process-wide cache of the detected optimal encoder, so we do not re-run
/// `nvidia-smi` / `ffmpeg -encoders` on every command invocation.
static OPTIMAL_ENCODER: std::sync::OnceLock<VideoEncoder> = std::sync::OnceLock::new();

/// FFmpeg video processor for clip extraction and composition
pub struct VideoProcessor {
    pub(super) ffmpeg_path: String,
    pub(super) optimal_encoder: VideoEncoder,
}

impl VideoProcessor {
    pub fn new() -> Result<Self> {
        let optimal_encoder = Self::detect_optimal_encoder();
        tracing::info!(
            encoder = %optimal_encoder.get_name(),
            "Detected optimal encoder"
        );

        let ffmpeg_path = get_ffmpeg_path().map_err(|e| VideoError::ProcessingError {
            message: format!("Failed to find FFmpeg: {}", e),
        })?;

        Ok(Self {
            ffmpeg_path: ffmpeg_path.to_string_lossy().to_string(),
            optimal_encoder,
        })
    }

    /// Create a new VideoProcessor with fallback to default settings
    pub fn new_with_fallback() -> Self {
        match Self::new() {
            Ok(processor) => processor,
            Err(e) => {
                warn!(
                    "Failed to create VideoProcessor with optimal settings: {}. Using fallback.",
                    e
                );
                Self {
                    ffmpeg_path: "ffmpeg".to_string(),
                    optimal_encoder: VideoEncoder::H264,
                }
            }
        }
    }

    /// Detect the optimal hardware-accelerated encoder available on the system.
    ///
    /// The result is cached process-wide: hardware probing spawns
    /// `nvidia-smi` / `ffmpeg -encoders` subprocesses which are wasteful to
    /// repeat on every video command.
    pub fn detect_optimal_encoder() -> VideoEncoder {
        OPTIMAL_ENCODER
            .get_or_init(Self::detect_optimal_encoder_uncached)
            .clone()
    }

    fn detect_optimal_encoder_uncached() -> VideoEncoder {
        if Self::check_nvidia_availability() {
            return VideoEncoder::NvidiaH264;
        }

        if Self::check_amd_availability() {
            return VideoEncoder::AmdH264;
        }

        if Self::check_intel_availability() {
            return VideoEncoder::IntelH264;
        }

        info!("No hardware acceleration detected, using software encoding");
        VideoEncoder::H264
    }

    fn check_nvidia_availability() -> bool {
        #[cfg(target_os = "windows")]
        {
            let mut command = std::process::Command::new("nvidia-smi");
            command.arg("-L");
            if let Ok(output) =
                command_output_with_timeout(command, ENCODER_DETECTION_TIMEOUT, "nvidia-smi probe")
            {
                if output.status.success() {
                    return !output.stdout.is_empty();
                }
            }
        }

        let mut command = Self::ffmpeg_probe_command();
        command.args(["-hide_banner", "-encoders"]);
        command_output_with_timeout(command, ENCODER_DETECTION_TIMEOUT, "FFmpeg encoder probe")
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("h264_nvenc"))
            .unwrap_or(false)
    }

    fn check_amd_availability() -> bool {
        let mut command = Self::ffmpeg_probe_command();
        command.args(["-hide_banner", "-encoders"]);
        command_output_with_timeout(command, ENCODER_DETECTION_TIMEOUT, "FFmpeg encoder probe")
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("h264_amf"))
            .unwrap_or(false)
    }

    fn check_intel_availability() -> bool {
        let mut command = Self::ffmpeg_probe_command();
        command.args(["-hide_banner", "-encoders"]);
        command_output_with_timeout(command, ENCODER_DETECTION_TIMEOUT, "FFmpeg encoder probe")
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("h264_qsv"))
            .unwrap_or(false)
    }

    fn ffmpeg_probe_command() -> std::process::Command {
        let ffmpeg_path = get_ffmpeg_path().unwrap_or_else(|_| PathBuf::from("ffmpeg"));
        std::process::Command::new(ffmpeg_path)
    }

    pub fn get_optimal_encoder(&self) -> &VideoEncoder {
        &self.optimal_encoder
    }

    async fn execute_ffmpeg_with_software_fallback<F>(
        &self,
        operation: &str,
        output: &Path,
        primary_encoder: &VideoEncoder,
        mut build_command: F,
    ) -> Result<()>
    where
        F: FnMut(&VideoEncoder) -> Result<TokioCommand>,
    {
        let mut command = build_command(primary_encoder)?;
        match execute_ffmpeg_command(&mut command).await {
            Ok(()) => {
                crate::video::get_global_stats().record_success();
                Ok(())
            }
            Err(primary_error) if primary_encoder.is_hardware_accelerated() => {
                warn!(
                    "{} failed with {}: {}. Retrying with H.264 software encoder.",
                    operation,
                    primary_encoder.get_name(),
                    primary_error
                );
                crate::video::get_global_stats().record_failure();
                let _ = tokio::fs::remove_file(output).await;

                let fallback_encoder = VideoEncoder::H264;
                let mut fallback_command = build_command(&fallback_encoder)?;
                match execute_ffmpeg_command(&mut fallback_command).await {
                    Ok(()) => {
                        crate::video::get_global_stats().record_success();
                        Ok(())
                    }
                    Err(fallback_error) => {
                        crate::video::get_global_stats().record_failure();
                        Err(VideoError::ProcessingError {
                            message: format!(
                                "{} failed with {} and software fallback also failed: {}",
                                operation, primary_error, fallback_error
                            ),
                        })
                    }
                }
            }
            Err(error) => {
                crate::video::get_global_stats().record_failure();
                Err(error)
            }
        }
    }

    /// Extract a clip from a video file
    pub async fn extract_clip(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        start_time: f64,
        duration: f64,
    ) -> Result<PathBuf> {
        let input = input_path.as_ref();
        let output = output_path.as_ref();

        info!(
            "Extracting clip: {:?} -> {:?} (start: {}s, duration: {}s)",
            input, output, start_time, duration
        );

        if !input.exists() {
            return Err(VideoError::FileNotFound {
                path: input.display().to_string(),
            });
        }

        if let Some(parent) = output.parent() {
            if !parent.exists() {
                return Err(VideoError::OutputDirectoryNotFound {
                    path: parent.display().to_string(),
                });
            }
        }

        let primary_encoder = if duration < 300.0 {
            self.optimal_encoder.clone()
        } else {
            VideoEncoder::H264
        };

        self.execute_ffmpeg_with_software_fallback(
            "clip extraction",
            output,
            &primary_encoder,
            |encoder| {
                let mut command = TokioCommand::new(&self.ffmpeg_path);

                command.arg("-ss");
                command.arg(format!("{:.3}", start_time));

                if duration < 300.0 {
                    // -hwaccel auto lets FFmpeg pick the best available decoder (cuda/dxva2/qsv)
                    // without hard-coding a backend that may not be present at runtime
                    command.args(["-hwaccel", "auto"]);
                }

                command.arg("-i");
                command.arg(input.to_str().ok_or_else(|| VideoError::FileAccessError {
                    path: input.display().to_string(),
                })?);

                command.arg("-t");
                command.arg(format!("{:.3}", duration));

                if duration < 300.0 {
                    let encoder_args = encoder.get_ffmpeg_args();
                    command.args(&encoder_args);

                    match encoder {
                        VideoEncoder::NvidiaH264 | VideoEncoder::NvidiaH265 => {
                            command.args(["-preset", "fast"]);
                        }
                        VideoEncoder::IntelH264 => {
                            command.args(["-preset", "medium"]);
                        }
                        _ => {}
                    }

                    // Quality is already encoded in the per-encoder args
                    // (libx264 `-crf`, nvenc `-cq`, amf `-qp`, qsv
                    // `-global_quality`). Appending a blanket `-crf` here would
                    // hard-fail hardware encoders, so we only add threading and
                    // the compatibility pixel format.
                    command.args(["-threads", "0", "-pix_fmt", "yuv420p"]);
                } else {
                    command.args(["-c", "copy"]);
                }

                command.args([
                    "-avoid_negative_ts",
                    "make_zero",
                    "-movflags",
                    "+faststart",
                    "-y",
                    output.to_str().ok_or_else(|| VideoError::FileAccessError {
                        path: output.display().to_string(),
                    })?,
                ]);

                Ok(command)
            },
        )
        .await?;

        if !output.exists() {
            return Err(VideoError::ProcessingError {
                message: format!("Output file was not created: {:?}", output),
            });
        }

        info!("Clip extracted successfully: {:?}", output);
        Ok(output.to_path_buf())
    }

    /// Generate an FFmpeg zoompan filter that applies zoom in/out at event timestamps.
    ///
    /// At each event time the zoom ramps from 1.0 to 1.3x over 0.3s (ease-in),
    /// holds at 1.3x for 0.5s, then ramps back to 1.0x over 0.3s (ease-out).
    /// Between events the zoom stays at 1.0x.
    ///
    /// The returned string is a complete `zoompan=...` filter ready for `-vf`.
    pub fn generate_event_zoom_filter(
        event_times: &[f64],
        fps: u32,
        width: u32,
        height: u32,
    ) -> String {
        if event_times.is_empty() {
            return format!("zoompan=z=1:d=1:s={}x{}:fps={}", width, height, fps);
        }

        // Build a nested if() expression for the zoom value.
        // Each event at time T produces three phases:
        //   [T, T+0.3)       ease-in:  1 + 0.3*(in_time-T)/0.3
        //   [T+0.3, T+0.8)   hold:     1.3
        //   [T+0.8, T+1.1)   ease-out: 1.3 - 0.3*(in_time-T-0.8)/0.3
        let mut expr = String::from("1");

        for t in event_times.iter().copied() {
            let t_in_end = t + 0.3;
            let t_hold_end = t + 0.8;
            let t_out_end = t + 1.1;

            // Build from outermost (ease-in) to innermost, wrapping the
            // previous expression as the "else" branch each time.
            // Phase 3: ease-out
            expr = format!(
                "if(between(in_time\\,{:.3}\\,{:.3})\\,1.3-0.3*(in_time-{:.3})/0.3\\,{})",
                t_hold_end, t_out_end, t_hold_end, expr
            );
            // Phase 2: hold
            expr = format!(
                "if(between(in_time\\,{:.3}\\,{:.3})\\,1.3\\,{})",
                t_in_end, t_hold_end, expr
            );
            // Phase 1: ease-in
            expr = format!(
                "if(between(in_time\\,{:.3}\\,{:.3})\\,1+0.3*(in_time-{:.3})/0.3\\,{})",
                t, t_in_end, t, expr
            );
        }

        format!(
            "zoompan=z='{}':d=1:s={}x{}:fps={}",
            expr, width, height, fps
        )
    }

    /// Compose multiple clips into a YouTube Short (9:16 aspect ratio)
    ///
    /// If `event_times` is provided, a zoom effect is applied at each timestamp.
    pub async fn compose_shorts(
        &self,
        clip_paths: &[PathBuf],
        output_path: impl AsRef<Path>,
        target_width: u32,
        target_height: u32,
    ) -> Result<PathBuf> {
        self.compose_shorts_with_zoom(clip_paths, output_path, target_width, target_height, None)
            .await
    }

    /// Compose multiple clips into a YouTube Short with optional event-based zoom.
    pub async fn compose_shorts_with_zoom(
        &self,
        clip_paths: &[PathBuf],
        output_path: impl AsRef<Path>,
        target_width: u32,
        target_height: u32,
        event_times: Option<&[f64]>,
    ) -> Result<PathBuf> {
        let output = output_path.as_ref();

        if clip_paths.is_empty() {
            return Err(VideoError::ProcessingError {
                message: "No clips provided for composition".to_string(),
            });
        }

        info!(
            "Composing {} clips into Short: {:?} ({}x{})",
            clip_paths.len(),
            output,
            target_width,
            target_height
        );

        for clip in clip_paths {
            if !clip.exists() {
                return Err(VideoError::FileNotFound {
                    path: clip.display().to_string(),
                });
            }
        }

        if let Some(parent) = output.parent() {
            if !parent.exists() {
                return Err(VideoError::OutputDirectoryNotFound {
                    path: parent.display().to_string(),
                });
            }
        }

        if clip_paths.len() == 1 {
            return self
                .scale_and_crop_clip_with_zoom(
                    &clip_paths[0],
                    output,
                    target_width,
                    target_height,
                    event_times,
                )
                .await;
        }

        let concat_file = output
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("concat_list.txt");

        let mut concat_content = String::new();
        for path in clip_paths {
            let path_str = path.to_str().ok_or_else(|| VideoError::FileAccessError {
                path: path.display().to_string(),
            })?;
            concat_content.push_str(&format!("file '{}'\n", path_str));
        }

        tokio::fs::write(&concat_file, concat_content)
            .await
            .map_err(|e| VideoError::ProcessingError {
                message: format!("Failed to write concat file: {}", e),
            })?;

        let base_filter = format!(
            "scale=-1:{}:force_original_aspect_ratio=increase,crop={}:{},setsar=1",
            target_height, target_width, target_height
        );
        let vf = match event_times {
            Some(times) if !times.is_empty() => {
                let zoom = Self::generate_event_zoom_filter(times, 30, target_width, target_height);
                format!("{},{}", base_filter, zoom)
            }
            _ => base_filter,
        };

        let result = self
            .execute_ffmpeg_with_software_fallback(
                "short composition",
                output,
                &self.optimal_encoder,
                |encoder| {
                    let mut command = TokioCommand::new(&self.ffmpeg_path);
                    command.args([
                        "-f",
                        "concat",
                        "-safe",
                        "0",
                        "-i",
                        concat_file
                            .to_str()
                            .ok_or_else(|| VideoError::FileAccessError {
                                path: concat_file.display().to_string(),
                            })?,
                        "-vf",
                        &vf,
                    ]);
                    for arg in encoder.get_ffmpeg_args() {
                        command.arg(arg);
                    }
                    command.args(BITRATE_CAP_ARGS);
                    command.args([
                        "-c:a",
                        "aac",
                        "-b:a",
                        "192k",
                        // faststart moves moov atom to front for faster YouTube streaming start
                        "-movflags",
                        "+faststart",
                        // prevents muxer overflow when audio/video packet queues diverge
                        "-max_muxing_queue_size",
                        "1024",
                        "-y",
                        output.to_str().ok_or_else(|| VideoError::FileAccessError {
                            path: output.display().to_string(),
                        })?,
                    ]);

                    Ok(command)
                },
            )
            .await;
        let _ = tokio::fs::remove_file(&concat_file).await;

        result.map_err(|e| VideoError::ConcatenationError {
            reason: e.to_string(),
        })?;

        if !output.exists() {
            return Err(VideoError::ProcessingError {
                message: format!("Output file was not created: {:?}", output),
            });
        }

        info!("Short composed successfully: {:?}", output);
        Ok(output.to_path_buf())
    }

    /// Compose multiple clips into a long-form montage (16:9 aspect ratio)
    pub async fn compose_montage(
        &self,
        clip_paths: &[PathBuf],
        output_path: impl AsRef<Path>,
        use_transition: bool,
    ) -> Result<PathBuf> {
        let output = output_path.as_ref();

        if clip_paths.is_empty() {
            return Err(VideoError::ProcessingError {
                message: "No clips provided for montage".to_string(),
            });
        }

        info!(
            "Composing {} clips into Montage: {:?} (16:9)",
            clip_paths.len(),
            output
        );

        for clip in clip_paths {
            if !clip.exists() {
                return Err(VideoError::FileNotFound {
                    path: clip.display().to_string(),
                });
            }
        }

        if let Some(parent) = output.parent() {
            if !parent.exists() {
                return Err(VideoError::OutputDirectoryNotFound {
                    path: parent.display().to_string(),
                });
            }
        }

        if use_transition && clip_paths.len() >= 2 {
            info!(
                "Applying fade transitions between {} clips",
                clip_paths.len()
            );

            let temp_dir = output
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();

            // Chain pairs: transition(clip[0], clip[1]) -> tmp0, transition(tmp0, clip[2]) -> tmp1, ...
            let mut current: PathBuf = clip_paths[0].clone();
            let mut temp_files: Vec<PathBuf> = Vec::new();

            for (i, next_clip) in clip_paths[1..].iter().enumerate() {
                let is_last = i == clip_paths.len() - 2;
                let temp_out = if is_last {
                    output.to_path_buf()
                } else {
                    temp_dir.join(format!("montage_transition_tmp_{}.mp4", i))
                };

                self.apply_transition(&current, next_clip, &temp_out, TransitionType::Fade, 0.5)
                    .await
                    .map_err(|e| VideoError::ProcessingError {
                        message: format!("Transition failed at clip {}: {}", i, e),
                    })?;

                if !is_last {
                    temp_files.push(temp_out.clone());
                    current = temp_out;
                }
            }

            // Clean up intermediate temp files
            for tmp in &temp_files {
                let _ = tokio::fs::remove_file(tmp).await;
            }

            info!(
                "Montage with transitions composed successfully: {:?}",
                output
            );
            return Ok(output.to_path_buf());
        }

        let concat_file = output
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("montage_list.txt");

        let mut concat_content = String::new();
        for path in clip_paths {
            let path_str = path.to_str().ok_or_else(|| VideoError::FileAccessError {
                path: path.display().to_string(),
            })?;
            concat_content.push_str(&format!("file '{}'\n", path_str));
        }

        tokio::fs::write(&concat_file, concat_content)
            .await
            .map_err(|e| VideoError::ProcessingError {
                message: format!("Failed to write concat file: {}", e),
            })?;

        let concat_file_str = concat_file
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid concat file path (non-UTF8 characters)"))?;
        let output_str = output
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid output path (non-UTF8 characters)"))?;

        let mut command = TokioCommand::new(&self.ffmpeg_path);
        command.args([
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            concat_file_str,
            "-c",
            "copy",
            "-y",
            output_str,
        ]);

        let result = execute_ffmpeg_command(&mut command).await;
        let _ = tokio::fs::remove_file(&concat_file).await;

        if let Err(e) = result {
            warn!("Stream copy failed for montage, trying re-encode: {}", e);

            let mut concat_content = String::new();
            for path in clip_paths {
                let path_str = path.to_str().ok_or_else(|| {
                    anyhow::anyhow!("Invalid clip path (non-UTF8 characters): {:?}", path)
                })?;
                concat_content.push_str(&format!("file '{}'\n", path_str));
            }
            tokio::fs::write(&concat_file, &concat_content)
                .await
                .context("Failed to write concat file for re-encode")?;

            let mut fallback_cmd = TokioCommand::new(&self.ffmpeg_path);
            fallback_cmd.args(["-f", "concat", "-safe", "0", "-i", concat_file_str]);
            for arg in self.optimal_encoder.get_ffmpeg_args() {
                fallback_cmd.arg(arg);
            }
            fallback_cmd.args(BITRATE_CAP_ARGS);
            fallback_cmd.args([
                "-c:a",
                "aac",
                // faststart moves moov atom to front for faster YouTube streaming start
                "-movflags",
                "+faststart",
                // prevents muxer overflow when audio/video packet queues diverge
                "-max_muxing_queue_size",
                "1024",
                "-y",
                output_str,
            ]);

            execute_ffmpeg_command(&mut fallback_cmd).await?;
            let _ = tokio::fs::remove_file(&concat_file).await;
        }

        info!("Montage composed successfully: {:?}", output);
        Ok(output.to_path_buf())
    }

    /// Scale and crop a single clip to target dimensions (9:16)
    pub(super) async fn scale_and_crop_clip(
        &self,
        input: &Path,
        output: &Path,
        target_width: u32,
        target_height: u32,
    ) -> Result<PathBuf> {
        info!(
            "Scaling and cropping clip: {:?} -> {:?} ({}x{})",
            input, output, target_width, target_height
        );

        let filter = format!(
            "scale=-1:{}:force_original_aspect_ratio=increase,crop={}:{},setsar=1",
            target_height, target_width, target_height
        );

        self.execute_ffmpeg_with_software_fallback(
            "single clip scaling",
            output,
            &self.optimal_encoder,
            |encoder| {
                let mut command = TokioCommand::new(&self.ffmpeg_path);
                command.args([
                    "-i",
                    input.to_str().ok_or_else(|| VideoError::FileAccessError {
                        path: input.display().to_string(),
                    })?,
                    "-vf",
                    &filter,
                ]);
                for arg in encoder.get_ffmpeg_args() {
                    command.arg(arg);
                }
                command.args(BITRATE_CAP_ARGS);
                command.args([
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                    // faststart moves moov atom to front for faster YouTube streaming start
                    "-movflags",
                    "+faststart",
                    // prevents muxer overflow when audio/video packet queues diverge
                    "-max_muxing_queue_size",
                    "1024",
                    "-y",
                    output.to_str().ok_or_else(|| VideoError::FileAccessError {
                        path: output.display().to_string(),
                    })?,
                ]);

                Ok(command)
            },
        )
        .await?;
        Ok(output.to_path_buf())
    }

    /// Scale and crop a single clip with optional event-based zoom effect.
    pub(super) async fn scale_and_crop_clip_with_zoom(
        &self,
        input: &Path,
        output: &Path,
        target_width: u32,
        target_height: u32,
        event_times: Option<&[f64]>,
    ) -> Result<PathBuf> {
        let base_filter = format!(
            "scale=-1:{}:force_original_aspect_ratio=increase,crop={}:{},setsar=1",
            target_height, target_width, target_height
        );
        let filter = match event_times {
            Some(times) if !times.is_empty() => {
                let zoom = Self::generate_event_zoom_filter(times, 30, target_width, target_height);
                format!("{},{}", base_filter, zoom)
            }
            _ => base_filter,
        };

        info!(
            "Scaling/cropping clip with zoom: {:?} -> {:?} ({}x{})",
            input, output, target_width, target_height
        );

        self.execute_ffmpeg_with_software_fallback(
            "single clip scaling with zoom",
            output,
            &self.optimal_encoder,
            |encoder| {
                let mut command = TokioCommand::new(&self.ffmpeg_path);
                command.args([
                    "-i",
                    input.to_str().ok_or_else(|| VideoError::FileAccessError {
                        path: input.display().to_string(),
                    })?,
                    "-vf",
                    &filter,
                ]);
                for arg in encoder.get_ffmpeg_args() {
                    command.arg(arg);
                }
                command.args(BITRATE_CAP_ARGS);
                command.args([
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                    "-movflags",
                    "+faststart",
                    "-max_muxing_queue_size",
                    "1024",
                    "-y",
                    output.to_str().ok_or_else(|| VideoError::FileAccessError {
                        path: output.display().to_string(),
                    })?,
                ]);

                Ok(command)
            },
        )
        .await?;
        Ok(output.to_path_buf())
    }

    /// Task 32: Detect source video dimensions via ffprobe.
    ///
    /// Returns `(width, height)` of the first video stream.
    pub async fn get_video_dimensions(&self, input_path: &Path) -> Result<(u32, u32)> {
        if !input_path.exists() {
            return Err(VideoError::FileNotFound {
                path: input_path.display().to_string(),
            });
        }

        let ffprobe_path = get_ffprobe_path().map_err(|_| VideoError::FfprobeNotFound)?;
        let mut command = TokioCommand::new(ffprobe_path);
        command.args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
            input_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: input_path.display().to_string(),
                })?,
        ]);
        command.kill_on_drop(true);

        let output = tokio::time::timeout(VIDEO_PROBE_TIMEOUT, command.output())
            .await
            .map_err(|_| VideoError::Timeout {
                timeout_secs: VIDEO_PROBE_TIMEOUT.as_secs(),
            })?
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    VideoError::FfprobeNotFound
                } else {
                    VideoError::ProcessingError {
                        message: format!("Failed to execute ffprobe: {}", e),
                    }
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VideoError::from_ffmpeg_stderr(&stderr));
        }

        let dim_str = String::from_utf8_lossy(&output.stdout);
        let dim_str = dim_str.trim();
        // Expected format: "1920x1080"
        let parts: Vec<&str> = dim_str.splitn(2, 'x').collect();
        if parts.len() != 2 {
            return Err(VideoError::ProcessingError {
                message: format!("Unexpected ffprobe dimension output: '{}'", dim_str),
            });
        }
        let width = parts[0]
            .parse::<u32>()
            .map_err(|e| VideoError::ProcessingError {
                message: format!("Failed to parse width '{}': {}", parts[0], e),
            })?;
        let height = parts[1]
            .parse::<u32>()
            .map_err(|e| VideoError::ProcessingError {
                message: format!("Failed to parse height '{}': {}", parts[1], e),
            })?;

        info!("Detected video dimensions: {}x{}", width, height);
        Ok((width, height))
    }

    /// Task 32: Generate a precise center-crop filter string for 9:16 (Shorts) from a 16:9 source.
    ///
    /// Computes exact pixel values so FFmpeg does not need to evaluate expressions at runtime.
    /// `target_width` and `target_height` define the desired output resolution (e.g. 1080x1920).
    ///
    /// This is a utility intended for future integration into `compose_shorts` and
    /// `scale_and_crop_clip` to replace the inline scale/crop filter strings with
    /// source-dimension-aware exact-pixel crops (avoids rounding artifacts on odd resolutions).
    pub fn generate_shorts_crop_filter(
        src_width: u32,
        src_height: u32,
        target_width: u32,
        target_height: u32,
    ) -> String {
        // We want to produce target_width x target_height (9:16) from a wider source.
        // Strategy:
        //   1. Scale so height matches target_height (preserving aspect ratio).
        //   2. Center-crop to target_width x target_height.
        //
        // Crop dimensions in source pixels that yield target_width:target_height ratio:
        //   crop_h = src_height
        //   crop_w = src_height * target_width / target_height
        //   x_offset = (src_width - crop_w) / 2
        //   y_offset = 0

        let crop_w = (src_height as f64 * target_width as f64 / target_height as f64) as u32;
        let crop_h = src_height;
        let x_offset = (src_width.saturating_sub(crop_w)) / 2;
        let y_offset = 0u32;

        // After crop, scale to exact target dimensions
        format!(
            "crop={}:{}:{}:{},scale={}:{}",
            crop_w, crop_h, x_offset, y_offset, target_width, target_height
        )
    }

    /// Get video duration in seconds
    pub async fn get_duration(&self, input_path: impl AsRef<Path>) -> Result<f64> {
        let input = input_path.as_ref();

        if !input.exists() {
            return Err(VideoError::FileNotFound {
                path: input.display().to_string(),
            });
        }

        let ffprobe_path = get_ffprobe_path().map_err(|_| VideoError::FfprobeNotFound)?;
        let mut command = TokioCommand::new(ffprobe_path);
        command.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            input.to_str().ok_or_else(|| VideoError::FileAccessError {
                path: input.display().to_string(),
            })?,
        ]);
        command.kill_on_drop(true);

        let output = tokio::time::timeout(VIDEO_PROBE_TIMEOUT, command.output())
            .await
            .map_err(|_| VideoError::Timeout {
                timeout_secs: VIDEO_PROBE_TIMEOUT.as_secs(),
            })?
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    VideoError::FfprobeNotFound
                } else {
                    VideoError::ProcessingError {
                        message: format!("Failed to execute ffprobe: {}", e),
                    }
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VideoError::from_ffmpeg_stderr(&stderr));
        }

        let duration_str = String::from_utf8_lossy(&output.stdout);
        let duration =
            duration_str
                .trim()
                .parse::<f64>()
                .map_err(|e| VideoError::ProcessingError {
                    message: format!("Failed to parse duration: {}", e),
                })?;

        Ok(duration)
    }
}

// ============================================================================
// Audio Normalization (LUFS-based two-pass loudnorm)
// ============================================================================

impl VideoProcessor {
    /// Parse loudnorm JSON output from FFmpeg stderr.
    ///
    /// FFmpeg prints the loudnorm stats as a JSON block in stderr when using
    /// `loudnorm=print_format=json`. This function extracts and parses it.
    pub fn parse_loudnorm_output(stderr: &str) -> std::result::Result<LoudnessInfo, VideoError> {
        // The JSON block printed by loudnorm looks like:
        // {
        //     "input_i" : "-24.05",
        //     "input_tp" : "-2.14",
        //     "input_lra" : "10.20",
        //     "input_thresh" : "-34.67",
        //     ...
        // }
        let json_start = stderr
            .rfind("{\n")
            .or_else(|| stderr.rfind("{\r\n"))
            .ok_or_else(|| VideoError::ProcessingError {
                message: "Failed to find loudnorm JSON in FFmpeg output".to_string(),
            })?;

        let json_end =
            stderr[json_start..]
                .find('}')
                .ok_or_else(|| VideoError::ProcessingError {
                    message: "Failed to find end of loudnorm JSON in FFmpeg output".to_string(),
                })?
                + json_start
                + 1;

        let json_str = &stderr[json_start..json_end];

        let parsed: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| VideoError::ProcessingError {
                message: format!("Failed to parse loudnorm JSON: {}", e),
            })?;

        let parse_field = |field: &str| -> std::result::Result<f64, VideoError> {
            parsed
                .get(field)
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .ok_or_else(|| VideoError::ProcessingError {
                    message: format!("Missing or invalid '{}' in loudnorm output", field),
                })
        };

        Ok(LoudnessInfo {
            input_i: parse_field("input_i")?,
            input_tp: parse_field("input_tp")?,
            input_lra: parse_field("input_lra")?,
            input_thresh: parse_field("input_thresh")?,
        })
    }

    /// Pass 1: Analyze audio levels and return loudnorm parameters.
    ///
    /// Runs FFmpeg with `-af loudnorm=print_format=json -f null -` to measure
    /// integrated loudness, true peak, loudness range, and threshold.
    async fn analyze_audio_loudness(&self, input_path: &Path) -> Result<LoudnessInfo> {
        use tokio::io::AsyncReadExt;

        if !input_path.exists() {
            return Err(VideoError::FileNotFound {
                path: input_path.display().to_string(),
            });
        }

        let input_str = input_path
            .to_str()
            .ok_or_else(|| VideoError::FileAccessError {
                path: input_path.display().to_string(),
            })?;

        info!("Analyzing audio loudness: {:?}", input_path);

        let mut command = TokioCommand::new(&self.ffmpeg_path);
        command.args([
            "-i",
            input_str,
            "-af",
            "loudnorm=print_format=json",
            "-f",
            "null",
            "-",
        ]);
        command.stderr(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::null());
        command.kill_on_drop(true);

        let mut child = command.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                VideoError::FfmpegNotFound
            } else {
                VideoError::ProcessingError {
                    message: format!("Failed to spawn FFmpeg for loudness analysis: {}", e),
                }
            }
        })?;

        let mut stderr_output = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            stderr.read_to_string(&mut stderr_output).await.ok();
        }

        let status = child
            .wait()
            .await
            .map_err(|e| VideoError::ProcessingError {
                message: format!("Failed to wait for FFmpeg loudness analysis: {}", e),
            })?;

        if !status.success() {
            return Err(VideoError::FfmpegProcessError {
                message: "Loudness analysis failed".to_string(),
                stderr: stderr_output,
            });
        }

        let loudness = Self::parse_loudnorm_output(&stderr_output)?;
        info!(
            "Audio analysis complete: I={:.1} LUFS, TP={:.1} dBTP, LRA={:.1} LU",
            loudness.input_i, loudness.input_tp, loudness.input_lra
        );

        Ok(loudness)
    }

    /// Two-pass LUFS-based audio normalization.
    ///
    /// Pass 1 analyzes current loudness levels. Pass 2 applies the loudnorm
    /// filter with measured values for sample-accurate normalization.
    ///
    /// `target_lufs` is typically -14.0 for YouTube or -16.0 for podcasts.
    pub async fn normalize_audio(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        target_lufs: f64,
    ) -> Result<PathBuf> {
        let input = input_path.as_ref();
        let output = output_path.as_ref();

        if !input.exists() {
            return Err(VideoError::FileNotFound {
                path: input.display().to_string(),
            });
        }

        if let Some(parent) = output.parent() {
            if !parent.exists() {
                return Err(VideoError::OutputDirectoryNotFound {
                    path: parent.display().to_string(),
                });
            }
        }

        // Pass 1: Analyze
        let loudness = self.analyze_audio_loudness(input).await?;

        info!(
            "Normalizing audio: {:.1} LUFS -> {:.1} LUFS",
            loudness.input_i, target_lufs
        );

        // Pass 2: Apply normalization with measured values
        let loudnorm_filter = format!(
            "loudnorm=I={:.1}:TP=-1.5:LRA=11:measured_I={:.2}:measured_TP={:.2}:measured_LRA={:.2}:measured_thresh={:.2}:linear=true",
            target_lufs,
            loudness.input_i,
            loudness.input_tp,
            loudness.input_lra,
            loudness.input_thresh,
        );

        let mut command = TokioCommand::new(&self.ffmpeg_path);
        command.args([
            "-i",
            input.to_str().ok_or_else(|| VideoError::FileAccessError {
                path: input.display().to_string(),
            })?,
            "-af",
            &loudnorm_filter,
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            "-y",
            output.to_str().ok_or_else(|| VideoError::FileAccessError {
                path: output.display().to_string(),
            })?,
        ]);

        execute_ffmpeg_command(&mut command).await?;

        if !output.exists() {
            return Err(VideoError::ProcessingError {
                message: format!("Normalized output file was not created: {:?}", output),
            });
        }

        info!("Audio normalization complete: {:?}", output);
        Ok(output.to_path_buf())
    }
}

impl Default for VideoProcessor {
    fn default() -> Self {
        Self::new_with_fallback()
    }
}

// ============================================================================
// Editor composition with per-clip trim, transitions, zoom, and normalization
// ============================================================================

impl VideoProcessor {
    /// Build the `-filter_complex` graph for `compose_with_options`.
    ///
    /// Inputs are assumed already trimmed at the input level (`-ss`/`-t`), so
    /// `[i:v]`/`[i:a]` are the trimmed segments. Produces final labels
    /// `[outv]` and `[outa]`.
    ///
    /// `has_audio[i]` marks whether input `i` actually carries an audio stream.
    /// For clips WITHOUT audio we never reference `[i:a]` (FFmpeg would fail with
    /// "Stream specifier :a matches no streams"); instead we synthesize stereo
    /// 48 kHz silence bounded to the clip's trimmed length so `concat` /
    /// `acrossfade` still see a finite `[a{i}]` stream. A missing entry defaults
    /// to `true` (assume audio) to preserve the previous behavior.
    ///
    /// `effective_durations` is consulted (a) when `transition` is `Some` to
    /// place xfade offsets, and (b) to bound synthesized silence for any silent
    /// clip — callers MUST populate it whenever `has_audio` contains a `false`.
    // The parameters are all distinct, independently-sourced scalars/slices that the
    // caller assembles from different places (ComposeOptions, per-clip probes, the
    // trim plan). Bundling them into a struct here would only move the same argument
    // list one level up while hiding which of them the graph actually depends on.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_compose_filtergraph(
        n: usize,
        width: u32,
        height: u32,
        fps: u32,
        transition: Option<(&str, f64)>,
        effective_durations: &[f64],
        has_audio: &[bool],
        event_times: Option<&[f64]>,
    ) -> String {
        let base_vf = format!(
            "scale=-1:{h}:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1,format=yuv420p,fps={fps}",
            h = height,
            w = width,
            fps = fps
        );
        let base_af =
            "aresample=async=1:out_sample_rate=48000,aformat=sample_fmts=fltp:channel_layouts=stereo";

        let mut parts: Vec<String> = Vec::new();
        for i in 0..n {
            parts.push(format!("[{i}:v]{base_vf}[v{i}]"));
            if has_audio.get(i).copied().unwrap_or(true) {
                parts.push(format!("[{i}:a]{base_af}[a{i}]"));
            } else {
                // No audio stream on this input: fabricate silence of the clip's
                // trimmed duration. anullsrc emits infinite silence, so atrim is
                // mandatory — an unbounded [a{i}] would make concat/acrossfade
                // wait forever on this segment. base_af then normalizes it to the
                // same fltp/stereo/48k the real-audio branch produces.
                let dur = effective_durations.get(i).copied().unwrap_or(0.0);
                parts.push(format!(
                    "anullsrc=channel_layout=stereo:sample_rate=48000,atrim=duration={dur:.3},asetpts=PTS-STARTPTS,{base_af}[a{i}]"
                ));
            }
        }

        let (video_label, audio_label) = if n == 1 {
            ("v0".to_string(), "a0".to_string())
        } else if let Some((kind, d)) = transition {
            let xkind = Self::map_transition_kind(kind);
            // Video xfade chain: offset_k = sum(dur[0..=k]) - (k+1)*d.
            let mut prev = "v0".to_string();
            let mut cum = effective_durations.first().copied().unwrap_or(0.0);
            for i in 1..n {
                let out = if i == n - 1 {
                    "vx_final".to_string()
                } else {
                    format!("vx{i}")
                };
                let offset = (cum - d).max(0.0);
                parts.push(format!(
                    "[{prev}][v{i}]xfade=transition={xkind}:duration={d}:offset={offset:.3}[{out}]"
                ));
                prev = out;
                cum += effective_durations.get(i).copied().unwrap_or(0.0) - d;
            }
            // Audio crossfade chain (stays in sync: both overlap by d per join).
            let mut aprev = "a0".to_string();
            for i in 1..n {
                let out = if i == n - 1 {
                    "ax_final".to_string()
                } else {
                    format!("ax{i}")
                };
                parts.push(format!("[{aprev}][a{i}]acrossfade=d={d}[{out}]"));
                aprev = out;
            }
            ("vx_final".to_string(), "ax_final".to_string())
        } else {
            // Hard-cut concat.
            let mut concat_in = String::new();
            for i in 0..n {
                concat_in.push_str(&format!("[v{i}][a{i}]"));
            }
            parts.push(format!("{concat_in}concat=n={n}:v=1:a=1[vc][ac]"));
            ("vc".to_string(), "ac".to_string())
        };

        // Optional event-based zoom on the composed video timeline.
        match event_times {
            Some(times) if !times.is_empty() => {
                let zoom = Self::generate_event_zoom_filter(times, fps, width, height);
                parts.push(format!("[{video_label}]{zoom}[outv]"));
            }
            _ => {
                parts.push(format!("[{video_label}]null[outv]"));
            }
        }
        parts.push(format!("[{audio_label}]anull[outa]"));

        parts.join(";")
    }

    fn map_transition_kind(kind: &str) -> &'static str {
        match kind.to_lowercase().as_str() {
            "fade" => "fade",
            "slide" | "slideleft" => "slideleft",
            "dissolve" => "dissolve",
            "wipe" | "wipeleft" => "wipeleft",
            _ => "fade",
        }
    }

    /// Returns `true` iff `path` contains at least one audio stream.
    ///
    /// Uses `ffprobe -select_streams a -show_entries stream=index` (CSV): a
    /// non-empty stdout means an audio stream exists. On ANY failure (ffprobe
    /// missing, spawn error, timeout, non-zero exit, non-UTF8 path) we return
    /// `false` and treat the clip as silent — synthesizing silence is always
    /// safe, whereas a false positive would reference a missing `[i:a]` and
    /// hard-fail the whole composition.
    pub(super) async fn clip_has_audio_stream(&self, path: &Path) -> bool {
        let ffprobe_path = match get_ffprobe_path() {
            Ok(p) => p,
            Err(_) => {
                warn!("ffprobe not found; treating {:?} as silent", path);
                return false;
            }
        };
        let path_str = match path.to_str() {
            Some(s) => s,
            None => {
                warn!("non-UTF8 clip path; treating as silent: {:?}", path);
                return false;
            }
        };

        let mut command = TokioCommand::new(ffprobe_path);
        command.args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
            path_str,
        ]);
        command.kill_on_drop(true);

        match tokio::time::timeout(VIDEO_PROBE_TIMEOUT, command.output()).await {
            Ok(Ok(output)) if output.status.success() => {
                !String::from_utf8_lossy(&output.stdout).trim().is_empty()
            }
            Ok(Ok(output)) => {
                warn!(
                    "ffprobe audio detection failed for {:?} (treating as silent): {}",
                    path,
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                false
            }
            Ok(Err(e)) => {
                warn!(
                    "ffprobe spawn failed for {:?} (treating as silent): {}",
                    path, e
                );
                false
            }
            Err(_) => {
                warn!(
                    "ffprobe audio detection timed out for {:?} (treating as silent)",
                    path
                );
                false
            }
        }
    }

    /// Compose editor clips into a single video in one re-encode pass.
    ///
    /// Supports per-clip input-level trimming (`ClipSpec.trim_*` → `-ss`/`-t`),
    /// aspect scaling/cropping, optional cross-clip transitions, optional
    /// event-based zoom, a bitrate ceiling, and an optional final 2-pass
    /// loudnorm — without the separate extract pass that caused generation loss.
    pub async fn compose_with_options(
        &self,
        clips: &[ClipSpec],
        output: &Path,
        opts: &ComposeOptions,
    ) -> Result<()> {
        if clips.is_empty() {
            return Err(VideoError::ProcessingError {
                message: "No clips provided for composition".to_string(),
            });
        }
        for clip in clips {
            if !clip.path.exists() {
                return Err(VideoError::FileNotFound {
                    path: clip.path.display().to_string(),
                });
            }
        }
        if let Some(parent) = output.parent() {
            if !parent.exists() {
                return Err(VideoError::OutputDirectoryNotFound {
                    path: parent.display().to_string(),
                });
            }
        }

        let fps = opts.fps.unwrap_or(60);

        // Probe each clip for an audio stream up front. Clips without audio get
        // synthesized silence in the filtergraph instead of a dangling `[i:a]`
        // reference that would hard-fail the export — and the software fallback,
        // which shares the exact same graph, would fail identically.
        let mut has_audio: Vec<bool> = Vec::with_capacity(clips.len());
        for clip in clips {
            has_audio.push(self.clip_has_audio_stream(&clip.path).await);
        }
        let any_silent = has_audio.iter().any(|&h| !h);

        let mut transition: Option<(String, f64)> = opts.transition.clone();

        // Effective per-clip durations are needed both to place xfade offsets
        // and to bound synthesized silence for any clip lacking audio, so probe
        // them whenever either applies.
        let mut effective_durations: Vec<f64> = Vec::new();
        if transition.is_some() || any_silent {
            for clip in clips {
                let dur = match clip.trim_duration {
                    Some(d) => d,
                    None => {
                        let full = self.get_duration(&clip.path).await?;
                        (full - clip.trim_start.unwrap_or(0.0)).max(0.0)
                    }
                };
                effective_durations.push(dur);
            }
        }

        if transition.is_some() {
            let min_dur = effective_durations
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
            if let Some((kind, d)) = transition.take() {
                if min_dur.is_finite() && min_dur <= d + 0.2 {
                    let safe = (min_dur - 0.2).max(0.0);
                    if safe >= 0.1 {
                        transition = Some((kind, safe));
                    } else {
                        warn!(
                            "Clip too short ({:.2}s) for transition; using hard cuts",
                            min_dur
                        );
                        transition = None;
                    }
                } else {
                    transition = Some((kind, d));
                }
            }
        }

        let transition_ref = transition.as_ref().map(|(k, d)| (k.as_str(), *d));
        let filter = Self::build_compose_filtergraph(
            clips.len(),
            opts.width,
            opts.height,
            fps,
            transition_ref,
            &effective_durations,
            &has_audio,
            opts.event_times.as_deref(),
        );

        // When normalizing we render to a temp file first, then loudnorm into
        // the final path (loudnorm copies video, so no extra generation loss).
        let compose_target: PathBuf = if opts.normalize_audio.is_some() {
            let dir = output.parent().unwrap_or_else(|| Path::new("."));
            let stem = output
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("compose");
            dir.join(format!(".{stem}.precompose.mp4"))
        } else {
            output.to_path_buf()
        };

        self.execute_ffmpeg_with_software_fallback(
            "editor composition",
            &compose_target,
            &self.optimal_encoder,
            |encoder| {
                let mut command = TokioCommand::new(&self.ffmpeg_path);
                for clip in clips {
                    if let Some(start) = clip.trim_start {
                        command.arg("-ss");
                        command.arg(format!("{:.3}", start));
                    }
                    if let Some(dur) = clip.trim_duration {
                        command.arg("-t");
                        command.arg(format!("{:.3}", dur));
                    }
                    command.arg("-i");
                    command.arg(
                        clip.path
                            .to_str()
                            .ok_or_else(|| VideoError::FileAccessError {
                                path: clip.path.display().to_string(),
                            })?,
                    );
                }
                command.args([
                    "-filter_complex",
                    &filter,
                    "-map",
                    "[outv]",
                    "-map",
                    "[outa]",
                ]);
                for arg in encoder.get_ffmpeg_args() {
                    command.arg(arg);
                }
                command.args(BITRATE_CAP_ARGS);
                command.args([
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                    "-movflags",
                    "+faststart",
                    "-max_muxing_queue_size",
                    "1024",
                    "-y",
                    compose_target
                        .to_str()
                        .ok_or_else(|| VideoError::FileAccessError {
                            path: compose_target.display().to_string(),
                        })?,
                ]);
                Ok(command)
            },
        )
        .await?;

        if !compose_target.exists() {
            return Err(VideoError::ProcessingError {
                message: format!("Composition output was not created: {:?}", compose_target),
            });
        }

        if let Some(target_lufs) = opts.normalize_audio {
            self.normalize_audio(&compose_target, output, target_lufs)
                .await?;
            let _ = tokio::fs::remove_file(&compose_target).await;
        }

        if !output.exists() {
            return Err(VideoError::ProcessingError {
                message: format!("Composition output was not created: {:?}", output),
            });
        }

        info!("Composition complete: {:?}", output);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_processor_creation() {
        let processor = VideoProcessor::new_with_fallback();
        assert!(!processor.ffmpeg_path.is_empty());
    }

    #[test]
    fn test_video_processor_creation_with_validation() {
        match VideoProcessor::new() {
            Ok(processor) => {
                assert!(!processor.ffmpeg_path.is_empty());
            }
            Err(_e) => {
                // Acceptable in some environments
            }
        }
    }

    #[test]
    fn test_scale_filter_generation() {
        let target_width = 1080;
        let target_height = 1920;

        let filter = format!(
            "scale=-1:{}:force_original_aspect_ratio=increase,crop={}:{},setsar=1",
            target_height, target_width, target_height
        );

        assert!(filter.contains("scale=-1:1920"));
        assert!(filter.contains("crop=1080:1920"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_extract_clip_integration() {}

    #[tokio::test]
    #[ignore]
    async fn test_compose_shorts_integration() {}

    #[tokio::test]
    #[ignore]
    async fn test_generate_thumbnail_integration() {}

    #[test]
    fn test_generate_event_zoom_filter_empty_events() {
        let filter = VideoProcessor::generate_event_zoom_filter(&[], 30, 1080, 1920);
        assert!(filter.starts_with("zoompan="));
        assert!(filter.contains("s=1080x1920"));
        assert!(filter.contains("fps=30"));
        assert!(filter.contains("z=1"));
    }

    #[test]
    fn test_generate_event_zoom_filter_single_event() {
        let filter = VideoProcessor::generate_event_zoom_filter(&[5.0], 30, 1080, 1920);
        assert!(filter.starts_with("zoompan=z='"));
        assert!(filter.contains("s=1080x1920"));
        assert!(filter.contains("fps=30"));
        // Should contain the three phases for t=5.0
        assert!(filter.contains("between(in_time"));
        assert!(filter.contains("5.000"));
        assert!(filter.contains("1.3"));
    }

    #[test]
    fn test_generate_event_zoom_filter_multiple_events() {
        let filter = VideoProcessor::generate_event_zoom_filter(&[2.0, 8.0, 15.0], 60, 1080, 1920);
        assert!(filter.starts_with("zoompan=z='"));
        assert!(filter.contains("fps=60"));
        // All three event times should appear
        assert!(filter.contains("2.000"));
        assert!(filter.contains("8.000"));
        assert!(filter.contains("15.000"));
    }

    #[test]
    fn test_generate_event_zoom_filter_produces_valid_structure() {
        let filter = VideoProcessor::generate_event_zoom_filter(&[3.0], 30, 1080, 1920);
        // Verify the filter has balanced parentheses (basic structural validity)
        let open_count = filter.chars().filter(|&c| c == '(').count();
        let close_count = filter.chars().filter(|&c| c == ')').count();
        assert_eq!(
            open_count, close_count,
            "Unbalanced parentheses in filter: {}",
            filter
        );
    }

    // ---- compose_with_options filtergraph builder ----

    #[test]
    fn build_compose_filtergraph_single_clip_hardcut() {
        let f =
            VideoProcessor::build_compose_filtergraph(1, 1080, 1920, 60, None, &[], &[true], None);
        assert!(f.contains("[0:v]"), "{}", f);
        assert!(f.contains("crop=1080:1920"), "{}", f);
        assert!(f.contains("[outv]") && f.ends_with("[outa]"), "{}", f);
        assert!(
            !f.contains("concat="),
            "single clip should not concat: {}",
            f
        );
    }

    #[test]
    fn build_compose_filtergraph_multi_clip_concat() {
        let f = VideoProcessor::build_compose_filtergraph(
            3,
            1080,
            1920,
            60,
            None,
            &[],
            &[true, true, true],
            None,
        );
        assert!(f.contains("concat=n=3:v=1:a=1[vc][ac]"), "{}", f);
        assert!(f.contains("[v0][a0][v1][a1][v2][a2]"), "{}", f);
    }

    #[test]
    fn build_compose_filtergraph_transition_uses_xfade_offsets() {
        // Two 5s clips with a 0.5s fade: first xfade offset = 5.0 - 0.5 = 4.5.
        let f = VideoProcessor::build_compose_filtergraph(
            2,
            1080,
            1920,
            30,
            Some(("fade", 0.5)),
            &[5.0, 5.0],
            &[true, true],
            None,
        );
        assert!(
            f.contains("xfade=transition=fade:duration=0.5:offset=4.500"),
            "{}",
            f
        );
        assert!(f.contains("acrossfade=d=0.5"), "{}", f);
        assert!(!f.contains("concat="), "{}", f);
    }

    #[test]
    fn build_compose_filtergraph_slide_maps_to_slideleft() {
        let f = VideoProcessor::build_compose_filtergraph(
            2,
            1080,
            1920,
            30,
            Some(("slide", 0.5)),
            &[5.0, 5.0],
            &[true, true],
            None,
        );
        assert!(f.contains("transition=slideleft"), "{}", f);
    }

    #[test]
    fn build_compose_filtergraph_zoom_appended() {
        let f = VideoProcessor::build_compose_filtergraph(
            2,
            1080,
            1920,
            60,
            None,
            &[],
            &[true, true],
            Some(&[3.0]),
        );
        assert!(f.contains("zoompan"), "{}", f);
        assert!(f.contains("fps=60"), "{}", f);
    }

    // ---- silent-clip handling: no [i:a] reference, synthesized silence ----

    #[test]
    fn build_compose_filtergraph_all_audio_uses_input_audio() {
        // Regression: every clip has audio -> real [i:a], no synthesized silence.
        let f = VideoProcessor::build_compose_filtergraph(
            2,
            1080,
            1920,
            60,
            None,
            &[5.0, 5.0],
            &[true, true],
            None,
        );
        assert!(f.contains("[0:a]"), "audio clip keeps [0:a]: {}", f);
        assert!(f.contains("[1:a]"), "audio clip keeps [1:a]: {}", f);
        assert!(
            !f.contains("anullsrc"),
            "no silence should be synthesized: {}",
            f
        );
    }

    #[test]
    fn build_compose_filtergraph_silent_clip_synthesizes_bounded_silence() {
        // Mixed: clip 1 has no audio -> anullsrc bounded to its trimmed duration,
        // and its [1:a] input must never be referenced.
        let f = VideoProcessor::build_compose_filtergraph(
            2,
            1080,
            1920,
            60,
            None,
            &[5.0, 4.0],
            &[true, false],
            None,
        );
        assert!(f.contains("[0:a]"), "audio clip keeps [0:a]: {}", f);
        assert!(
            !f.contains("[1:a]"),
            "silent clip must not reference [1:a]: {}",
            f
        );
        assert!(
            f.contains("anullsrc=channel_layout=stereo:sample_rate=48000"),
            "silence synthesized for silent clip: {}",
            f
        );
        assert!(
            f.contains("atrim=duration=4.000"),
            "silence bounded to clip's trimmed duration: {}",
            f
        );
        assert!(
            f.contains("[a1]"),
            "synthesized silence still labeled [a1]: {}",
            f
        );
        // Concat still consumes [v_][a_] pairs for every clip.
        assert!(f.contains("[v0][a0][v1][a1]concat=n=2"), "{}", f);
    }

    #[test]
    fn build_compose_filtergraph_all_silent_synthesizes_all_silence() {
        let f = VideoProcessor::build_compose_filtergraph(
            2,
            1080,
            1920,
            60,
            None,
            &[3.0, 3.0],
            &[false, false],
            None,
        );
        assert!(
            !f.contains("[0:a]") && !f.contains("[1:a]"),
            "no input audio references when all clips are silent: {}",
            f
        );
        assert_eq!(
            f.matches("anullsrc").count(),
            2,
            "exactly one silence source per clip: {}",
            f
        );
        assert!(f.contains("[v0][a0][v1][a1]concat=n=2"), "{}", f);
    }

    #[test]
    fn build_compose_filtergraph_silent_clip_with_transition_feeds_acrossfade() {
        // Transition path: acrossfade must still find [a1] from synthesized silence.
        let f = VideoProcessor::build_compose_filtergraph(
            2,
            1080,
            1920,
            30,
            Some(("fade", 0.5)),
            &[5.0, 5.0],
            &[true, false],
            None,
        );
        assert!(
            !f.contains("[1:a]"),
            "silent clip must not reference [1:a] even with transition: {}",
            f
        );
        assert!(f.contains("anullsrc"), "silence synthesized: {}", f);
        assert!(
            f.contains("[a0][a1]acrossfade=d=0.5"),
            "acrossfade consumes the synthesized [a1]: {}",
            f
        );
    }

    // ---- Audio normalization: loudnorm parsing tests ----

    #[test]
    fn test_parse_loudnorm_output_valid() {
        let stderr = r#"
[Parsed_loudnorm_0 @ 0x12345] Summary:
{
	"input_i" : "-24.05",
	"input_tp" : "-2.14",
	"input_lra" : "10.20",
	"input_thresh" : "-34.67",
	"output_i" : "-14.02",
	"output_tp" : "-1.50",
	"output_lra" : "8.10",
	"output_thresh" : "-24.19",
	"normalization_type" : "dynamic",
	"target_offset" : "0.02"
}
"#;
        let info = VideoProcessor::parse_loudnorm_output(stderr).unwrap();
        assert_eq!(
            info,
            LoudnessInfo {
                input_i: -24.05,
                input_tp: -2.14,
                input_lra: 10.20,
                input_thresh: -34.67,
            }
        );
    }

    #[test]
    fn test_parse_loudnorm_output_with_crlf() {
        let stderr = "some ffmpeg output\r\n{\r\n\t\"input_i\" : \"-18.50\",\r\n\t\"input_tp\" : \"-0.80\",\r\n\t\"input_lra\" : \"5.30\",\r\n\t\"input_thresh\" : \"-28.90\",\r\n\t\"output_i\" : \"-14.00\",\r\n\t\"output_tp\" : \"-1.50\",\r\n\t\"output_lra\" : \"4.20\",\r\n\t\"output_thresh\" : \"-24.50\",\r\n\t\"normalization_type\" : \"dynamic\",\r\n\t\"target_offset\" : \"0.00\"\r\n}\r\n";
        let info = VideoProcessor::parse_loudnorm_output(stderr).unwrap();
        assert_eq!(info.input_i, -18.50);
        assert_eq!(info.input_tp, -0.80);
        assert_eq!(info.input_lra, 5.30);
        assert_eq!(info.input_thresh, -28.90);
    }

    #[test]
    fn test_parse_loudnorm_output_no_json() {
        let stderr = "some random ffmpeg output without json";
        let result = VideoProcessor::parse_loudnorm_output(stderr);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("loudnorm JSON"), "error: {}", err);
    }

    #[test]
    fn test_parse_loudnorm_output_missing_field() {
        let stderr = r#"
{
	"input_i" : "-24.05",
	"input_tp" : "-2.14"
}
"#;
        let result = VideoProcessor::parse_loudnorm_output(stderr);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("input_lra"), "error: {}", err);
    }

    #[test]
    fn test_parse_loudnorm_output_invalid_number() {
        let stderr = r#"
{
	"input_i" : "not_a_number",
	"input_tp" : "-2.14",
	"input_lra" : "10.20",
	"input_thresh" : "-34.67"
}
"#;
        let result = VideoProcessor::parse_loudnorm_output(stderr);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("input_i"), "error: {}", err);
    }

    #[test]
    fn test_parse_loudnorm_output_picks_last_json_block() {
        // FFmpeg may print multiple JSON-like blocks; we use rfind to pick the last one
        let stderr = r#"
some earlier { "unrelated": "data" }
more output
{
	"input_i" : "-20.00",
	"input_tp" : "-1.00",
	"input_lra" : "7.00",
	"input_thresh" : "-30.00",
	"output_i" : "-14.00",
	"output_tp" : "-1.50",
	"output_lra" : "6.00",
	"output_thresh" : "-24.00",
	"normalization_type" : "dynamic",
	"target_offset" : "0.00"
}
"#;
        let info = VideoProcessor::parse_loudnorm_output(stderr).unwrap();
        assert_eq!(info.input_i, -20.00);
    }
}
