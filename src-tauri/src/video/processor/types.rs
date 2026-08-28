#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single source clip for `VideoProcessor::compose_with_options`, with
/// optional input-level trimming (mapped to FFmpeg `-ss` / `-t` before `-i`
/// so trim + concat + scale happen in a single re-encode pass — no separate
/// extract pass, no extra generation loss).
#[derive(Debug, Clone)]
pub struct ClipSpec {
    pub path: PathBuf,
    /// Seconds; `None` = from the clip start.
    pub trim_start: Option<f64>,
    /// Seconds; `None` = to the clip end.
    pub trim_duration: Option<f64>,
}

/// Options for `VideoProcessor::compose_with_options` (offline composition).
#[derive(Debug, Clone)]
pub struct ComposeOptions {
    pub width: u32,
    pub height: u32,
    /// `("fade"|"slide"|"dissolve"|"wipeleft", duration_secs)`. `None` = hard cut.
    pub transition: Option<(String, f64)>,
    /// Zoom event times (seconds) on the *composed* output timeline.
    pub event_times: Option<Vec<f64>>,
    /// Source fps used by zoompan and fps normalization (default 60).
    pub fps: Option<u32>,
    /// `Some(target_lufs)` applies a final 2-pass loudnorm to the composite.
    pub normalize_audio: Option<f64>,
    /// 클립별 훅 자막. `clip_specs` 와 인덱스가 맞는다. `None` = 자막 없음.
    ///
    /// 각 클립의 **앞 몇 초** 동안 "왜 이 장면이 여기 있는지"를 한 줄로 띄운다
    /// ("펜타킬 · 1v3 · 체력 8%"). 클립 단위로 넣는 이유는, 합친 뒤에 넣으면
    /// 어느 구간이 어느 클립인지 계산해 `enable=between(...)` 을 손으로 맞춰야
    /// 하는데 그 계산은 전환(xfade)이 붙는 순간 어긋나기 때문이다. 합치기 **전**
    /// 각 입력에 걸면 `t` 가 그 클립 안에서의 시각이라 어긋날 여지가 없다.
    #[allow(clippy::type_complexity)]
    pub captions: Option<Vec<Option<CaptionSpec>>>,
    /// Deterministic presentation used for vertical game footage.
    pub framing: VerticalFraming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalFraming {
    LolFocusStack,
    SafeFullFrame,
    CenterCrop,
}

/// 한 클립 위에 띄울 훅 자막.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionSpec {
    /// 굵게 나갈 첫 줄 — 무슨 장면인가("펜타킬").
    pub title: String,
    /// 그 아래 작게 — 왜 볼 만한가("혼자서 · 1v3 · 체력 8%"). 없으면 제목만.
    pub detail: Option<String>,
    /// 클립 시작부터 몇 초 동안 보일지.
    pub duration_secs: f64,
}

/// Types for video transitions and effects
#[derive(Debug, Clone)]
pub enum TransitionType {
    Fade,
    Slide,
    Dissolve,
    Wipe,
}

/// Hardware-accelerated video encoder types
#[derive(Debug, Clone)]
pub enum VideoEncoder {
    H264,       // Software H.264
    H265,       // Software H.265
    NvidiaH264, // NVIDIA NVENC H.264
    NvidiaH265, // NVIDIA NVENC H.265
    AmdH264,    // AMD VCE H.264
    IntelH264,  // Intel Quick Sync H.264
}

impl VideoEncoder {
    /// FFmpeg encoder arguments for **offline** processing (export / compose /
    /// extract), tuned for quality rather than latency.
    ///
    /// Each variant is self-contained on quality: software encoders carry their
    /// own `-crf`, and hardware encoders carry their own `-cq` / `-qp` /
    /// `-global_quality`. Callers MUST NOT append a blanket `-crf` after these
    /// args — that is unsupported by nvenc/amf/qsv and hard-fails the encode.
    ///
    /// These are NOT the low-latency args used for realtime recording; the live
    /// segment recorder (`recording::integration_backend::segment_recorder`)
    /// owns its own `-tune ll`-style arguments.
    pub fn get_ffmpeg_args(&self) -> Vec<&'static str> {
        match self {
            // -crf 23 is the software quality target; -preset medium balances
            // speed/size. fastdecode tune is dropped for offline (it hurts quality).
            VideoEncoder::H264 => vec!["-c:v", "libx264", "-preset", "medium", "-crf", "23"],
            VideoEncoder::H265 => vec!["-c:v", "libx265", "-preset", "medium", "-crf", "28"],
            // p5 + hq tune favors quality over latency for offline export.
            // -rc:v vbr with -b:v 0 enables pure CQ mode; -bf/-rc-lookahead
            // improve compression at safe (Maxwell+) levels.
            VideoEncoder::NvidiaH264 => vec![
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p5",
                "-tune",
                "hq",
                "-rc:v",
                "vbr",
                "-b:v",
                "0",
                "-cq",
                "23",
                "-bf",
                "3",
                "-rc-lookahead",
                "20",
            ],
            VideoEncoder::NvidiaH265 => vec![
                "-c:v",
                "hevc_nvenc",
                "-preset",
                "p5",
                "-tune",
                "hq",
                "-rc:v",
                "vbr",
                "-b:v",
                "0",
                "-cq",
                "28",
                "-bf",
                "3",
                "-rc-lookahead",
                "20",
            ],
            // "quality" preset favors quality over the "balanced"/"speed" presets.
            VideoEncoder::AmdH264 => vec![
                "-c:v", "h264_amf", "-quality", "quality", "-rc", "vbr_peak", "-qp_i", "23",
                "-qp_p", "23", "-qp_b", "23",
            ],
            // "slow" preset favors quality; ICQ via -global_quality (no -tune ll offline).
            VideoEncoder::IntelH264 => vec![
                "-c:v",
                "h264_qsv",
                "-preset",
                "slow",
                "-global_quality",
                "23",
            ],
        }
    }

    pub fn get_name(&self) -> &'static str {
        match self {
            VideoEncoder::H264 => "H.264 (Software)",
            VideoEncoder::H265 => "H.265 (Software)",
            VideoEncoder::NvidiaH264 => "NVIDIA H.264 (Hardware)",
            VideoEncoder::NvidiaH265 => "NVIDIA H.265 (Hardware)",
            VideoEncoder::AmdH264 => "AMD H.264 (Hardware)",
            VideoEncoder::IntelH264 => "Intel H.264 (Hardware)",
        }
    }

    pub fn is_hardware_accelerated(&self) -> bool {
        matches!(
            self,
            VideoEncoder::NvidiaH264
                | VideoEncoder::NvidiaH265
                | VideoEncoder::AmdH264
                | VideoEncoder::IntelH264
        )
    }

    pub fn get_output_format(&self) -> &'static str {
        match self {
            VideoEncoder::H264 | VideoEncoder::H265 => "yuv420p",
            VideoEncoder::NvidiaH264 | VideoEncoder::AmdH264 | VideoEncoder::IntelH264 => "cuda",
            VideoEncoder::NvidiaH265 => "cuda",
        }
    }
}

/// Audio loudness measurement from FFmpeg loudnorm first-pass analysis
#[derive(Debug, Clone, PartialEq)]
pub struct LoudnessInfo {
    /// Integrated loudness (LUFS)
    pub input_i: f64,
    /// True peak (dBTP)
    pub input_tp: f64,
    /// Loudness range (LU)
    pub input_lra: f64,
    /// Threshold (LUFS)
    pub input_thresh: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorGrading {
    pub brightness: f64, // -1.0 to 1.0
    pub contrast: f64,   // 0.0 to 2.0
    pub saturation: f64, // 0.0 to 2.0
    pub gamma: f64,      // 0.1 to 3.0
    pub vignette: bool,  // Add vignette effect
}

impl Default for ColorGrading {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            gamma: 1.0,
            vignette: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TextPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

#[derive(Debug, Clone)]
pub struct TextStyle {
    pub size: u32,
    pub color: String,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            size: 48,
            color: "WHITE".to_string(),
        }
    }
}

/// Configuration for text overlay in chained effects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextOverlayConfig {
    pub text: String,
    pub position: String, // "center", "topleft", "topright", "bottomleft", "bottomright"
    pub size: u32,
    pub color: String,
}

/// Multiple effects to apply in a single FFmpeg pass
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChainedEffects {
    pub slow_motion: Option<f64>,                // speed factor (0.25-0.75)
    pub color_grading: Option<ColorGrading>,     // brightness, contrast, saturation
    pub text_overlay: Option<TextOverlayConfig>, // text, position, size, color
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- VideoEncoder::get_name ----

    #[test]
    fn video_encoder_h264_get_name_contains_h264() {
        let name = VideoEncoder::H264.get_name();
        assert!(
            name.contains("H.264"),
            "expected H.264 in name, got: {}",
            name
        );
    }

    #[test]
    fn video_encoder_h265_get_name_contains_h265() {
        let name = VideoEncoder::H265.get_name();
        assert!(
            name.contains("H.265"),
            "expected H.265 in name, got: {}",
            name
        );
    }

    #[test]
    fn video_encoder_nvidia_h264_get_name_contains_nvidia() {
        let name = VideoEncoder::NvidiaH264.get_name();
        assert!(
            name.contains("NVIDIA"),
            "expected NVIDIA in name, got: {}",
            name
        );
    }

    #[test]
    fn video_encoder_nvidia_h265_get_name_contains_nvidia() {
        let name = VideoEncoder::NvidiaH265.get_name();
        assert!(
            name.contains("NVIDIA"),
            "expected NVIDIA in name, got: {}",
            name
        );
    }

    #[test]
    fn video_encoder_amd_h264_get_name_contains_amd() {
        let name = VideoEncoder::AmdH264.get_name();
        assert!(name.contains("AMD"), "expected AMD in name, got: {}", name);
    }

    #[test]
    fn video_encoder_intel_h264_get_name_contains_intel() {
        let name = VideoEncoder::IntelH264.get_name();
        assert!(
            name.contains("Intel"),
            "expected Intel in name, got: {}",
            name
        );
    }

    // ---- VideoEncoder::get_ffmpeg_args ----

    #[test]
    fn video_encoder_h264_ffmpeg_args_use_libx264() {
        let args = VideoEncoder::H264.get_ffmpeg_args();
        assert!(
            args.contains(&"libx264"),
            "H264 args should contain libx264"
        );
    }

    #[test]
    fn video_encoder_h265_ffmpeg_args_use_libx265() {
        let args = VideoEncoder::H265.get_ffmpeg_args();
        assert!(
            args.contains(&"libx265"),
            "H265 args should contain libx265"
        );
    }

    #[test]
    fn video_encoder_nvidia_h264_ffmpeg_args_use_h264_nvenc() {
        let args = VideoEncoder::NvidiaH264.get_ffmpeg_args();
        assert!(
            args.contains(&"h264_nvenc"),
            "NvidiaH264 args should contain h264_nvenc"
        );
    }

    #[test]
    fn video_encoder_nvidia_h265_ffmpeg_args_use_hevc_nvenc() {
        let args = VideoEncoder::NvidiaH265.get_ffmpeg_args();
        assert!(
            args.contains(&"hevc_nvenc"),
            "NvidiaH265 args should contain hevc_nvenc"
        );
    }

    #[test]
    fn video_encoder_amd_h264_ffmpeg_args_use_h264_amf() {
        let args = VideoEncoder::AmdH264.get_ffmpeg_args();
        assert!(
            args.contains(&"h264_amf"),
            "AmdH264 args should contain h264_amf"
        );
    }

    #[test]
    fn video_encoder_intel_h264_ffmpeg_args_use_h264_qsv() {
        let args = VideoEncoder::IntelH264.get_ffmpeg_args();
        assert!(
            args.contains(&"h264_qsv"),
            "IntelH264 args should contain h264_qsv"
        );
    }

    #[test]
    fn video_encoder_ffmpeg_args_always_include_codec_flag() {
        let encoders = [
            VideoEncoder::H264,
            VideoEncoder::H265,
            VideoEncoder::NvidiaH264,
            VideoEncoder::NvidiaH265,
            VideoEncoder::AmdH264,
            VideoEncoder::IntelH264,
        ];
        for encoder in encoders {
            let args = encoder.get_ffmpeg_args();
            assert!(
                args.contains(&"-c:v"),
                "{} ffmpeg args should contain -c:v",
                encoder.get_name()
            );
        }
    }

    #[test]
    fn video_encoder_ffmpeg_args_returns_non_empty_vec() {
        assert!(!VideoEncoder::H264.get_ffmpeg_args().is_empty());
        assert!(!VideoEncoder::NvidiaH265.get_ffmpeg_args().is_empty());
    }

    #[test]
    fn video_encoder_hardware_detection_matches_accelerated_variants() {
        assert!(!VideoEncoder::H264.is_hardware_accelerated());
        assert!(!VideoEncoder::H265.is_hardware_accelerated());
        assert!(VideoEncoder::NvidiaH264.is_hardware_accelerated());
        assert!(VideoEncoder::NvidiaH265.is_hardware_accelerated());
        assert!(VideoEncoder::AmdH264.is_hardware_accelerated());
        assert!(VideoEncoder::IntelH264.is_hardware_accelerated());
    }

    // ---- VideoEncoder::get_output_format ----

    #[test]
    fn video_encoder_h264_output_format_is_yuv420p() {
        assert_eq!(VideoEncoder::H264.get_output_format(), "yuv420p");
    }

    #[test]
    fn video_encoder_h265_output_format_is_yuv420p() {
        assert_eq!(VideoEncoder::H265.get_output_format(), "yuv420p");
    }

    #[test]
    fn video_encoder_nvidia_h264_output_format_is_cuda() {
        assert_eq!(VideoEncoder::NvidiaH264.get_output_format(), "cuda");
    }

    #[test]
    fn video_encoder_nvidia_h265_output_format_is_cuda() {
        assert_eq!(VideoEncoder::NvidiaH265.get_output_format(), "cuda");
    }

    #[test]
    fn video_encoder_amd_h264_output_format_is_cuda() {
        assert_eq!(VideoEncoder::AmdH264.get_output_format(), "cuda");
    }

    #[test]
    fn video_encoder_intel_h264_output_format_is_cuda() {
        assert_eq!(VideoEncoder::IntelH264.get_output_format(), "cuda");
    }

    // ---- ColorGrading ----

    #[test]
    fn color_grading_default_brightness_is_zero() {
        let cg = ColorGrading::default();
        assert_eq!(cg.brightness, 0.0);
    }

    #[test]
    fn color_grading_default_contrast_is_one() {
        let cg = ColorGrading::default();
        assert_eq!(cg.contrast, 1.0);
    }

    #[test]
    fn color_grading_default_saturation_is_one() {
        let cg = ColorGrading::default();
        assert_eq!(cg.saturation, 1.0);
    }

    #[test]
    fn color_grading_default_gamma_is_one() {
        let cg = ColorGrading::default();
        assert_eq!(cg.gamma, 1.0);
    }

    #[test]
    fn color_grading_default_vignette_is_false() {
        let cg = ColorGrading::default();
        assert!(!cg.vignette);
    }

    // ---- TextStyle ----

    #[test]
    fn text_style_default_size_is_48() {
        let style = TextStyle::default();
        assert_eq!(style.size, 48);
    }

    #[test]
    fn text_style_default_color_is_white() {
        let style = TextStyle::default();
        assert_eq!(style.color, "WHITE");
    }

    // ---- TransitionType ----

    #[test]
    fn transition_type_variants_can_be_constructed() {
        let _fade = TransitionType::Fade;
        let _slide = TransitionType::Slide;
        let _dissolve = TransitionType::Dissolve;
        let _wipe = TransitionType::Wipe;
    }

    #[test]
    fn transition_type_can_be_cloned() {
        let t = TransitionType::Fade;
        let _cloned = t.clone();
    }
}
