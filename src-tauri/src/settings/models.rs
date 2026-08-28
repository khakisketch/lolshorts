use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete recording settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSettings {
    /// Persisted settings schema. Missing values are treated as a pre-v4 file
    /// by the loader, then upgraded without relying on a sidecar marker.
    #[serde(default)]
    pub schema_version: u32,
    pub event_filter: EventFilterSettings,
    pub game_mode: GameModeSettings,
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub clip_timing: ClipTimingSettings,
    pub hotkeys: HotkeySettings,
    #[serde(default)]
    pub storage: StorageSettings,

    // General settings
    #[serde(default, alias = "auto_start_with_league")]
    pub launch_on_windows_startup: bool,
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
            schema_version: SETTINGS_SCHEMA_VERSION,
            event_filter: EventFilterSettings::default(),
            game_mode: GameModeSettings::default(),
            video: VideoSettings::default(),
            audio: AudioSettings::default(),
            clip_timing: ClipTimingSettings::default(),
            hotkeys: HotkeySettings::default(),
            storage: StorageSettings::default(),

            launch_on_windows_startup: false,
            minimize_to_tray: true,
            show_notifications: true,
            show_replay_popup: true,
            crash_reporting_enabled: false,
            // B4 fix: was `false` here while `#[serde(default = "default_true")]` on the
            // field above defaulted deserialized (missing-field) settings to `true` — a
            // fresh install (no settings.json yet, uses this Default impl) got the
            // overlay off while an existing user's settings.json missing this new field
            // got it on. docs/superpowers/specs/2026-03-13-commercial-release-design.md
            // §"Overlay Feature Flag" specifies `overlay_enabled: bool (default: true)`,
            // so align this impl to `true` to match serde's default and the spec.
            overlay_enabled: true,
        }
    }
}

/// The current on-disk shape of [`RecordingSettings`].
pub const SETTINGS_SCHEMA_VERSION: u32 = 4;

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

// PartialEq: `HighlightPreset::from_filters` compares a live toggle set against
// each preset's canonical set to decide which preset (if any) is selected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventFilterSettings {
    // 킬 관련
    //
    // 여기 있는 것들은 전부 `record_kills` 의 **하위 상황**이다. 감지는 킬 하나에
    // 가장 특별한 이름 하나만 붙이므로(`detect_trigger`), 하위를 끄면 그 킬은
    // 부모인 `record_kills` 로 내려가 판정된다(`EventTrigger::parent`).
    pub record_kills: bool,
    pub record_multikills: bool,
    pub record_first_blood: bool,
    /// 연속킬 중인 상대를 잡은 것 — **내가 딴** 킬이다.
    ///
    /// 예전에는 이 줄이 "데스 관련" 묶음에 있었고 기본값이 `false` 였다. 이름만
    /// 보고 "내가 셧다운 당한 것"으로 분류한 것인데, 감지 코드는 `killer_name ==
    /// 나` 일 때만 이 트리거를 만든다(`live_client.rs`). 그래서 기본 설정에서
    /// **가장 값진 킬 중 하나가 조용히 빠지고 있었다**.
    pub record_shutdown: bool,

    // 데스 관련
    pub record_deaths: bool,
    /// 내가 퍼블을 **당한** 것. 데스의 하위 상황이라 `record_deaths` 가 부모다.
    ///
    /// "죽는 장면은 됐고 퍼블 당한 것만" 이 성립하도록 따로 둔다 — 라인전에서
    /// 갱을 당한 순간은 복기 가치가 다른 데스와 다르다.
    #[serde(default)]
    pub record_first_blood_victim: bool,

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

/// 하이라이트 프리셋 — 개별 이벤트 토글 24개의 상위 개념.
///
/// 토글을 없애지 않는다. 기본 설정 화면에는 프리셋 하나만 보이고, 고급을 펼치면
/// 종전처럼 개별 토글이 나온다. 프리셋과 다른 조합을 만들면 `Custom`이 된다.
///
/// 이 계층이 필요한 이유는 실기기 테스트에서 드러났다: `record_deaths`/`record_assists`가
/// 기본 off라 KDA 4/4/13인 게임에서 어시스트 클립이 하나도 생기지 않았는데, 앱은 아무
/// 말도 하지 않았다. 24개 불리언은 "무엇을 담을지"라는 하나의 질문을 24조각으로 쪼개
/// 사용자에게 떠넘긴 것이고, 그 조각 중 하나만 틀려도 조용히 실패한다.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HighlightPreset {
    /// 많이 담기 — 죽는 장면까지 포함해 폭넓게.
    Everything,
    /// 균형 (기본) — 내 활약 위주. 킬·멀티킬·어시스트·주요 오브젝트.
    #[default]
    Balanced,
    /// 확실한 것만 — 멀티킬·스틸·역전 같은 장면만.
    BestOnly,
    /// 고급에서 개별 토글을 직접 조합한 상태.
    Custom,
}

impl HighlightPreset {
    /// 프리셋이 규정하는 이벤트 토글 조합. `Custom` 은 규정하지 않는다(None).
    pub fn to_filters(self) -> Option<EventFilterSettings> {
        let base = EventFilterSettings::default();
        Some(match self {
            Self::Custom => return None,
            Self::Everything => EventFilterSettings {
                record_deaths: true,
                record_first_blood_victim: true,
                record_assists: true,
                record_turret: true,
                ..base
            },
            // 기본값. `record_assists: true` 가 종전 기본과 다른 지점 —
            // 어시스트는 "내가 한 일"이고, 한타 위주인 칼바람에서는 킬보다 흔하다.
            // 죽는 장면은 기본에서 제외한다(원하면 '많이 담기').
            Self::Balanced => EventFilterSettings {
                record_assists: true,
                ..base
            },
            Self::BestOnly => EventFilterSettings {
                record_kills: false,
                record_assists: false,
                record_deaths: false,
                record_first_blood: false,
                record_dragon: false,
                record_herald: false,
                record_inhibitor: false,
                record_voidgrubs: false,
                record_ace: false,
                min_priority: 3,
                ..base
            },
        })
    }

    /// 현재 토글 조합에 해당하는 프리셋. 어느 것과도 맞지 않으면 `Custom`.
    pub fn from_filters(filters: &EventFilterSettings) -> Self {
        for preset in [Self::Balanced, Self::Everything, Self::BestOnly] {
            if preset.to_filters().as_ref() == Some(filters) {
                return preset;
            }
        }
        Self::Custom
    }
}

impl EventFilterSettings {
    /// 부모가 켜져 있는 하위 상황을 함께 켠다.
    ///
    /// # 왜 저장된 값을 고치나
    ///
    /// 설정 화면은 부모가 켜져 있는 동안 하위 스위치를 **감춘다** — 강등 덕분에
    /// 그 스위치가 결과를 바꾸지 못하기 때문이다(`EventTrigger::parent`). 그래서
    /// 예전 설정 파일에 남아 있는 "킬 켜짐 + 셧다운 꺼짐" 같은 조합은 사용자가
    /// 화면에서 되돌릴 수 없는 상태가 된다.
    ///
    /// 담기는 순간은 어느 쪽이든 같다(강등되어 킬로 남는다). 달라지는 것은
    /// **이름과 점수**다: 켜 두면 셧다운은 셧다운으로 저장되어 자동 편집에서
    /// 제 값(55점)을 받고, 꺼 두면 평범한 킬(25점)로 묻힌다. 사용자가 고를 수
    /// 없는 축이라면 손해 보지 않는 쪽으로 맞춰 두는 것이 맞다.
    ///
    /// 표는 `EventTrigger::parent()` 와 같은 계층이어야 한다 — 어긋나면
    /// `hierarchy_table_matches_trigger_parents` 가 깨진다.
    pub fn reconcile_hierarchy(&mut self) {
        if self.record_kills {
            self.record_multikills = true;
            self.record_shutdown = true;
            self.record_outplay = true;
            self.record_low_hp = true;
            // 퍼블은 별개 이벤트라 백엔드 부모가 없지만, 킬을 켜 두면 그 순간은
            // 어차피 담긴다. 화면도 같은 이유로 킬의 하위에 놓는다.
            self.record_first_blood = true;
        }
        if self.record_deaths {
            self.record_trade_kill = true;
            self.record_first_blood_victim = true;
        }
        if self.record_dragon {
            self.record_elder = true;
        }
        // 스틸은 드래곤에서도 바론에서도 나온다. 둘 중 하나라도 꺼져 있으면
        // "그래도 스틸은 담을까"가 사용자에게 열려 있는 질문이므로 건드리지 않는다.
        if self.record_dragon && self.record_baron {
            self.record_steal = true;
        }
    }
}

impl Default for EventFilterSettings {
    fn default() -> Self {
        Self {
            // 기본적으로 하이라이트만 녹화
            record_kills: true,
            record_multikills: true,
            record_first_blood: true,

            // 셧다운은 킬 계열이고 기본으로 담는다. `false` 이던 동안 기본 설정의
            // 사용자는 "킬을 담겠다"고 켜 둔 채로 연속킬 저지 장면을 잃었다.
            record_shutdown: true,

            record_deaths: false, // 데스는 기본적으로 OFF
            record_first_blood_victim: false,

            // `HighlightPreset::Balanced`(= `#[default]`) 와 같은 값이어야 한다.
            //
            // 이 한 줄이 `false` 이던 동안, 아무것도 건드리지 않은 새 설치의 필터
            // 조합은 **어떤 프리셋과도 일치하지 않았다**. 그래서 기본 설정 화면은
            // 첫 실행부터 "직접 설정" 배지를 달고 카드가 하나도 선택되지 않은 채로
            // 떴다 — 앱이 자기 기본값을 설명하지 못하는 상태였고, 실기기 렌더에서
            // 그대로 확인됐다. 프리셋 판정은 구조체 전체를 `PartialEq` 로 비교하므로
            // 필드 하나만 어긋나도 결과가 `Custom` 이 된다.
            //
            // 어시스트를 켜는 쪽으로 맞춘 것은 `Balanced` 의 정의가 그렇기 때문이다
            // (실기기 테스트에서 어시스트 장면이 하이라이트 후보로 유효했다).
            record_assists: true,

            record_dragon: true,
            record_baron: true,
            record_elder: true,
            record_herald: true,

            // 구조물은 쇼츠 소재가 아니다. 포탑은 한 판에 열 개 넘게 부서지고,
            // 억제기도 "이기고 있다"는 사실을 알릴 뿐 볼거리가 아니다(점수 30 —
            // 에이스 68·셧다운 55 와 비교하면 자동 편집이 고를 일이 거의 없다).
            // 담고 싶은 사람은 고급 설정에서 켠다.
            record_turret: false,
            record_inhibitor: false,
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
            // H.264, not H.265: the editor preview plays clips through the webview's
            // <video> element, and WebView2 only decodes HEVC when the OS has the
            // (separately installed) HEVC extensions. With H.265 the preview is a
            // black frame and nothing tells the user why. Compatibility beats the
            // file-size win for a clip you are going to upload anyway.
            codec: VideoCodec::H264,
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
    /// Populated from the list returned by `enumerate_system_audio_devices()`.
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
            // Microphone capture IS wired (cpal input -> mic_capture.wav ->
            // save_clip amix; see segment_recorder), but stays opt-in by
            // default: voice capture is privacy-sensitive, so the user must
            // explicitly enable it in the audio settings.
            record_microphone: false,
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
            audio_device_id: self.audio_device_id.clone(),
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
    pub merge_time_threshold: f64, // 10초 기본
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTiming {
    pub pre_duration: u32,
    pub post_duration: u32,
}

impl Default for ClipTimingSettings {
    fn default() -> Self {
        let mut event_timings = HashMap::new();

        event_timings.insert(
            "kill".to_string(),
            EventTiming {
                pre_duration: 8,
                post_duration: 5,
            },
        );
        for event_type in ["death", "assist", "turret"] {
            event_timings.insert(
                event_type.to_string(),
                EventTiming {
                    pre_duration: 6,
                    post_duration: 4,
                },
            );
        }
        for event_type in ["multikill", "outplay"] {
            event_timings.insert(
                event_type.to_string(),
                EventTiming {
                    pre_duration: 12,
                    post_duration: 8,
                },
            );
        }
        for event_type in ["dragon", "baron", "herald", "objective"] {
            event_timings.insert(
                event_type.to_string(),
                EventTiming {
                    pre_duration: 10,
                    post_duration: 6,
                },
            );
        }
        event_timings.insert(
            "steal".to_string(),
            EventTiming {
                pre_duration: 15,
                post_duration: 10,
            },
        );
        event_timings.insert(
            "ace".to_string(),
            EventTiming {
                pre_duration: 10,
                post_duration: 10,
            },
        );
        event_timings.insert(
            "game_end".to_string(),
            EventTiming {
                pre_duration: 12,
                post_duration: 3,
            },
        );

        Self {
            default_pre_duration: 8,
            default_post_duration: 5,
            event_timings,
            merge_consecutive_events: true,
            merge_time_threshold: 10.0,
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

    /// The exact timing profile written by releases before schema v4.
    ///
    /// A strict comparison is deliberate: a user changing even one duration,
    /// adding an event override, or changing the merge threshold keeps their
    /// chosen profile intact during the v4 upgrade.
    pub fn is_legacy_default_profile(&self) -> bool {
        self.default_pre_duration == 10
            && self.default_post_duration == 3
            && self.merge_consecutive_events
            && self.merge_time_threshold == 15.0
            && self.event_timings.len() == 3
            && matches!(self.event_timings.get("kill"), Some(timing) if timing.pre_duration == 10 && timing.post_duration == 3)
            && matches!(self.event_timings.get("multikill"), Some(timing) if timing.pre_duration == 15 && timing.post_duration == 5)
            && matches!(self.event_timings.get("steal"), Some(timing) if timing.pre_duration == 20 && timing.post_duration == 5)
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
        // H.264 by default so the editor preview can actually decode the clip;
        // see `highlight_preset_tests::default_codec_is_h264_...`.
        assert!(matches!(settings.video.codec, VideoCodec::H264));

        // Audio defaults: microphone capture defaults to off (it isn't
        // wired into the actual recording pipeline yet -- see
        // `AudioSettings::default()`'s doc comment).
        assert!(!settings.audio.record_microphone);
        assert_eq!(settings.audio.microphone_volume, 120);
        assert_eq!(settings.audio.system_audio_volume, 100);

        // Clip timing defaults
        assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(settings.clip_timing.default_pre_duration, 8);
        assert_eq!(settings.clip_timing.default_post_duration, 5);
        assert!(settings.clip_timing.merge_consecutive_events);
        assert_eq!(settings.clip_timing.merge_time_threshold, 10.0);

        // Hotkey defaults
        assert_eq!(settings.hotkeys.manual_save_clip, "F8");
        assert_eq!(settings.hotkeys.toggle_recording, "F9");
        assert_eq!(settings.hotkeys.delete_last_clip, "F10");
    }

    #[test]
    fn legacy_autostart_field_migrates_to_windows_startup_name() {
        let mut value = serde_json::to_value(RecordingSettings::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("launch_on_windows_startup");
        object.insert(
            "auto_start_with_league".to_string(),
            serde_json::json!(true),
        );

        let settings: RecordingSettings = serde_json::from_value(value).unwrap();
        assert!(settings.launch_on_windows_startup);

        let persisted = serde_json::to_value(settings).unwrap();
        assert_eq!(persisted["launch_on_windows_startup"], true);
        assert!(persisted.get("auto_start_with_league").is_none());
    }

    #[test]
    fn test_event_timing_lookup() {
        let settings = ClipTimingSettings::default();

        let multikill_timing = settings.get_timing_for_event("multikill");
        assert_eq!(multikill_timing.pre_duration, 12);
        assert_eq!(multikill_timing.post_duration, 8);

        let unknown_timing = settings.get_timing_for_event("unknown_event");
        assert_eq!(unknown_timing.pre_duration, 8); // fallback to default
        assert_eq!(unknown_timing.post_duration, 5);
    }

    #[test]
    fn fresh_clip_timing_uses_the_balanced_v4_profile() {
        let settings = ClipTimingSettings::default();

        for event_type in ["death", "assist", "turret"] {
            let timing = settings.get_timing_for_event(event_type);
            assert_eq!((timing.pre_duration, timing.post_duration), (6, 4));
        }
        for event_type in ["multikill", "outplay"] {
            let timing = settings.get_timing_for_event(event_type);
            assert_eq!((timing.pre_duration, timing.post_duration), (12, 8));
        }
        for event_type in ["dragon", "baron", "herald", "objective"] {
            let timing = settings.get_timing_for_event(event_type);
            assert_eq!((timing.pre_duration, timing.post_duration), (10, 6));
        }
        assert_eq!(
            (
                settings.get_timing_for_event("steal").pre_duration,
                settings.get_timing_for_event("steal").post_duration,
            ),
            (15, 10)
        );
        assert_eq!(
            (
                settings.get_timing_for_event("ace").pre_duration,
                settings.get_timing_for_event("ace").post_duration,
            ),
            (10, 10)
        );
        assert_eq!(
            (
                settings.get_timing_for_event("game_end").pre_duration,
                settings.get_timing_for_event("game_end").post_duration,
            ),
            (12, 3)
        );
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
        let s = VideoSettings {
            bitrate_preset: BitratePreset::Custom(5000),
            ..Default::default()
        };
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_video_settings_custom_bitrate_too_low() {
        let s = VideoSettings {
            bitrate_preset: BitratePreset::Custom(50),
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_video_settings_custom_bitrate_too_high() {
        let s = VideoSettings {
            bitrate_preset: BitratePreset::Custom(60000),
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_audio_volume_max_valid() {
        let s = AudioSettings {
            microphone_volume: 200,
            system_audio_volume: 200,
            ..Default::default()
        };
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_storage_settings_valid() {
        let s = StorageSettings {
            auto_delete_days: 30,
            max_storage_gb: 100,
            ..Default::default()
        };
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_storage_auto_delete_days_zero_invalid() {
        let s = StorageSettings {
            auto_delete_days: 0,
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_event_min_priority_out_of_range() {
        let s = EventFilterSettings {
            min_priority: 10,
            ..Default::default()
        };
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
        let mut s = VideoSettings {
            bitrate_preset: BitratePreset::Custom(100),
            ..Default::default()
        };
        assert!(s.validate().is_ok());
        s.bitrate_preset = BitratePreset::Custom(99);
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_video_settings_custom_bitrate_boundary_high() {
        let mut s = VideoSettings {
            bitrate_preset: BitratePreset::Custom(50_000),
            ..Default::default()
        };
        assert!(s.validate().is_ok());
        s.bitrate_preset = BitratePreset::Custom(50_001);
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_video_settings_get_resolution() {
        let mut s = VideoSettings {
            resolution: Resolution::R1920x1080,
            ..Default::default()
        };
        assert_eq!(s.get_resolution(), (1920, 1080));
        s.resolution = Resolution::R2560x1440;
        assert_eq!(s.get_resolution(), (2560, 1440));
        s.resolution = Resolution::R3840x2160;
        assert_eq!(s.get_resolution(), (3840, 2160));
    }

    #[test]
    fn test_video_settings_get_fps() {
        let mut s = VideoSettings {
            frame_rate: FrameRate::Fps30,
            ..Default::default()
        };
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

        let s2 = VideoSettings {
            bitrate_preset: BitratePreset::Low,
            ..Default::default()
        };
        assert_eq!(s2.get_bitrate(), 10_000_000);

        let s3 = VideoSettings {
            bitrate_preset: BitratePreset::Custom(1000),
            ..Default::default()
        };
        assert_eq!(s3.get_bitrate(), 1_000_000); // 1000 kbps = 1Mbps
    }

    #[test]
    fn test_video_is_h265() {
        let mut s = VideoSettings {
            codec: VideoCodec::H265,
            ..Default::default()
        };
        assert!(s.is_h265());
        s.codec = VideoCodec::H264;
        assert!(!s.is_h265());
    }

    // ---- AudioSettings validate() error paths ----

    #[test]
    fn test_audio_microphone_volume_too_high() {
        let s = AudioSettings {
            microphone_volume: 201,
            ..Default::default()
        };
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("Microphone volume"));
    }

    #[test]
    fn test_audio_system_volume_too_high() {
        let s = AudioSettings {
            system_audio_volume: 201,
            ..Default::default()
        };
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("System audio volume"));
    }

    #[test]
    fn test_audio_both_volumes_at_zero_valid() {
        let s = AudioSettings {
            microphone_volume: 0,
            system_audio_volume: 0,
            ..Default::default()
        };
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_to_audio_config_carries_audio_device_id() {
        // audio_device_id must reach recording::audio::AudioConfig so the
        // WASAPI capture backend can honor an explicit device selection
        // instead of always falling back to the OS default output device.
        let s = AudioSettings {
            audio_device_id: Some("Speakers (Realtek)".to_string()),
            system_audio_device: Some("Stereo Mix".to_string()),
            ..Default::default()
        };
        let config = s.to_audio_config();
        assert_eq!(
            config.audio_device_id,
            Some("Speakers (Realtek)".to_string())
        );
        // system_audio_device still passes through independently (used as
        // the DirectShow fallback / secondary WASAPI hint).
        assert_eq!(config.system_audio_device, Some("Stereo Mix".to_string()));
    }

    #[test]
    fn test_to_audio_config_audio_device_id_defaults_to_none() {
        let s = AudioSettings::default();
        assert_eq!(s.to_audio_config().audio_device_id, None);
    }

    // ---- StorageSettings validate() error paths ----

    #[test]
    fn test_storage_auto_delete_days_too_high() {
        let s = StorageSettings {
            auto_delete_days: 366,
            ..Default::default()
        };
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("auto_delete_days"));
    }

    #[test]
    fn test_storage_max_storage_gb_zero_invalid() {
        let s = StorageSettings {
            max_storage_gb: 0,
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_storage_max_storage_gb_too_high() {
        let s = StorageSettings {
            max_storage_gb: 10_001,
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_storage_boundary_values_valid() {
        let mut s = StorageSettings {
            auto_delete_days: 1,
            max_storage_gb: 1,
            ..Default::default()
        };
        assert!(s.validate().is_ok());
        s.auto_delete_days = 365;
        s.max_storage_gb = 10_000;
        assert!(s.validate().is_ok());
    }

    // ---- EventFilterSettings validate() error paths ----

    #[test]
    fn test_event_min_priority_zero_invalid() {
        let s = EventFilterSettings {
            min_priority: 0,
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_event_min_priority_boundary_valid() {
        let mut s = EventFilterSettings {
            min_priority: 1,
            ..Default::default()
        };
        assert!(s.validate().is_ok());
        s.min_priority = 5;
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_contest_window_secs_too_low() {
        let s = EventFilterSettings {
            contest_window_secs: 4,
            ..Default::default()
        };
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("contest_window_secs"));
    }

    #[test]
    fn test_contest_window_secs_too_high() {
        let s = EventFilterSettings {
            contest_window_secs: 31,
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn test_contest_window_secs_boundary_valid() {
        let mut s = EventFilterSettings {
            contest_window_secs: 5,
            ..Default::default()
        };
        assert!(s.validate().is_ok());
        s.contest_window_secs = 30;
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_min_game_duration_secs_too_high() {
        let s = EventFilterSettings {
            min_game_duration_secs: 3601,
            ..Default::default()
        };
        assert!(s.validate().is_err());
        let err = s.validate().unwrap_err();
        assert!(err.contains("min_game_duration_secs"));
    }

    #[test]
    fn test_min_game_duration_secs_zero_valid() {
        let s = EventFilterSettings {
            min_game_duration_secs: 0,
            ..Default::default()
        };
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_min_game_duration_secs_max_valid() {
        let s = EventFilterSettings {
            min_game_duration_secs: 3600,
            ..Default::default()
        };
        assert!(s.validate().is_ok());
    }

    // ---- Serialization round-trip with new fields ----

    #[test]
    fn test_event_filter_serialization_round_trip_with_new_fields() {
        let s = EventFilterSettings {
            min_game_duration_secs: 600,
            contest_window_secs: 15,
            ..Default::default()
        };
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

#[cfg(test)]
mod highlight_preset_tests {
    use super::{EventFilterSettings, HighlightPreset, VideoCodec, VideoSettings};

    #[test]
    fn default_preset_is_balanced() {
        assert_eq!(HighlightPreset::default(), HighlightPreset::Balanced);
    }

    #[test]
    fn balanced_records_assists() {
        // The field test lost every one of 13 assists because this was off and
        // nothing said so. Balanced is the default, so this is the guarantee that
        // an untouched install captures the player's own plays.
        let f = HighlightPreset::Balanced.to_filters().unwrap();
        assert!(f.record_assists, "assists must be on in the default preset");
        assert!(f.record_kills);
        assert!(f.record_multikills);
    }

    #[test]
    fn balanced_leaves_deaths_out_but_everything_includes_them() {
        assert!(
            !HighlightPreset::Balanced
                .to_filters()
                .unwrap()
                .record_deaths
        );
        assert!(
            HighlightPreset::Everything
                .to_filters()
                .unwrap()
                .record_deaths
        );
    }

    #[test]
    fn best_only_raises_the_priority_floor() {
        let f = HighlightPreset::BestOnly.to_filters().unwrap();
        assert!(f.min_priority >= 3);
        assert!(!f.record_kills, "a plain kill is not a 'best' moment");
        assert!(f.record_multikills);
    }

    #[test]
    fn custom_defines_no_filter_set() {
        assert!(HighlightPreset::Custom.to_filters().is_none());
    }

    #[test]
    fn every_preset_round_trips_through_from_filters() {
        for preset in [
            HighlightPreset::Everything,
            HighlightPreset::Balanced,
            HighlightPreset::BestOnly,
        ] {
            let filters = preset.to_filters().expect("preset defines filters");
            assert_eq!(
                HighlightPreset::from_filters(&filters),
                preset,
                "{preset:?} must be recognised from its own filter set"
            );
        }
    }

    #[test]
    fn hand_edited_toggles_read_back_as_custom() {
        let mut f = HighlightPreset::Balanced.to_filters().unwrap();
        f.record_turret = !f.record_turret;
        assert_eq!(HighlightPreset::from_filters(&f), HighlightPreset::Custom);
    }

    #[test]
    fn default_codec_is_h264_so_the_editor_preview_can_play_it() {
        // H.265 clips render as a black frame in the WebView2 <video> element
        // unless the OS has HEVC extensions installed, with no error surfaced.
        assert_eq!(VideoSettings::default().codec, VideoCodec::H264);
    }

    #[test]
    fn presets_are_distinct_from_each_other() {
        let a = HighlightPreset::Everything.to_filters().unwrap();
        let b = HighlightPreset::Balanced.to_filters().unwrap();
        let c = HighlightPreset::BestOnly.to_filters().unwrap();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn stock_defaults_are_exactly_the_default_preset() {
        // A fresh install must be describable by the preset the settings screen
        // marks as default. While `record_assists` differed, the stock filter set
        // matched NO preset, so `from_filters` returned `Custom` and the basic
        // settings screen opened with a "직접 설정" badge and no card selected --
        // the app could not describe its own out-of-the-box state. Confirmed by
        // rendering the real screen, not by reading code.
        let stock = EventFilterSettings::default();
        let default_preset = HighlightPreset::default().to_filters().unwrap();
        assert_eq!(stock, default_preset);
        assert_eq!(
            HighlightPreset::from_filters(&stock),
            HighlightPreset::default(),
            "새 설치의 필터 조합은 기본 프리셋으로 되읽혀야 한다"
        );
    }

    #[test]
    fn default_preset_records_assists() {
        // The field test that motivated `Balanced` found assist plays to be
        // usable highlight material; the stock default follows it.
        assert!(EventFilterSettings::default().record_assists);
    }
}
