//! macOS-optimized FFmpeg integration
//!
//! Provides macOS-specific FFmpeg optimizations and hardware acceleration
//! Supports VideoToolbox, AudioToolbox, and other macOS frameworks

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tokio::process::Command as TokioCommand;

use crate::recording::audio::AudioConfig;

/// macOS FFmpeg encoder options
#[derive(Debug, Clone)]
pub struct MacFFmpegEncoder {
    pub name: String,
    pub encoder: String,
    pub pixel_format: String,
    pub preset: Option<String>,
    pub tune: Option<String>,
    pub profile: Option<String>,
    pub level: Option<String>,
    pub is_hardware: bool,
    pub supports_b_frames: bool,
}

/// macOS FFmpeg configuration
#[derive(Debug, Clone)]
pub struct MacFFmpegConfig {
    pub input_format: String,
    pub video_encoder: MacFFmpegEncoder,
    pub audio_encoder: String,
    pub container_format: String,
    pub mov_flags: Vec<String>,
    pub additional_args: Vec<String>,
}

impl Default for MacFFmpegConfig {
    fn default() -> Self {
        Self {
            input_format: "avfoundation".to_string(),
            video_encoder: MacFFmpegEncoder {
                name: "VideoToolbox".to_string(),
                encoder: "h264_videotoolbox".to_string(),
                pixel_format: "nv12".to_string(),
                preset: Some("medium".to_string()),
                tune: None,
                profile: Some("high".to_string()),
                level: Some("4.0".to_string()),
                is_hardware: true,
                supports_b_frames: true,
            },
            audio_encoder: "aac".to_string(),
            container_format: "mp4".to_string(),
            mov_flags: vec!["+faststart".to_string(), "use_metadata_tags".to_string()],
            additional_args: vec![
                "-threads".to_string(),
                "0".to_string(),
                "-async".to_string(),
                "1".to_string(),
            ],
        }
    }
}

/// macOS FFmpeg command builder
pub struct MacFFmpegCommandBuilder {
    config: MacFFmpegConfig,
    input_sources: Vec<String>,
    output_path: PathBuf,
    video_params: VideoParameters,
    audio_params: AudioParameters,
}

#[derive(Debug, Clone)]
pub struct VideoParameters {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub bitrate: u32,
    pub gop_size: u32,
    pub max_b_frames: u32,
}

#[derive(Debug, Clone)]
pub struct AudioParameters {
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate: u32,
    pub enabled: bool,
}

impl MacFFmpegCommandBuilder {
    pub fn new(config: MacFFmpegConfig, output_path: PathBuf) -> Self {
        Self {
            config,
            input_sources: Vec::new(),
            output_path,
            video_params: VideoParameters {
                width: 1920,
                height: 1080,
                fps: 60.0,
                bitrate: 15_000_000,
                gop_size: 60,
                max_b_frames: 3,
            },
            audio_params: AudioParameters {
                sample_rate: 44100,
                channels: 2,
                bitrate: 128_000,
                enabled: false,
            },
        }
    }

    /// Add video input source (display capture)
    pub fn add_video_input(mut self, display_index: i32) -> Self {
        let input = format!("{}:0", display_index);
        self.input_sources.push(input);
        self
    }

    /// Add audio input source
    pub fn add_audio_input(mut self, device_index: i32) -> Self {
        let input = format!("{}:{}", self.input_sources[0], device_index);
        if self.input_sources.len() > 1 {
            self.input_sources[1] = input;
        } else {
            self.input_sources.push(input);
        }
        self.audio_params.enabled = true;
        self
    }

    /// Set video parameters
    pub fn with_video_params(mut self, params: VideoParameters) -> Self {
        self.video_params = params;
        self
    }

    /// Set audio parameters
    pub fn with_audio_params(mut self, params: AudioParameters) -> Self {
        self.audio_params = params;
        self
    }

    /// Build FFmpeg command
    pub fn build(self) -> Result<TokioCommand> {
        let ffmpeg_path = crate::utils::ffmpeg::get_ffmpeg_path()?;
        let mut cmd = TokioCommand::new(ffmpeg_path);

        // Global options
        cmd.args(["-y"]); // Overwrite output

        // Input configuration
        cmd.args(["-f", &self.config.input_format]);
        cmd.args(["-framerate", &self.video_params.fps.to_string()]);
        cmd.args([
            "-video_size",
            &format!("{}x{}", self.video_params.width, self.video_params.height),
        ]);
        cmd.args(["-pixel_format", &self.config.video_encoder.pixel_format]);

        // Input sources
        for input in &self.input_sources {
            cmd.args(["-i", input]);
        }

        // Map streams
        if self.input_sources.len() > 1 && self.audio_params.enabled {
            cmd.args(["-map", "0:v:0", "-map", "1:a?"]);
        } else {
            cmd.args(["-map", "0:v:0"]);
        }

        // Video encoding options
        cmd.args(["-c:v", &self.config.video_encoder.encoder]);

        if let Some(ref preset) = self.config.video_encoder.preset {
            cmd.args(["-preset", preset]);
        }

        if let Some(ref tune) = self.config.video_encoder.tune {
            cmd.args(["-tune", tune]);
        }

        if let Some(ref profile) = self.config.video_encoder.profile {
            cmd.args(["-profile:v", profile]);
        }

        if let Some(ref level) = self.config.video_encoder.level {
            cmd.args(["-level:v", level]);
        }

        cmd.args([
            "-b:v",
            &format!("{}M", self.video_params.bitrate / 1_000_000),
        ]);
        cmd.args([
            "-maxrate",
            &format!(
                "{}M",
                (self.video_params.bitrate as f64 * 1.5) / 1_000_000.0
            ),
        ]);
        cmd.args([
            "-bufsize",
            &format!(
                "{}M",
                (self.video_params.bitrate as f64 * 2.0) / 1_000_000.0
            ),
        ]);

        // GOP and B-frames
        cmd.args(["-g", &self.video_params.gop_size.to_string()]);
        if self.config.video_encoder.supports_b_frames {
            cmd.args(["-bf", &self.video_params.max_b_frames.to_string()]);
        }

        // Audio encoding options (if enabled)
        if self.audio_params.enabled {
            cmd.args(["-c:a", &self.config.audio_encoder]);
            cmd.args(["-b:a", &format!("{}k", self.audio_params.bitrate / 1_000)]);
            cmd.args(["-ar", &self.audio_params.sample_rate.to_string()]);
            cmd.args(["-ac", &self.audio_params.channels.to_string()]);
        }

        // Container format options
        cmd.args(["-f", &self.config.container_format]);
        for flag in &self.config.mov_flags {
            cmd.args(["-movflags", flag]);
        }

        // Additional arguments
        cmd.args(&self.config.additional_args);

        // Output
        cmd.arg(&self.output_path);

        // Process configuration
        cmd.stdin(Stdio::null());
        cmd.stderr(Stdio::piped());
        cmd.stdout(Stdio::null());

        Ok(cmd)
    }
}

/// Detect available macOS encoders
pub fn detect_available_macos_encoders() -> Result<Vec<MacFFmpegEncoder>> {
    let ffmpeg_path = crate::utils::ffmpeg::get_ffmpeg_path()?;
    let output = Command::new(ffmpeg_path)
        .args(["-encoders"])
        .output()
        .context("Failed to get FFmpeg encoders list")?;

    let output_str = String::from_utf8(output.stdout)?;
    parse_macos_encoders(&output_str)
}

/// Parse FFmpeg encoder list for macOS encoders
fn parse_macos_encoders(output: &str) -> Result<Vec<MacFFmpegEncoder>> {
    let mut encoders = Vec::new();

    for line in output.lines() {
        if line.trim().is_empty() || line.starts_with("----") {
            continue;
        }

        // Parse encoder line format: A..... h264_videotoolbox H.264 VideoToolbox encoder
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let encoder_name = parts[1];
            let description = parts[2..].join(" ");

            if is_macos_encoder(encoder_name, &description) {
                encoders.push(create_encoder_info(encoder_name, &description));
            }
        }
    }

    // Add software encoders as fallback
    encoders.push(MacFFmpegEncoder {
        name: "libx264".to_string(),
        encoder: "libx264".to_string(),
        pixel_format: "yuv420p".to_string(),
        preset: Some("medium".to_string()),
        tune: None,
        profile: Some("high".to_string()),
        level: Some("4.0".to_string()),
        is_hardware: false,
        supports_b_frames: true,
    });

    encoders.push(MacFFmpegEncoder {
        name: "libx265".to_string(),
        encoder: "libx265".to_string(),
        pixel_format: "yuv420p".to_string(),
        preset: Some("medium".to_string()),
        tune: None,
        profile: Some("main".to_string()),
        level: Some("5.0".to_string()),
        is_hardware: false,
        supports_b_frames: true,
    });

    Ok(encoders)
}

/// Check if encoder is macOS-specific
fn is_macos_encoder(encoder_name: &str, description: &str) -> bool {
    encoder_name.contains("videotoolbox")
        || encoder_name.contains("audiotoolbox")
        || description.to_lowercase().contains("videotoolbox")
        || description.to_lowercase().contains("apple")
}

/// Create encoder info from name and description
fn create_encoder_info(encoder_name: &str, description: &str) -> MacFFmpegEncoder {
    let is_h264 = encoder_name.contains("h264") || description.contains("H.264");
    let is_hevc = encoder_name.contains("hevc")
        || description.contains("H.265")
        || description.contains("HEVC");
    let is_hardware = encoder_name.contains("videotoolbox");

    MacFFmpegEncoder {
        name: description.to_string(),
        encoder: encoder_name.to_string(),
        pixel_format: if is_hardware {
            "nv12".to_string()
        } else {
            "yuv420p".to_string()
        },
        preset: Some("medium".to_string()),
        tune: if is_h264 {
            Some("zerolatency".to_string())
        } else {
            None
        },
        profile: Some(if is_h264 {
            "high".to_string()
        } else if is_hevc {
            "main".to_string()
        } else {
            "main".to_string()
        }),
        level: Some("4.0".to_string()),
        is_hardware,
        supports_b_frames: !encoder_name.contains("baseline"),
    }
}

/// Optimize FFmpeg parameters for specific encoder and use case
pub fn optimize_ffmpeg_params(
    encoder: &MacFFmpegEncoder,
    use_case: FFmpegUseCase,
) -> MacFFmpegConfig {
    let mut config = MacFFmpegConfig::default();
    config.video_encoder = encoder.clone();

    match use_case {
        FFmpegUseCase::RealTimeRecording => {
            config.video_encoder.preset = Some("veryfast".to_string());
            if encoder.is_hardware {
                config.video_encoder.tune = Some("zerolatency".to_string());
            }
            config.video_encoder.max_b_frames = if encoder.is_hardware { 0 } else { 1 };
            config.additional_args.push("-async".to_string());
            config.additional_args.push("1".to_string());
        }
        FFmpegUseCase::HighQualityEncoding => {
            config.video_encoder.preset = Some("slow".to_string());
            config.video_encoder.max_b_frames = 3;
            config.additional_args.push("-crf".to_string());
            config.additional_args.push("18".to_string());
        }
        FFmpegUseCase::LowLatencyStreaming => {
            config.video_encoder.preset = Some("ultrafast".to_string());
            config.video_encoder.tune = Some("zerolatency".to_string());
            config.video_encoder.max_b_frames = 0;
            config.additional_args.extend_from_slice(&[
                "-x264opts".to_string(),
                "no-scenecut".to_string(),
                "-tune".to_string(),
                "zerolatency".to_string(),
            ]);
        }
    }

    config
}

#[derive(Debug, Clone)]
pub enum FFmpegUseCase {
    RealTimeRecording,
    HighQualityEncoding,
    LowLatencyStreaming,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoder_creation() {
        let encoder = create_encoder_info("h264_videotoolbox", "H.264 VideoToolbox encoder");
        assert_eq!(encoder.encoder, "h264_videotoolbox");
        assert!(encoder.is_hardware);
        assert_eq!(encoder.pixel_format, "nv12");
    }

    #[test]
    fn test_ffmpeg_command_builder() {
        let config = MacFFmpegConfig::default();
        let builder = MacFFmpegCommandBuilder::new(config, PathBuf::from("/tmp/output.mp4"))
            .add_video_input(1)
            .with_video_params(VideoParameters {
                width: 1920,
                height: 1080,
                fps: 60.0,
                bitrate: 10_000_000,
                gop_size: 30,
                max_b_frames: 2,
            });

        let cmd = builder.build();
        assert!(cmd.is_ok());
    }

    #[test]
    fn test_optimization_for_use_cases() {
        let encoder = MacFFmpegEncoder {
            name: "VideoToolbox".to_string(),
            encoder: "h264_videotoolbox".to_string(),
            pixel_format: "nv12".to_string(),
            preset: Some("medium".to_string()),
            tune: None,
            profile: Some("high".to_string()),
            level: Some("4.0".to_string()),
            is_hardware: true,
            supports_b_frames: true,
        };

        let realtime_config = optimize_ffmpeg_params(&encoder, FFmpegUseCase::RealTimeRecording);
        assert_eq!(
            realtime_config.video_encoder.preset,
            Some("veryfast".to_string())
        );

        let high_quality_config =
            optimize_ffmpeg_params(&encoder, FFmpegUseCase::HighQualityEncoding);
        assert_eq!(
            high_quality_config.video_encoder.preset,
            Some("slow".to_string())
        );
    }
}
