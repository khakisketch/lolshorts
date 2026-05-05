#![allow(dead_code)]

use serde::{Deserialize, Serialize};

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
    pub fn get_ffmpeg_args(&self) -> Vec<&'static str> {
        match self {
            VideoEncoder::H264 => vec![
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-tune",
                "fastdecode",
            ],
            VideoEncoder::H265 => vec![
                "-c:v",
                "libx265",
                "-preset",
                "medium",
                "-tune",
                "fastdecode",
            ],
            // p4 balances quality and speed (p1=fastest/worst, p7=slowest/best)
            // -rc:v vbr uses modern FFmpeg syntax; -b:v 0 enables pure CQ mode
            VideoEncoder::NvidiaH264 => vec![
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p4",
                "-tune",
                "ll",
                "-rc:v",
                "vbr",
                "-b:v",
                "0",
                "-cq",
                "23",
            ],
            VideoEncoder::NvidiaH265 => vec![
                "-c:v",
                "hevc_nvenc",
                "-preset",
                "p4",
                "-tune",
                "ll",
                "-rc:v",
                "vbr",
                "-b:v",
                "0",
                "-cq",
                "28",
            ],
            // balanced trades a few% speed for better quality vs speed preset
            VideoEncoder::AmdH264 => vec![
                "-c:v", "h264_amf", "-quality", "balanced", "-rc", "vbr_peak", "-qp_i", "23",
                "-qp_p", "23",
            ],
            // faster is more broadly supported than veryfast across QSV driver versions
            VideoEncoder::IntelH264 => vec![
                "-c:v",
                "h264_qsv",
                "-preset",
                "faster",
                "-tune",
                "ll",
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
