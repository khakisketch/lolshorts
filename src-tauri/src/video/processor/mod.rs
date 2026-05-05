#![allow(dead_code)]

pub mod effects;
pub mod pipeline;
pub mod types;

pub use effects::{escape_ffmpeg_text, validate_ffmpeg_color};
pub use pipeline::VideoProcessor;
pub use types::{
    ChainedEffects, ColorGrading, LoudnessInfo, TextOverlayConfig, TextPosition, TextStyle,
    TransitionType, VideoEncoder,
};
