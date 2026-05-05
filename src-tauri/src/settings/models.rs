use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete recording settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSettings {
    pub event_filter: EventFilterSettings,
    pub game_mode: GameModeSettings,
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub clip_timing: ClipTimingSettings,
    pub hotkeys: HotkeySettings,
    #[serde(default)]
    pub storage: StorageSettings,

    // General settings
    pub auto_start_with_league: bool,
    pub minimize_to_tray: bool,
    pub show_notifications: bool,
    #[serde(default = "default_show_replay_popup")]
    pub show_replay_popup: bool,
    #[serde(default)]
    pub crash_reporting_enabled: bool,
    #[serde(default = "default_true")]
    pub overlay_enabled: bool,
}

fn default_show_replay_popup() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_audio_target_lufs() -> f64 {
    -14.0 // YouTube standard
}

fn default_min_game_duration() -> u32 {
    300 // 5 minutes
}

fn default_contest_window() -> u32 {
    10 // seconds
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self {
            event_filter: EventFilterSettings::default(),
            game_mode: GameModeSettings::default(),
            video: VideoSettings::default(),
            audio: AudioSettings::default(),
            clip_timing: ClipTimingSettings::default(),
            hotkeys: HotkeySettings::default(),
            storage: StorageSettings::default(),

            auto_start_with_league: true,
            minimize_to_tray: true,
            show_notifications: true,
            show_replay_popup: true,
            crash_reporting_enabled: false,
            overlay_enabled: false,
        }
    }
}

impl RecordingSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.video.validate()?;
        self.audio.validate()?;
        self.storage.validate()?;
        self.event_filter.validate()?;
        Ok(())
    }
}

// ============================================================================
// Storage Settings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettings {
    /// Enable automatic deletion of old clips
    pub auto_delete_enabled: bool,
    /// Delete clips older than this many days
    pub auto_delete_days: u32,
    /// Maximum total clip storage in GB before oldest clips are deleted
    pub max_storage_gb: u32,
    /// Whether to also delete clips that have been exported/uploaded
    pub delete_exported_clips: bool,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            auto_delete_enabled: false,
            auto_delete_days: 30,
            max_storage_gb: 50,
            delete_exported_clips: false,
        }
    }
}

impl StorageSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.auto_delete_days == 0 || self.auto_delete_days > 365 {
            return Err(format!(
                "auto_delete_days {} out of range 1-365",
                self.auto_delete_days
            ));
        }
        if self.max_storage_gb == 0 || self.max_storage_gb > 10_000 {
            return Err(format!(
                "max_storage_gb {} out of range 1-10000",
                self.max_storage_gb
            ));
        }
        Ok(())
    }
}

// ============================================================================
// Event Filter Settings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilterSettings {
    // 킬 관련
    pub record_kills: bool,
    pub record_multikills: bool,
    pub record_first_blood: bool,

    // 데스 관련
    pub record_deaths: bool,
    pub record_shutdown: bool,

    // 어시스트 관련
    pub record_assists: bool,

    // 오브젝트
    pub record_dragon: bool,
    pub record_baron: bool,
    pub record_elder: bool,
    pub record_herald: bool,

    // 구조물
    pub record_turret: bool,
    pub record_inhibitor: bool,
    pub record_nexus: bool,

    // 특수 이벤트
    pub record_ace: bool,
    pub record_game_end: bool,
    pub record_steal: bool,

    // 추가 오브젝트
    #[serde(default = "default_true")]
    pub record_voidgrubs: bool,
    #[serde(default = "default_true")]
    pub record_atakhan: bool,

    // 고급 이벤트 감지
    #[serde(default = "default_true")]
    pub record_outplay: bool, // 1vX outplay detection
    #[serde(default = "default_true")]
    pub record_trade_kill: bool, // Trade kill detection (kill then die)
    #[serde(default = "default_true")]
    pub record_low_hp: bool, // Low HP outplay detection

    // 우선순위 필터
    pub min_priority: u8, // 1-5

    // Task 29: 게임 최소 시간 필터 (리메이크/짧은 게임 제외)
    #[serde(default = "default_min_game_duration")]
    pub min_game_duration_secs: u32, // 0-3600, default 300 (5 minutes)

    // Task 30: 스틸 감지 컨테스트 윈도우 (초)
    #[serde(default = "default_contest_window")]
    pub contest_window_secs: u32, // 5-30, default 10
}

impl Default for EventFilterSettings {
    fn default() -> Self {
        Self {
            // 기본적으로 하이라이트만 녹화
            record_kills: true,
            record_multikills: true,
            record_first_blood: true,

            record_deaths: false, // 데스는 기본적으로 OFF
            record_shutdown: false,

            record_assists: false, // 어시스트는 기본적으로 OFF

            record_dragon: true,
            record_baron: true,
            record_elder: true,
            record_herald: true,

            record_turret: false, // 타워는 너무 많아서 OFF
            record_inhibitor: true,
            record_nexus: true,

            record_ace: true,
            record_game_end: true,
            record_steal: true,

            record_voidgrubs: true,
            record_atakhan: true,

            record_outplay: true,
            record_trade_kill: true,
            record_low_hp: true,

            min_priority: 1, // Allow all events including single kills
            min_game_duration_secs: default_min_game_duration(),
            contest_window_secs: default_contest_window(),
        }
    }
}

impl EventFilterSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.min_priority < 1 || self.min_priority > 5 {
            return Err(format!(
                "min_priority {} out of range 1-5",
                self.min_priority
            ));
        }
        // Task 29: validate min_game_duration_secs range 0-3600
        if self.min_game_duration_secs > 3600 {
            return Err(format!(
                "min_game_duration_secs {} out of range 0-3600",
                self.min_game_duration_secs
            ));
        }
        // Task 30: validate contest_window_secs range 5-30
        if self.contest_window_secs < 5 || self.contest_window_secs > 30 {
            return Err(format!(
                "contest_window_secs {} out of range 5-30",
                self.contest_window_secs
            ));
        }
        Ok(())
    }
}

// ============================================================================
// Game Mode Settings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameModeSettings {
    pub record_ranked_solo: bool,
    pub record_ranked_flex: bool,
    pub record_normal: bool,
    pub record_quick_play: bool,
    pub record_aram: bool,
    pub record_arena: bool,
    pub record_special: bool,
    pub record_custom: bool,
    pub record_practice: bool,
}

impl Default for GameModeSettings {
    fn default() -> Self {
        Self {
            record_ranked_solo: true,
            record_ranked_flex: true,
            record_normal: true,
            record_quick_play: true,
            record_aram: true,
            record_arena: true,
            record_special: false,  // 특별 모드는 기본 OFF
            record_custom: false,   // 커스텀은 기본 OFF
            record_practice: false, // 연습은 기본 OFF
        }
    }
}

// ============================================================================
// Video Settings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    pub resolution: Resolution,
    pub frame_rate: FrameRate,
    pub bitrate_preset: BitratePreset,
    pub codec: VideoCodec,
    pub encoder: EncoderPreference,
    /// Monitor index for capture (0 = primary monitor)
    #[serde(default)]
    pub monitor_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    R1920x1080, // 1080p (추천)
    R2560x1440, // 1440p
    R3840x2160, // 4K
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FrameRate {
    Fps30,
    Fps60, // 추천
    Fps120,
    Fps144,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BitratePreset {
    Low,         // 10 Mbps (720p60)
    Medium,      // 20 Mbps (1080p60) - 추천
    High,        // 40 Mbps (1440p60)
    VeryHigh,    // 80 Mbps (4K60)
    Custom(u32), // 사용자 지정 (kbps)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264, // 호환성 최고
    H265, // 효율성 최고 (추천)
    Av1,  // 차세대 (실험적)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EncoderPreference {
    Auto,     // 자동 선택 (추천)
    Nvenc,    // NVIDIA GPU
    Qsv,      // Intel GPU
    Amf,      // AMD GPU
    Software, // CPU (느림, 호환성 높음)
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            resolution: Resolution::R1920x1080,
            frame_rate: FrameRate::Fps60,
            bitrate_preset: BitratePreset::Medium,
            codec: VideoCodec::H265,
            encoder: EncoderPreference::Auto,
            monitor_index: 0,
        }
    }
}

impl VideoSettings {
    /// Convert resolution to (width, height) tuple
    pub fn get_resolution(&self) -> (u32, u32) {
        match self.resolution {
            Resolution::R1920x1080 => (1920, 1080),
            Resolution::R2560x1440 => (2560, 1440),
            Resolution::R3840x2160 => (3840, 2160),
        }
    }

    /// Convert frame rate to u32
    pub fn get_fps(&self) -> u32 {
        match self.frame_rate {
            FrameRate::Fps30 => 30,
            FrameRate::Fps60 => 60,
            FrameRate::Fps120 => 120,
            FrameRate::Fps144 => 144,
        }
    }

    /// Convert bitrate preset to actual bitrate in bps
    pub fn get_bitrate(&self) -> u32 {
        match &self.bitrate_preset {
            BitratePreset::Low => 10_000_000,           // 10 Mbps
            BitratePreset::Medium => 20_000_000,        // 20 Mbps
            BitratePreset::High => 40_000_000,          // 40 Mbps
            BitratePreset::VeryHigh => 80_000_000,      // 80 Mbps
            BitratePreset::Custom(kbps) => kbps * 1000, // Convert kbps to bps
        }
    }

    /// Check if using H.265 codec
    pub fn is_h265(&self) -> bool {
        matches!(self.codec, VideoCodec::H265)
    }

    pub fn validate(&self) -> Result<(), String> {
        if let BitratePreset::Custom(kbps) = &self.bitrate_preset {
            if *kbps < 100 || *kbps > 50_000 {
                return Err(format!("Custom bitrate {} out of range 100-50000", kbps));
            }
        }
        Ok(())
    }
}

// ============================================================================
// Audio Settings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioSettings {
    // 마이크 녹음
    pub record_microphone: bool,
    pub microphone_device: Option<String>,
    pub microphone_volume: u8, // 0-200%

    // 시스템 오디오 녹음
    pub record_system_audio: bool,
    pub system_audio_device: Option<String>,
    pub system_audio_volume: u8, // 0-200%

    // 오디오 품질
    pub sample_rate: SampleRate,
    pub bitrate: AudioBitrate,

    /// Explicit WASAPI device ID for loopback capture (None = system default output device).
    /// Populated from the list returned by `enumerate_audio_devices()`.
    #[serde(default)]
    pub audio_device_id: Option<String>,

    /// Enable LUFS-based audio normalization during video export
    #[serde(default = "default_true")]
    pub audio_normalize: bool,

    /// Target integrated loudness in LUFS (-14.0 = YouTube standard)
    #[serde(default = "default_audio_target_lufs")]
    pub audio_target_lufs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SampleRate {
    Hz44100,
    Hz48000, // 추천
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AudioBitrate {
    Kbps128,
    Kbps192, // 추천
    Kbps256,
    Kbps320,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            record_microphone: true,
            microphone_device: None, // 기본 장치
            microphone_volume: 120,  // 120%

            record_system_audio: true,
            system_audio_device: None, // 기본 장치
            system_audio_volume: 100,  // 100%

            sample_rate: SampleRate::Hz48000,
            bitrate: AudioBitrate::Kbps192,
            audio_device_id: None,
            audio_normalize: true,
            audio_target_lufs: default_audio_target_lufs(),
        }
    }
}

impl AudioSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.microphone_volume > 200 {
            return Err(format!(
                "Microphone volume {} exceeds max 200",
                self.microphone_volume
            ));
        }
        if self.system_audio_volume > 200 {
            return Err(format!(
                "System audio volume {} exceeds max 200",
                self.system_audio_volume
            ));
        }
        if self.audio_target_lufs > 0.0 || self.audio_target_lufs < -70.0 {
            return Err(format!(
                "audio_target_lufs {:.1} out of range -70.0..0.0",
                self.audio_target_lufs
            ));
        }
        Ok(())
    }

    /// Convert to recording::audio::AudioConfig
    pub fn to_audio_config(&self) -> crate::recording::audio::AudioConfig {
        crate::recording::audio::AudioConfig {
            record_microphone: self.record_microphone,
            microphone_device: self.microphone_device.clone(),
            microphone_volume: self.microphone_volume,

            record_system_audio: self.record_system_audio,
            system_audio_device: self.system_audio_device.clone(),
            system_audio_volume: self.system_audio_volume,

            sample_rate: match self.sample_rate {
                SampleRate::Hz44100 => 44100,
                SampleRate::Hz48000 => 48000,
            },
            bitrate: match self.bitrate {
                AudioBitrate::Kbps128 => 128,
                AudioBitrate::Kbps192 => 192,
                AudioBitrate::Kbps256 => 256,
                AudioBitrate::Kbps320 => 320,
            },
        }
    }
}

// ============================================================================
// Clip Timing Settings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipTimingSettings {
    // 기본 클립 길이
    pub default_pre_duration: u32,  // 이벤트 이전 (초)
    pub default_post_duration: u32, // 이벤트 이후 (초)

    // 이벤트별 커스텀 타이밍
    pub event_timings: HashMap<String, EventTiming>,

    // 이벤트 병합
    pub merge_consecutive_events: bool,
    pub merge_time_threshold: f64, // 15초 기본
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTiming {
    pub pre_duration: u32,
    pub post_duration: u32,
}

impl Default for ClipTimingSettings {
    fn default() -> Self {
        let mut event_timings = HashMap::new();

        // 멀티킬은 길게
        event_timings.insert(
            "multikill".to_string(),
            EventTiming {
                pre_duration: 15,
                post_duration: 5,
            },
        );

        // 스틸은 더 길게 (빌드업 포함)
        event_timings.insert(
            "steal".to_string(),
            EventTiming {
                pre_duration: 20,
                post_duration: 5,
            },
        );

        // 일반 킬은 짧게
        event_timings.insert(
            "kill".to_string(),
            EventTiming {
                pre_duration: 10,
                post_duration: 3,
            },
        );

        Self {
            default_pre_duration: 10,
            default_post_duration: 3,
            event_timings,
            merge_consecutive_events: true,
            merge_time_threshold: 15.0,
        }
    }
}

impl ClipTimingSettings {
    /// Get timing for a specific event type
    pub fn get_timing_for_event(&self, event_type: &str) -> EventTiming {
        self.event_timings
            .get(event_type)
            .cloned()
            .unwrap_or(EventTiming {
                pre_duration: self.default_pre_duration,
                post_duration: self.default_post_duration,
            })
    }
}

// ============================================================================
// Hotkey Settings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeySettings {
    pub manual_save_clip: String, // "F8" 기본
    pub toggle_recording: String, // "F9" 기본
    pub delete_last_clip: String, // "F10" 기본
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            manual_save_clip: "F8".to_string(),
            toggle_recording: "F9".to_string(),
            delete_last_clip: "F10".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = RecordingSettings::default();

        // Event filter defaults
        assert!(settings.event_filter.record_kills);
        assert!(settings.event_filter.record_multikills);
        assert!(!settings.event_filter.record_deaths);
        assert_eq!(settings.event_filter.min_priority, 1);

        // Game mode defaults
        assert!(settings.game_mode.record_ranked_solo);
        assert!(!settings.game_mode.record_practice);

        // Video defaults
        assert!(matches!(settings.video.resolution, Resolution::R1920x1080));
        assert!(matches!(settings.video.frame_rate, FrameRate::Fps60));
        assert!(matches!(settings.video.codec, VideoCodec::H265));

        // Audio defaults
        assert!(settings.audio.record_microphone);
        assert_eq!(settings.audio.microphone_volume, 120);
        assert_eq!(settings.audio.system_audio_volume, 100);

        // Clip timing defaults
        assert_eq!(settings.clip_timing.default_pre_duration, 10);
        assert_eq!(settings.clip_timing.default_post_duration, 3);
        assert!(settings.clip_timing.merge_consecutive_events);
        assert_eq!(settings.clip_timing.merge_time_threshold, 15.0);

        // Hotkey defaults
        assert_eq!(settings.hotkeys.manual_save_clip, "F8");
        assert_eq!(settings.hotkeys.toggle_recording, "F9");
        assert_eq!(settings.hotkeys.delete_last_clip, "F10");
    }

    #[test]
    fn test_event_timing_lookup() {
        let settings = ClipTimingSettings::default();

        let multikill_timing = settings.get_timing_for_event("multikill");
        assert_eq!(multikill_timing.pre_duration, 15);
        assert_eq!(multikill_timing.post_duration, 5);

        let unknown_timing = settings.get_timing_for_event("unknown_event");
        assert_eq!(unknown_timing.pre_duration, 10); // fallback to default
        assert_eq!(unknown_timing.post_duration, 3);
    }

    #[test]
    fn test_serialization() {
        let settings = RecordingSettings::default();

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&settings).unwrap();
        assert!(json.contains("event_filter"));
        assert!(json.contains("game_mode"));

        // Deserialize back
        let deserialized: RecordingSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.event_filter.min_priority,
            settings.event_filter.min_priority
        );
    }

    #[test]
    fn test_video_settings_custom_bitrate_valid() {
        let mut s = VideoSettings::default();
        s.bitrate_preset = BitratePreset::Custom(5000);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_video_settings_custom_bitrate_too_low() {
        let mut s = VideoSettings::default();
        s.bitrate_preset = BitratePreset::Custom(50);
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_video_settings_custom_bitrate_too_high() {
        let mut s = VideoSettings::default();
        s.bitrate_preset = BitratePreset::Custom(60000);
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_audio_volume_max_valid() {
        let mut s = AudioSettings::default();
        s.microphone_volume = 200;
        s.system_audio_volume = 200;
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_storage_settings_valid() {
        let mut s = StorageSettings::default();
        s.auto_delete_days = 30;
        s.max_storage_gb = 100;
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_storage_auto_delete_days_zero_invalid() {
        let mut s = StorageSettings::default();
        s.auto_delete_days = 0;
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_event_min_priority_out_of_range() {
        let mut s = EventFilterSettings::default();
        s.min_priority = 10;
        assert!(s.validate().is_err());
    }

    // ---- VideoSettings validate() error paths ----

    #[test]
    fn test_video_settings_preset_bitrates_always_valid() {
        let mut s = VideoSettings::default();
        for preset in [
            BitratePreset::Low,
            BitratePreset::Medium,
            BitratePreset::High,
            BitratePreset::VeryHigh,
        ] {
            s.bitrate_preset = preset;
            assert!(s.validate().is_ok());
        }
    }

    #[test]
    fn test_video_settings_custom_bitrate_boundary_low() {
        let mut s = VideoSettings::default();
        s.bitrate_preset = BitratePreset::Custom(100);
        assert!(s.validate().is_ok());
        s.bitrate_preset = BitratePreset::Custom(99);
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_video_settings_custom_bitrate_boundary_high() {
        let mut s = VideoSettings::default();
        s.bitrate_preset = BitratePreset::Custom(50_000);
        assert!(s.validate().is_ok());
        s.bitrate_preset = BitratePreset::Custom(50_001);
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_video_settings_get_resolution() {
        let mut s = VideoSettings::default();
        s.resolution = Resolution::R1920x1080;
        assert_eq!(s.get_resolution(), (1920, 1080));
        s.resolution = Resolution::R2560x1440;
        assert_eq!(s.get_resolution(), (2560, 1440));
        s.resolution = Resolution::R3840x2160;
        assert_eq!(s.get_resolution(), (3840, 2160));
    }

    #[test]
    fn test_video_settings_get_fps() {
        let mut s = VideoSettings::default();
        s.frame_rate = FrameRate::Fps30;
        assert_eq!(s.get_fps(), 30);
        s.frame_rate = FrameRate::Fps60;
        assert_eq!(s.get_fps(), 60);
        s.frame_rate = FrameRate::Fps120;
        assert_eq!(s.get_fps(), 120);
        s.frame_rate = FrameRate::Fps144;
        assert_eq!(s.get_fps(), 144);
    }

    #[test]
    fn test_video_settings_get_bitrate() {
        let s = VideoSettings::default(); // Medium = 20 Mbps
        assert_eq!(s.get_bitrate(), 20_000_000);

        let mut s2 = VideoSettings::default();
        s2.bitrate_preset = BitratePreset::Low;
        assert_eq!(s2.get_bitrate(), 10_000_000);

        let mut s3 = VideoSettings::default();
        s3.bitrate_preset = BitratePreset::Custom(1000);
        assert_eq!(s3.get_bitrate(), 1_000_000); // 1000 kbps = 1Mbps
    }

    #[test]
    fn test_video_is_h265() {
        let mut s = VideoSettings::default();
        s.codec = VideoCodec::H265;
        assert!(s.is_h265());
        s.codec = VideoCodec::H264;
        assert!(!s.is_h265());
    }

    // ---- AudioSettings validate() error paths ----

    #[test]
    fn test_audio_microphone_volume_too_high() {
        let mut s = AudioSettings::default();
        s.microphone_volume = 201;
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("Microphone volume"));
    }

    #[test]
    fn test_audio_system_volume_too_high() {
        let mut s = AudioSettings::default();
        s.system_audio_volume = 201;
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("System audio volume"));
    }

    #[test]
    fn test_audio_both_volumes_at_zero_valid() {
        let mut s = AudioSettings::default();
        s.microphone_volume = 0;
        s.system_audio_volume = 0;
        assert!(s.validate().is_ok());
    }

    // ---- StorageSettings validate() error paths ----

    #[test]
    fn test_storage_auto_delete_days_too_high() {
        let mut s = StorageSettings::default();
        s.auto_delete_days = 366;
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("auto_delete_days"));
    }

    #[test]
    fn test_storage_max_storage_gb_zero_invalid() {
        let mut s = StorageSettings::default();
        s.max_storage_gb = 0;
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_storage_max_storage_gb_too_high() {
        let mut s = StorageSettings::default();
        s.max_storage_gb = 10_001;
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_storage_boundary_values_valid() {
        let mut s = StorageSettings::default();
        s.auto_delete_days = 1;
        s.max_storage_gb = 1;
        assert!(s.validate().is_ok());
        s.auto_delete_days = 365;
        s.max_storage_gb = 10_000;
        assert!(s.validate().is_ok());
    }

    // ---- EventFilterSettings validate() error paths ----

    #[test]
    fn test_event_min_priority_zero_invalid() {
        let mut s = EventFilterSettings::default();
        s.min_priority = 0;
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_event_min_priority_boundary_valid() {
        let mut s = EventFilterSettings::default();
        s.min_priority = 1;
        assert!(s.validate().is_ok());
        s.min_priority = 5;
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_contest_window_secs_too_low() {
        let mut s = EventFilterSettings::default();
        s.contest_window_secs = 4;
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("contest_window_secs"));
    }

    #[test]
    fn test_contest_window_secs_too_high() {
        let mut s = EventFilterSettings::default();
        s.contest_window_secs = 31;
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_contest_window_secs_boundary_valid() {
        let mut s = EventFilterSettings::default();
        s.contest_window_secs = 5;
        assert!(s.validate().is_ok());
        s.contest_window_secs = 30;
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_min_game_duration_secs_too_high() {
        let mut s = EventFilterSettings::default();
        s.min_game_duration_secs = 3601;
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("min_game_duration_secs"));
    }

    #[test]
    fn test_min_game_duration_secs_zero_valid() {
        let mut s = EventFilterSettings::default();
        s.min_game_duration_secs = 0;
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_min_game_duration_secs_max_valid() {
        let mut s = EventFilterSettings::default();
        s.min_game_duration_secs = 3600;
        assert!(s.validate().is_ok());
    }

    // ---- Serialization round-trip with new fields ----

    #[test]
    fn test_event_filter_serialization_round_trip_with_new_fields() {
        let mut s = EventFilterSettings::default();
        s.min_game_duration_secs = 600;
        s.contest_window_secs = 15;
        let json = serde_json::to_string(&s).unwrap();
        let deserialized: EventFilterSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.min_game_duration_secs, 600);
        assert_eq!(deserialized.contest_window_secs, 15);
    }

    #[test]
    fn test_json_missing_new_fields_uses_defaults() {
        // Simulate old JSON without new serde(default) fields
        let json = r#"{
            "record_kills": true,
            "record_multikills": true,
            "record_first_blood": true,
            "record_deaths": false,
            "record_shutdown": false,
            "record_assists": false,
            "record_dragon": true,
            "record_baron": true,
            "record_elder": true,
            "record_herald": true,
            "record_turret": false,
            "record_inhibitor": true,
            "record_nexus": true,
            "record_ace": true,
            "record_game_end": true,
            "record_steal": true,
            "min_priority": 1
        }"#;
        let s: EventFilterSettings = serde_json::from_str(json).unwrap();
        // serde(default) fields should use their default functions
        assert_eq!(s.min_game_duration_secs, 300);
        assert_eq!(s.contest_window_secs, 10);
        assert!(s.record_voidgrubs);
        assert!(s.record_atakhan);
    }

    // ---- Default values ----

    #[test]
    fn test_storage_default_values() {
        let s = StorageSettings::default();
        assert!(!s.auto_delete_enabled);
        assert_eq!(s.auto_delete_days, 30);
        assert_eq!(s.max_storage_gb, 50);
        assert!(!s.delete_exported_clips);
    }

    #[test]
    fn test_recording_settings_validate_all_valid() {
        let s = RecordingSettings::default();
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_recording_settings_validate_propagates_video_error() {
        let mut s = RecordingSettings::default();
        s.video.bitrate_preset = BitratePreset::Custom(0);
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_recording_settings_validate_propagates_audio_error() {
        let mut s = RecordingSettings::default();
        s.audio.microphone_volume = 255;
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_recording_settings_validate_propagates_storage_error() {
        let mut s = RecordingSettings::default();
        s.storage.auto_delete_days = 0;
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_recording_settings_validate_propagates_event_filter_error() {
        let mut s = RecordingSettings::default();
        s.event_filter.contest_window_secs = 0;
        assert!(s.validate().is_err());
    }
}
