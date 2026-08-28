#![allow(dead_code)]
use std::path::{Path, PathBuf};
use tokio::process::Command as TokioCommand;
use tracing::info;

use super::super::{execute_ffmpeg_command, Result, VideoError};
use super::pipeline::VideoProcessor;
use super::types::{ChainedEffects, ColorGrading, TextPosition, TextStyle, TransitionType};

/// 폰트 파일 경로를 drawtext `fontfile=` 절로 변환한다:
/// 역슬래시 → 슬래시 정규화 후 콜론을 이스케이프하고 작은따옴표로 감싼다.
///
/// (픽셀로 검증된 정상 동작: `fontfile='C\:/Windows/Fonts/malgun.ttf'`. 이름만
/// 넘기면 fontconfig 폴백으로 조용히 성공(exit 0)하지만 한글 글리프가 없는 폰트로
/// 떨어져 텍스트가 빈 네모(tofu)로 렌더링된다.)
pub(crate) fn fontfile_clause(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let escaped = normalized.replace(':', "\\:");
    format!("fontfile='{}'", escaped)
}

/// 훅 자막처럼 **앱이 스스로 그리는** 글자에 쓸 폰트 절.
///
/// 캔버스 텍스트(`resolve_font_clause`)와 달리 사용자가 고르는 축이 아니다 —
/// 필요한 성질이 하나뿐이기 때문이다: 한글 글리프가 있을 것. 맑은 고딕이 없는
/// 환경(비 Windows)에서는 빈 문자열을 돌려주어 drawtext 가 fontconfig 기본 폰트를
/// 쓰게 한다(글자가 안 나오느니 다른 폰트로라도 나오는 편이 낫다).
pub(crate) fn caption_font_clause() -> String {
    for candidate in [
        r"C:\Windows\Fonts\malgun.ttf",
        r"C:\Windows\Fonts\NanumGothic.ttf",
        r"C:\Windows\Fonts\arial.ttf",
    ] {
        if Path::new(candidate).exists() {
            return fontfile_clause(candidate);
        }
    }
    String::new()
}

/// Escape text for FFmpeg drawtext filter to prevent filter injection.
/// Escapes characters that have special meaning in FFmpeg filter expressions.
pub fn escape_ffmpeg_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\'', "'\\\\\\''")
        .replace(':', "\\:")
        .replace(';', "\\;")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

/// Validate a color string for FFmpeg drawtext fontcolor parameter.
/// Accepts named colors from an allowlist and hex colors in 0x format.
pub fn validate_ffmpeg_color(color: &str) -> std::result::Result<String, VideoError> {
    let lower = color.to_lowercase();
    let allowed_names = [
        "white", "yellow", "red", "green", "blue", "black", "cyan", "magenta", "orange",
    ];
    if allowed_names.contains(&lower.as_str()) {
        return Ok(lower);
    }
    // Accept hex colors: 0xRRGGBB or 0xRRGGBBAA
    if lower.starts_with("0x")
        && (lower.len() == 8 || lower.len() == 10)
        && lower[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Ok(lower);
    }
    Err(VideoError::ProcessingError {
        message: format!(
            "Invalid color '{}'. Use a named color (white, yellow, red, green, blue, black, cyan, magenta, orange) or hex (0xRRGGBB).",
            color
        ),
    })
}

/// Build an atempo filter chain for the given speed factor.
/// FFmpeg atempo only accepts values in [0.5, 100.0], so for speeds below 0.5
/// we chain multiple atempo filters (e.g., speed=0.25 becomes atempo=0.5,atempo=0.5).
fn build_atempo_chain(speed: f64) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut remaining = speed;
    while remaining < 0.5 {
        parts.push("atempo=0.5".to_string());
        remaining /= 0.5;
    }
    parts.push(format!("atempo={}", remaining));
    parts.join(",")
}

/// Shared input/output pre-validation for single-effect functions so callers
/// get a friendly `VideoError` instead of a raw FFmpeg stderr dump.
fn validate_effect_io(inputs: &[&Path], output: &Path) -> Result<()> {
    for input in inputs {
        if !input.exists() {
            return Err(VideoError::FileNotFound {
                path: input.display().to_string(),
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
    Ok(())
}

impl VideoProcessor {
    /// Generate a thumbnail from a video file
    pub async fn generate_thumbnail(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        time_offset: f64,
    ) -> Result<PathBuf> {
        let input = input_path.as_ref();
        let output = output_path.as_ref();

        info!(
            "Generating thumbnail: {:?} -> {:?} (offset: {}s)",
            input, output, time_offset
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

        let mut command = TokioCommand::new(&self.ffmpeg_path);
        command.args([
            "-ss",
            &time_offset.to_string(),
            "-i",
            input.to_str().ok_or_else(|| VideoError::FileAccessError {
                path: input.display().to_string(),
            })?,
            "-vframes",
            "1",
            "-q:v",
            "2",
            "-y",
            output.to_str().ok_or_else(|| VideoError::FileAccessError {
                path: output.display().to_string(),
            })?,
        ]);

        #[cfg(target_os = "windows")]
        command.creation_flags(0x08000000 | 0x00004000); // no window + below-normal priority

        execute_ffmpeg_command(&mut command).await?;

        if !output.exists() {
            return Err(VideoError::ProcessingError {
                message: format!("Output file was not created: {:?}", output),
            });
        }

        info!("Thumbnail generated successfully: {:?}", output);
        Ok(output.to_path_buf())
    }

    /// Apply advanced video transitions between clips
    pub async fn apply_transition(
        &self,
        input1: impl AsRef<Path>,
        input2: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        transition_type: TransitionType,
        duration: f64,
    ) -> Result<PathBuf> {
        let output = output_path.as_ref();

        info!(
            "Applying {:?} transition: {:?} + {:?} -> {:?} ({}s)",
            transition_type,
            input1.as_ref(),
            input2.as_ref(),
            output,
            duration
        );

        validate_effect_io(&[input1.as_ref(), input2.as_ref()], output)?;

        let filter_complex = match transition_type {
            TransitionType::Fade => format!(
                "[0:v][1:v]xfade=transition=fade:duration={}:offset=0[outv]",
                duration
            ),
            TransitionType::Slide => format!(
                "[0:v][1:v]xfade=transition=slideleft:duration={}:offset=0[outv]",
                duration
            ),
            TransitionType::Dissolve => format!(
                "[0:v][1:v]xfade=transition=dissolve:duration={}:offset=0[outv]",
                duration
            ),
            TransitionType::Wipe => format!(
                "[0:v][1:v]xfade=transition=wipeleft:duration={}:offset=0[outv]",
                duration
            ),
        };

        let mut command = TokioCommand::new(&self.ffmpeg_path);
        command
            .arg("-i")
            .arg(input1.as_ref())
            .arg("-i")
            .arg(input2.as_ref())
            .arg("-filter_complex")
            .arg(&filter_complex)
            .arg("-map")
            .arg("[outv]");
        for arg in self.optimal_encoder.get_ffmpeg_args() {
            command.arg(arg);
        }
        command.args(["-c:a", "aac", "-b:a", "192k"]);
        command.arg("-y").arg(output);

        execute_ffmpeg_command(&mut command).await?;
        Ok(output.to_path_buf())
    }

    /// Apply color grading and visual enhancements
    pub async fn apply_color_grading(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        grading: ColorGrading,
    ) -> Result<PathBuf> {
        let output = output_path.as_ref();

        info!(
            "Applying color grading: {:?} -> {:?}",
            input_path.as_ref(),
            output
        );

        validate_effect_io(&[input_path.as_ref()], output)?;

        let mut filters = Vec::new();

        // FIX #9: Build a single eq= filter with all parameters combined,
        // instead of separate eq= filters where only the last one takes effect.
        {
            let mut eq_parts: Vec<String> = Vec::new();
            if grading.brightness != 0.0 {
                eq_parts.push(format!("brightness={}", grading.brightness));
            }
            if grading.contrast != 1.0 {
                eq_parts.push(format!("contrast={}", grading.contrast));
            }
            if grading.saturation != 1.0 {
                eq_parts.push(format!("saturation={}", grading.saturation));
            }
            if grading.gamma != 1.0 {
                eq_parts.push(format!("gamma={}", grading.gamma));
            }
            if !eq_parts.is_empty() {
                filters.push(format!("eq={}", eq_parts.join(":")));
            }
        }
        if grading.vignette {
            filters.push("vignette='ih*0.5:ih*0.5:0.8'".to_string());
        }

        let filter_string = if filters.is_empty() {
            "copy".to_string()
        } else {
            filters.join(",")
        };

        let mut command = TokioCommand::new(&self.ffmpeg_path);
        command
            .arg("-i")
            .arg(input_path.as_ref())
            .arg("-vf")
            .arg(&filter_string);
        for arg in self.optimal_encoder.get_ffmpeg_args() {
            command.arg(arg);
        }
        command.args(["-c:a", "aac", "-b:a", "192k"]);
        command.arg("-y").arg(output);

        execute_ffmpeg_command(&mut command).await?;
        Ok(output.to_path_buf())
    }

    /// Generate slow-motion effect
    pub async fn apply_slow_motion(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        speed_factor: f64,
    ) -> Result<PathBuf> {
        let output = output_path.as_ref();

        info!(
            "Applying slow motion ({}x speed): {:?} -> {:?}",
            speed_factor,
            input_path.as_ref(),
            output
        );

        if speed_factor >= 1.0 {
            return Err(VideoError::ProcessingError {
                message: "Speed factor must be less than 1.0 for slow motion".to_string(),
            });
        }

        validate_effect_io(&[input_path.as_ref()], output)?;

        let video_filter = format!("setpts={}*PTS", 1.0 / speed_factor);
        // FIX #8: Also adjust audio tempo to match slow motion speed
        let audio_filter = build_atempo_chain(speed_factor);

        let mut command = TokioCommand::new(&self.ffmpeg_path);
        command
            .arg("-i")
            .arg(input_path.as_ref())
            .arg("-filter:v")
            .arg(&video_filter)
            .arg("-filter:a")
            .arg(&audio_filter)
            .arg("-r")
            .arg("60");
        for arg in self.optimal_encoder.get_ffmpeg_args() {
            command.arg(arg);
        }
        command.args(["-c:a", "aac", "-b:a", "192k"]);
        command.arg("-y").arg(output);

        execute_ffmpeg_command(&mut command).await?;
        Ok(output.to_path_buf())
    }

    /// Add text overlay with styling
    pub async fn add_text_overlay(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        text: &str,
        position: TextPosition,
        style: TextStyle,
    ) -> Result<PathBuf> {
        let output = output_path.as_ref();

        info!(
            "Adding text overlay '{}': {:?} -> {:?}",
            text,
            input_path.as_ref(),
            output
        );

        validate_effect_io(&[input_path.as_ref()], output)?;

        let (x, y) = match position {
            TextPosition::TopLeft => ("10", "10"),
            TextPosition::TopRight => ("w-text_w-10", "10"),
            TextPosition::BottomLeft => ("10", "h-text_h-10"),
            TextPosition::BottomRight => ("w-text_w-10", "h-text_h-10"),
            TextPosition::Center => ("(w-text_w)/2", "(h-text_h)/2"),
        };

        let safe_text = escape_ffmpeg_text(text);
        let safe_color = validate_ffmpeg_color(&style.color)?;

        let filter = format!(
            "drawtext=text='{}':x={}:y={}:fontsize={}:fontcolor={}:shadowx=2:shadowy=2",
            safe_text, x, y, style.size, safe_color
        );

        let mut command = TokioCommand::new(&self.ffmpeg_path);
        command
            .arg("-i")
            .arg(input_path.as_ref())
            .arg("-vf")
            .arg(&filter);
        for arg in self.optimal_encoder.get_ffmpeg_args() {
            command.arg(arg);
        }
        command.args(["-c:a", "aac", "-b:a", "192k"]);
        command.arg("-y").arg(output);

        execute_ffmpeg_command(&mut command).await?;
        Ok(output.to_path_buf())
    }

    /// Apply multiple effects (slow-motion, color grading, text overlay) in a single FFmpeg pass
    pub async fn apply_chained_effects(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
        effects: ChainedEffects,
    ) -> Result<PathBuf> {
        let input = input_path.as_ref();
        let output = output_path.as_ref();

        info!("Applying chained effects: {:?} -> {:?}", input, output);

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

        // Validate speed factor if provided
        if let Some(speed) = effects.slow_motion {
            if speed <= 0.0 || speed >= 1.0 {
                return Err(VideoError::ProcessingError {
                    message: "Speed factor must be between 0.0 (exclusive) and 1.0 (exclusive) for slow motion".to_string(),
                });
            }
        }

        // Build filter chain: combine all enabled effects with commas
        let mut filters: Vec<String> = Vec::new();

        // Slow motion: setpts=1/{speed}*PTS
        if let Some(speed) = effects.slow_motion {
            filters.push(format!("setpts={}*PTS", 1.0 / speed));
        }

        // Color grading: eq=brightness={b}:contrast={c}:saturation={s}
        if let Some(ref grading) = effects.color_grading {
            let mut eq_parts: Vec<String> = Vec::new();
            if grading.brightness != 0.0 {
                eq_parts.push(format!("brightness={}", grading.brightness));
            }
            if grading.contrast != 1.0 {
                eq_parts.push(format!("contrast={}", grading.contrast));
            }
            if grading.saturation != 1.0 {
                eq_parts.push(format!("saturation={}", grading.saturation));
            }
            if grading.gamma != 1.0 {
                eq_parts.push(format!("gamma={}", grading.gamma));
            }
            if !eq_parts.is_empty() {
                filters.push(format!("eq={}", eq_parts.join(":")));
            }
            if grading.vignette {
                filters.push("vignette='ih*0.5:ih*0.5:0.8'".to_string());
            }
        }

        // Text overlay: drawtext=text='{text}':x={x}:y={y}:fontsize={size}:fontcolor={color}
        if let Some(ref text_cfg) = effects.text_overlay {
            let (x, y) = match text_cfg.position.to_lowercase().as_str() {
                "topleft" | "top_left" => ("10", "10"),
                "topright" | "top_right" => ("w-text_w-10", "10"),
                "bottomleft" | "bottom_left" => ("10", "h-text_h-10"),
                "bottomright" | "bottom_right" => ("w-text_w-10", "h-text_h-10"),
                _ => ("(w-text_w)/2", "(h-text_h)/2"), // center
            };

            let safe_text = escape_ffmpeg_text(&text_cfg.text);
            let safe_color = validate_ffmpeg_color(&text_cfg.color)?;

            filters.push(format!(
                "drawtext=text='{}':x={}:y={}:fontsize={}:fontcolor={}:shadowx=2:shadowy=2",
                safe_text, x, y, text_cfg.size, safe_color
            ));
        }

        // If no effects are enabled, just copy
        let filter_string = if filters.is_empty() {
            "copy".to_string()
        } else {
            filters.join(",")
        };

        let mut command = TokioCommand::new(&self.ffmpeg_path);
        command.arg("-i").arg(input);

        // Use -filter:v (alias -vf) for the combined filter chain
        command.arg("-filter:v").arg(&filter_string);

        // If slow motion is active, also adjust audio tempo.
        // FFmpeg atempo only accepts [0.5, 100.0], so chain filters for lower speeds.
        if let Some(speed) = effects.slow_motion {
            command.arg("-filter:a").arg(build_atempo_chain(speed));
        }

        for arg in self.optimal_encoder.get_ffmpeg_args() {
            command.arg(arg);
        }
        command.args(["-c:a", "aac", "-b:a", "192k"]);
        command.arg("-y").arg(output);

        execute_ffmpeg_command(&mut command).await?;

        info!("Chained effects applied successfully: {:?}", output);
        Ok(output.to_path_buf())
    }
}
