//! Versioned delivery-contract validation for generated and platform-exported media.
//!
//! Contract v1 intentionally uses one LoLShorts product contract for YouTube
//! Shorts, TikTok and Instagram Reels: MP4/H.264/yuv420p, progressive 1080x1920
//! at CFR <= 60fps, AAC 48kHz stereo, <= 20Mbps and <= 180 seconds. The upload
//! layer must still enforce account-specific limits returned by each platform;
//! TikTok explicitly exposes `max_video_post_duration_sec` per creator.
//! Product references (reviewed 2026-08-10):
//! - https://support.google.com/youtube/answer/15424877
//! - https://developers.tiktok.com/doc/content-posting-api-media-transfer-guide
//! - https://developers.tiktok.com/doc/content-posting-api-reference-query-creator-info
//! - https://www.postman.com/meta/instagram/folder/f95kq5e/reels-publishing

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::auto_composer::PlatformPreset;

pub const OUTPUT_CONTRACT_VERSION: u32 = 1;
pub const OUTPUT_WIDTH: u32 = 1080;
pub const OUTPUT_HEIGHT: u32 = 1920;
pub const OUTPUT_MAX_FPS: f64 = 60.0;
pub const OUTPUT_MAX_DURATION_SECS: f64 = 180.0;
pub const OUTPUT_MAX_BITRATE_BPS: u64 = 20_000_000;
pub const OUTPUT_DURATION_TOLERANCE_SECS: f64 = 0.25;
const FFPROBE_TIMEOUT: Duration = Duration::from_secs(20);
const DECODE_VALIDATION_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputValidationStatus {
    Valid,
    Warning,
    Invalid,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputValidationSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputValidationIssue {
    pub code: String,
    pub severity: OutputValidationSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputValidationReport {
    pub contract_version: u32,
    pub status: OutputValidationStatus,
    pub preset: PlatformPreset,
    pub probed_at: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub is_cfr: bool,
    pub sample_aspect_ratio: String,
    pub display_aspect_ratio: String,
    pub duration: f64,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    pub pixel_format: String,
    pub sample_rate: Option<u32>,
    pub audio_channels: Option<u32>,
    pub file_size_bytes: u64,
    pub bitrate_bps: u64,
    pub decode_smoke_passed: bool,
    pub issues: Vec<OutputValidationIssue>,
}

impl OutputValidationReport {
    pub fn unknown(preset: PlatformPreset, code: &str, message: impl Into<String>) -> Self {
        Self {
            contract_version: OUTPUT_CONTRACT_VERSION,
            status: OutputValidationStatus::Unknown,
            preset,
            probed_at: Utc::now().to_rfc3339(),
            width: 0,
            height: 0,
            fps: 0.0,
            is_cfr: false,
            sample_aspect_ratio: String::new(),
            display_aspect_ratio: String::new(),
            duration: 0.0,
            video_codec: String::new(),
            audio_codec: None,
            pixel_format: String::new(),
            sample_rate: None,
            audio_channels: None,
            file_size_bytes: 0,
            bitrate_bps: 0,
            decode_smoke_passed: false,
            issues: vec![OutputValidationIssue {
                code: code.to_string(),
                severity: OutputValidationSeverity::Warning,
                message: message.into(),
            }],
        }
    }

    pub fn is_delivery_ready(&self) -> bool {
        matches!(
            self.status,
            OutputValidationStatus::Valid | OutputValidationStatus::Warning
        )
    }
}

#[derive(Debug, Deserialize)]
struct ProbeRoot {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    #[serde(default)]
    format: ProbeFormat,
}

#[derive(Debug, Default, Deserialize)]
struct ProbeFormat {
    format_name: Option<String>,
    duration: Option<String>,
    size: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    field_order: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    sample_aspect_ratio: Option<String>,
    display_aspect_ratio: Option<String>,
    duration: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
}

pub struct OutputValidator;

impl OutputValidator {
    pub async fn validate(
        path: &Path,
        preset: PlatformPreset,
        planned_duration: Option<f64>,
    ) -> OutputValidationReport {
        if !path.is_file() {
            let mut report = OutputValidationReport::unknown(
                preset,
                "file_missing",
                "The output file does not exist.",
            );
            report.status = OutputValidationStatus::Invalid;
            report.issues[0].severity = OutputValidationSeverity::Error;
            return report;
        }

        let ffprobe = match crate::utils::ffmpeg::get_ffprobe_path() {
            Ok(path) => path,
            Err(error) => {
                return OutputValidationReport::unknown(
                    preset,
                    "probe_unavailable",
                    format!("FFprobe is unavailable: {error}"),
                )
            }
        };
        let probe = Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output();
        let output = match tokio::time::timeout(FFPROBE_TIMEOUT, probe).await {
            Ok(Ok(output)) if output.status.success() => output,
            Ok(Ok(output)) => {
                let mut report = OutputValidationReport::unknown(
                    preset,
                    "probe_failed",
                    String::from_utf8_lossy(&output.stderr).trim().to_string(),
                );
                report.status = OutputValidationStatus::Invalid;
                report.issues[0].severity = OutputValidationSeverity::Error;
                return report;
            }
            Ok(Err(error)) => {
                return OutputValidationReport::unknown(
                    preset,
                    "probe_failed",
                    format!("Could not start FFprobe: {error}"),
                )
            }
            Err(_) => {
                let mut report = OutputValidationReport::unknown(
                    preset,
                    "probe_timeout",
                    format!(
                        "FFprobe did not finish within {} seconds.",
                        FFPROBE_TIMEOUT.as_secs()
                    ),
                );
                report.status = OutputValidationStatus::Invalid;
                report.issues[0].severity = OutputValidationSeverity::Error;
                return report;
            }
        };

        let mut report = match Self::from_probe_json(&output.stdout, path, preset, planned_duration)
        {
            Ok(report) => report,
            Err(message) => {
                let mut report =
                    OutputValidationReport::unknown(preset, "probe_json_invalid", message);
                report.status = OutputValidationStatus::Invalid;
                report.issues[0].severity = OutputValidationSeverity::Error;
                return report;
            }
        };

        report.decode_smoke_passed = Self::decode_smoke(path).await;
        if !report.decode_smoke_passed {
            crate::utils::telemetry::capture_operational_error("media", "clip_decode_failed");
            push_error(
                &mut report.issues,
                "decode_failed",
                "FFmpeg could not decode the complete video and audio streams.",
            );
        }
        report.status = status_from_issues(&report.issues);
        report
    }

    fn from_probe_json(
        json: &[u8],
        path: &Path,
        preset: PlatformPreset,
        planned_duration: Option<f64>,
    ) -> Result<OutputValidationReport, String> {
        let root: ProbeRoot = serde_json::from_slice(json)
            .map_err(|error| format!("Invalid FFprobe JSON: {error}"))?;
        let video = root
            .streams
            .iter()
            .find(|stream| stream.codec_type.as_deref() == Some("video"));
        let audio = root
            .streams
            .iter()
            .find(|stream| stream.codec_type.as_deref() == Some("audio"));
        let duration = parse_number(root.format.duration.as_deref())
            .or_else(|| video.and_then(|stream| parse_number(stream.duration.as_deref())))
            .unwrap_or(0.0);
        let fps = video
            .and_then(|stream| {
                parse_ratio(stream.avg_frame_rate.as_deref())
                    .or_else(|| parse_ratio(stream.r_frame_rate.as_deref()))
            })
            .unwrap_or(0.0);
        let average_fps = video.and_then(|stream| parse_ratio(stream.avg_frame_rate.as_deref()));
        let real_fps = video.and_then(|stream| parse_ratio(stream.r_frame_rate.as_deref()));
        let is_cfr = matches!((average_fps, real_fps), (Some(average), Some(real)) if (average - real).abs() <= 0.01);
        let file_size_bytes = std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or_else(|_| parse_integer(root.format.size.as_deref()).unwrap_or(0));
        let bitrate_bps = parse_integer(root.format.bit_rate.as_deref()).unwrap_or_else(|| {
            if duration > 0.0 {
                ((file_size_bytes as f64 * 8.0) / duration).round() as u64
            } else {
                0
            }
        });
        let mut issues = Vec::new();

        let format_name = root.format.format_name.unwrap_or_default();
        if !format_name
            .split(',')
            .any(|name| name == "mov" || name == "mp4")
        {
            push_error(
                &mut issues,
                "container_not_mp4",
                "The container is not MP4.",
            );
        }
        let (width, height, video_codec, pixel_format, sample_aspect_ratio, display_aspect_ratio) =
            match video {
                Some(video) => {
                    let width = video.width.unwrap_or(0);
                    let height = video.height.unwrap_or(0);
                    let codec = video.codec_name.clone().unwrap_or_default();
                    let pixel_format = video.pix_fmt.clone().unwrap_or_default();
                    if codec != "h264" {
                        push_error(&mut issues, "video_codec_not_h264", "Video must use H.264.");
                    }
                    if pixel_format != "yuv420p" {
                        push_error(
                            &mut issues,
                            "pixel_format_not_yuv420p",
                            "Video must use yuv420p.",
                        );
                    }
                    if width != OUTPUT_WIDTH || height != OUTPUT_HEIGHT {
                        push_error(
                            &mut issues,
                            "dimensions_not_1080x1920",
                            "Video must be 1080x1920.",
                        );
                    }
                    let sample_aspect_ratio = video.sample_aspect_ratio.clone().unwrap_or_default();
                    let display_aspect_ratio =
                        video.display_aspect_ratio.clone().unwrap_or_default();
                    if !ratio_matches(&sample_aspect_ratio, 1.0) {
                        push_error(
                            &mut issues,
                            "sample_aspect_ratio_not_square",
                            "Video must use square pixels (SAR 1:1).",
                        );
                    }
                    if !ratio_matches(&display_aspect_ratio, 9.0 / 16.0) {
                        push_error(
                            &mut issues,
                            "display_aspect_ratio_not_9_16",
                            "Video display aspect ratio must be 9:16.",
                        );
                    }
                    if !matches!(
                        video.field_order.as_deref(),
                        None | Some("progressive") | Some("unknown")
                    ) {
                        push_error(
                            &mut issues,
                            "video_not_progressive",
                            "Interlaced video is not supported.",
                        );
                    }
                    (
                        width,
                        height,
                        codec,
                        pixel_format,
                        sample_aspect_ratio,
                        display_aspect_ratio,
                    )
                }
                None => {
                    push_error(
                        &mut issues,
                        "video_stream_missing",
                        "A non-empty video stream is required.",
                    );
                    (
                        0,
                        0,
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                    )
                }
            };
        if fps <= 0.0 || !fps.is_finite() {
            push_error(
                &mut issues,
                "fps_invalid",
                "Frame rate could not be measured.",
            );
        } else if fps > OUTPUT_MAX_FPS + 0.01 {
            push_error(
                &mut issues,
                "fps_over_60",
                "Frame rate must not exceed 60fps.",
            );
        }
        if !is_cfr {
            push_error(
                &mut issues,
                "frame_rate_not_cfr",
                "Average and real frame rates must match (CFR).",
            );
        }
        if duration <= 0.0 || !duration.is_finite() {
            push_error(
                &mut issues,
                "duration_invalid",
                "Duration must be positive.",
            );
        } else {
            if duration > OUTPUT_MAX_DURATION_SECS + OUTPUT_DURATION_TOLERANCE_SECS {
                push_error(
                    &mut issues,
                    "duration_over_180",
                    "Output duration exceeds 180 seconds.",
                );
            }
            if let Some(planned) = planned_duration.filter(|value| value.is_finite()) {
                if (duration - planned).abs() > OUTPUT_DURATION_TOLERANCE_SECS {
                    push_error(
                        &mut issues,
                        "duration_mismatch",
                        "Measured duration differs from the planned duration by more than 0.25 seconds.",
                    );
                }
            }
        }
        if bitrate_bps > OUTPUT_MAX_BITRATE_BPS {
            push_error(
                &mut issues,
                "bitrate_over_20mbps",
                "Average bitrate exceeds 20Mbps.",
            );
        }

        let (audio_codec, sample_rate, audio_channels) = match audio {
            Some(audio) => {
                let codec = audio.codec_name.clone().unwrap_or_default();
                let sample_rate = audio
                    .sample_rate
                    .as_deref()
                    .and_then(|value| value.parse::<u32>().ok());
                let channels = audio.channels;
                if codec != "aac" {
                    push_error(&mut issues, "audio_codec_not_aac", "Audio must use AAC.");
                }
                if sample_rate != Some(48_000) {
                    push_error(
                        &mut issues,
                        "audio_sample_rate_not_48000",
                        "Audio must use a 48kHz sample rate.",
                    );
                }
                if channels != Some(2) {
                    push_error(
                        &mut issues,
                        "audio_not_stereo",
                        "Audio must contain two channels.",
                    );
                }
                (Some(codec), sample_rate, channels)
            }
            None => {
                push_error(
                    &mut issues,
                    "audio_stream_missing",
                    "A bounded AAC audio stream is required.",
                );
                (None, None, None)
            }
        };

        Ok(OutputValidationReport {
            contract_version: OUTPUT_CONTRACT_VERSION,
            status: status_from_issues(&issues),
            preset,
            probed_at: Utc::now().to_rfc3339(),
            width,
            height,
            fps,
            is_cfr,
            sample_aspect_ratio,
            display_aspect_ratio,
            duration,
            video_codec,
            audio_codec,
            pixel_format,
            sample_rate,
            audio_channels,
            file_size_bytes,
            bitrate_bps,
            decode_smoke_passed: false,
            issues,
        })
    }

    async fn decode_smoke(path: &Path) -> bool {
        let ffmpeg = match crate::utils::ffmpeg::get_ffmpeg_path() {
            Ok(path) => path,
            Err(_) => return false,
        };
        let decode = Command::new(ffmpeg)
            .args(["-v", "error", "-xerror", "-i"])
            .arg(path)
            .args(["-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status();
        match tokio::time::timeout(DECODE_VALIDATION_TIMEOUT, decode).await {
            Ok(Ok(status)) => status.success(),
            Ok(Err(error)) => {
                tracing::warn!(%error, "Decode validation process failed");
                false
            }
            Err(_) => {
                tracing::warn!(
                    timeout_secs = DECODE_VALIDATION_TIMEOUT.as_secs(),
                    "Decode validation timed out"
                );
                false
            }
        }
    }

    pub async fn transcode_to_contract(source: &Path, partial: &Path) -> Result<(), String> {
        let parent = partial
            .parent()
            .ok_or_else(|| "Platform export has no parent directory".to_string())?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
        let has_audio = Self::has_audio_stream(source).await;
        let ffmpeg = crate::utils::ffmpeg::get_ffmpeg_path().map_err(|error| error.to_string())?;

        let run = |encoder: &'static str| {
            let ffmpeg = ffmpeg.clone();
            let source = source.to_path_buf();
            let partial = partial.to_path_buf();
            async move {
                let mut command = Command::new(ffmpeg);
                command.args(["-y", "-v", "error", "-i"]).arg(&source);
                if !has_audio {
                    command.args(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo"]);
                }
                command.args([
                    "-map",
                    "0:v:0",
                    "-vf",
                    "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:black,fps=60,format=yuv420p",
                    "-c:v",
                    encoder,
                    "-b:v",
                    "12M",
                    "-maxrate",
                    "20M",
                    "-bufsize",
                    "40M",
                ]);
                if has_audio {
                    command.args(["-map", "0:a:0"]);
                } else {
                    command.args(["-map", "1:a:0"]);
                }
                command.args([
                    "-c:a",
                    "aac",
                    "-ar",
                    "48000",
                    "-ac",
                    "2",
                    "-t",
                    "180",
                    "-shortest",
                    "-movflags",
                    "+faststart",
                    "-f",
                    "mp4",
                ]);
                command.arg(&partial);
                crate::video::execute_ffmpeg_command(&mut command)
                    .await
                    .map_err(|error| error.to_string())
            }
        };

        if let Err(hardware_error) = run("h264_nvenc").await {
            tracing::warn!(
                "Hardware platform export failed; using libx264: {}",
                hardware_error
            );
            let _ = tokio::fs::remove_file(partial).await;
            run("libx264").await?;
        }
        Ok(())
    }

    async fn has_audio_stream(source: &Path) -> bool {
        let Ok(ffprobe) = crate::utils::ffmpeg::get_ffprobe_path() else {
            return false;
        };
        let probe = Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
            ])
            .arg(source)
            .kill_on_drop(true)
            .output();
        match tokio::time::timeout(FFPROBE_TIMEOUT, probe).await {
            Ok(Ok(output)) => output.status.success() && !output.stdout.is_empty(),
            Ok(Err(error)) => {
                tracing::warn!(%error, "Audio stream probe failed");
                false
            }
            Err(_) => {
                tracing::warn!(
                    timeout_secs = FFPROBE_TIMEOUT.as_secs(),
                    "Audio stream probe timed out"
                );
                false
            }
        }
    }
}

pub fn file_fingerprint(path: &Path) -> Result<String, std::io::Error> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hash = 0xcbf29ce484222325u64;
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        for byte in &buffer[..count] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Ok(format!("fnv1a64:{hash:016x}:{size}"))
}

pub async fn file_fingerprint_async(path: PathBuf) -> Result<String, std::io::Error> {
    tokio::task::spawn_blocking(move || file_fingerprint(&path))
        .await
        .map_err(|error| std::io::Error::other(format!("fingerprint task failed: {error}")))?
}

pub fn validated_path_for(partial: &Path) -> PathBuf {
    let name = partial
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("output.partial.mp4")
        .replace(".partial.mp4", ".validated.mp4");
    partial.with_file_name(name)
}

fn push_error(issues: &mut Vec<OutputValidationIssue>, code: &str, message: &str) {
    issues.push(OutputValidationIssue {
        code: code.to_string(),
        severity: OutputValidationSeverity::Error,
        message: message.to_string(),
    });
}

fn status_from_issues(issues: &[OutputValidationIssue]) -> OutputValidationStatus {
    if issues
        .iter()
        .any(|issue| issue.severity == OutputValidationSeverity::Error)
    {
        OutputValidationStatus::Invalid
    } else if issues.is_empty() {
        OutputValidationStatus::Valid
    } else {
        OutputValidationStatus::Warning
    }
}

fn parse_number(value: Option<&str>) -> Option<f64> {
    value?.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn parse_integer(value: Option<&str>) -> Option<u64> {
    value?.parse::<u64>().ok()
}

fn parse_ratio(value: Option<&str>) -> Option<f64> {
    let value = value?;
    let (numerator, denominator) = value.split_once('/')?;
    let numerator = numerator.parse::<f64>().ok()?;
    let denominator = denominator.parse::<f64>().ok()?;
    (denominator != 0.0).then_some(numerator / denominator)
}

fn ratio_matches(value: &str, expected: f64) -> bool {
    let parsed = value.split_once(':').and_then(|(numerator, denominator)| {
        let numerator = numerator.parse::<f64>().ok()?;
        let denominator = denominator.parse::<f64>().ok()?;
        (denominator != 0.0).then_some(numerator / denominator)
    });
    matches!(parsed, Some(actual) if (actual - expected).abs() <= 0.001)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_probe_json() -> Vec<u8> {
        br#"{
          "streams": [
            {"codec_type":"video","codec_name":"h264","width":1080,"height":1920,"pix_fmt":"yuv420p","field_order":"progressive","avg_frame_rate":"60/1","r_frame_rate":"60/1","sample_aspect_ratio":"1:1","display_aspect_ratio":"9:16","duration":"10.000"},
            {"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":2,"duration":"10.000"}
          ],
          "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"10.000","size":"1000000","bit_rate":"800000"}
        }"#.to_vec()
    }

    #[test]
    fn accepts_common_delivery_contract() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let mut report = OutputValidator::from_probe_json(
            &valid_probe_json(),
            temp.path(),
            PlatformPreset::YoutubeShorts,
            Some(10.0),
        )
        .unwrap();
        report.decode_smoke_passed = true;
        report.status = status_from_issues(&report.issues);
        assert_eq!(report.status, OutputValidationStatus::Valid);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn reports_stable_codes_for_bad_video_and_missing_audio() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let json = br#"{"streams":[{"codec_type":"video","codec_name":"hevc","width":1920,"height":1080,"pix_fmt":"yuv444p","field_order":"tt","avg_frame_rate":"120/1","r_frame_rate":"60/1","sample_aspect_ratio":"4:3","display_aspect_ratio":"16:9"}],"format":{"format_name":"matroska","duration":"181","bit_rate":"21000000"}}"#;
        let report =
            OutputValidator::from_probe_json(json, temp.path(), PlatformPreset::Tiktok, None)
                .unwrap();
        let codes = report
            .issues
            .iter()
            .map(|issue| issue.code.as_str())
            .collect::<Vec<_>>();
        assert!(codes.contains(&"container_not_mp4"));
        assert!(codes.contains(&"video_codec_not_h264"));
        assert!(codes.contains(&"dimensions_not_1080x1920"));
        assert!(codes.contains(&"frame_rate_not_cfr"));
        assert!(codes.contains(&"sample_aspect_ratio_not_square"));
        assert!(codes.contains(&"audio_stream_missing"));
        assert!(codes.contains(&"duration_over_180"));
    }

    #[test]
    fn fingerprint_changes_with_content() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"one").unwrap();
        let first = file_fingerprint(temp.path()).unwrap();
        std::fs::write(temp.path(), b"two").unwrap();
        let second = file_fingerprint(temp.path()).unwrap();
        assert_ne!(first, second);
    }
}
