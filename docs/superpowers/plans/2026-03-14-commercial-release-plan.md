# LoLShorts Commercial Release Implementation Plan

> **Archived / superseded plan:** This historical implementation plan is superseded by the current non-payment commercial readiness plan. Do not treat its payment, production-readiness, TikTok upload, or Instagram upload tasks as current implementation scope. Current scope keeps TikTok/Instagram to preset/export guidance only, keeps payment/Toss deferred, and requires E5 Field QA evidence before commercial or production-readiness claims.

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Historical goal only: implement all 24 improvements across 5 domains to bring LoLShorts toward commercial release quality. This is superseded and Field-QA-gated, not a current readiness claim.

**Architecture:** Tauri 2.0 desktop app (React 18 + TypeScript frontend, Rust backend, FFmpeg for video processing). Changes span recording pipeline, infrastructure, social platforms, video processing, and in-game overlay.

**Tech Stack:** Rust, TypeScript/React, FFmpeg, cpal (WASAPI), Sentry, Tauri plugins (updater, autostart, clipboard-manager, shell)

**Spec:** `docs/superpowers/specs/2026-03-13-commercial-release-design.md`

---

## File Structure

### New files to create
```
src-tauri/src/recording/wasapi_audio.rs        — WASAPI loopback capture via cpal
src-tauri/src/social/mod.rs                    — Social platform module root
src-tauri/src/social/tiktok/mod.rs             — TikTok module, archived obsolete direct-upload scope
src-tauri/src/social/tiktok/auth.rs            — TikTok OAuth 2.0 PKCE, archived obsolete direct-upload scope
src-tauri/src/social/tiktok/upload.rs          — TikTok video upload, archived obsolete direct-upload scope
src-tauri/src/social/tiktok/commands.rs        — TikTok Tauri commands, archived obsolete direct-upload scope
src-tauri/src/social/instagram/mod.rs          — Instagram module, archived obsolete direct-upload scope
src-tauri/src/social/instagram/auth.rs         — Instagram/Facebook OAuth, archived obsolete direct-upload scope
src-tauri/src/social/instagram/upload.rs       — Instagram Reels upload, archived obsolete direct-upload scope
src-tauri/src/social/instagram/commands.rs     — Instagram Tauri commands, archived obsolete direct-upload scope
src-tauri/src/overlay/mod.rs                   — Overlay window management
src-tauri/src/overlay/click_through.rs         — WS_EX_TRANSPARENT setup
src/pages/Overlay.tsx                          — Overlay React page
src/components/overlay/RecordingIndicator.tsx   — Red dot + timer
src/components/overlay/ClipSavedToast.tsx       — "클립 저장됨!" animation
src/components/overlay/EventFeed.tsx            — Recent events display
src/components/social/TikTokAuth.tsx            — TikTok auth UI, archived obsolete direct-upload scope
src/components/social/TikTokUpload.tsx          — TikTok upload UI, archived obsolete direct-upload scope
src/components/social/InstagramAuth.tsx         — Instagram auth UI, archived obsolete direct-upload scope
src/components/social/InstagramUpload.tsx       — Instagram upload UI, archived obsolete direct-upload scope
src/components/editor/EffectsPanel.tsx          — Speed/color/text effects UI
src/api/tiktok.ts                              — TikTok API calls, archived obsolete direct-upload scope
src/api/instagram.ts                           — Instagram API calls, archived obsolete direct-upload scope
src/types/tiktok.ts                            — TikTok types, archived obsolete direct-upload scope
src/types/instagram.ts                         — Instagram types, archived obsolete direct-upload scope
```

### Existing files to modify
```
src-tauri/Cargo.toml                           — Add cpal, tauri-plugin-updater, tauri-plugin-autostart, sentry, chrono-tz
src-tauri/tauri.conf.json                      — Updater key, overlay window, CSP domains
src-tauri/src/main.rs                          — Plugin registration, new commands, overlay setup
src-tauri/src/recording/audio.rs               — WASAPI integration
src-tauri/src/recording/live_client.rs         — New EventTrigger variants + detection
src-tauri/src/recording/auto_clip_manager.rs   — New event match arms, game mode filtering
src-tauri/src/recording/game_monitor.rs        — Pass game mode/queue to clip manager, overlay show/hide
src-tauri/src/recording/integration_backend/segment_recorder.rs — FFmpeg health monitoring, segment sorting, audio input
src-tauri/src/settings/models.rs               — New settings fields (record_voidgrubs, record_atakhan, crash_reporting, overlay)
src-tauri/src/video/processor/pipeline.rs      — Hardware encoder (3 locations)
src-tauri/src/video/processor/effects.rs       — Hardware encoder (4 locations)
src-tauri/src/video/auto_composer/processing.rs — Hardware encoder (2 locations)
src-tauri/src/video/commands.rs                — Effects commands, GIF export
src-tauri/src/youtube/commands.rs              — Resumable upload, quota tracking, scheduler
src-tauri/src/hotkey.rs                        — Dynamic hotkey registration
src-tauri/src/tray.rs                          — Cleanup on quit
src-tauri/src/utils/cleanup.rs                 — (no change, already implemented)
src/App.tsx                                    — Sentry init, overlay route
src/pages/Settings.tsx                         — Error handling with toasts
src/pages/Dashboard.tsx                        — Toast notifications
src/pages/Editor.tsx                           — Toast notifications
src/pages/YouTube.tsx                          — Toast notifications, social tabs
src/components/settings/GeneralSettings.tsx    — Crash reporting toggle, overlay toggle
src/components/settings/EventFilterSettings.tsx — Voidgrubs, Atakhan toggles
src/components/editor/ExportModal.tsx          — Format selector, GIF, local export
src/components/editor/TimelineClip.tsx         — Right-click effects menu
src/stores/recordingStore.ts                   — New settings defaults
src/types/index.ts                             — New settings types
src/locales/*/translation.json (20 files)      — New i18n keys
package.json                                   — Add @sentry/react
```

---

## Chunk 1: Recording Pipeline (Tasks 1-6)

### Task 1: Add new EventTrigger variants and detection

**Files:**
- Modify: `src-tauri/src/recording/live_client.rs`
- Modify: `src-tauri/src/settings/models.rs`

- [ ] **Step 1: Add `record_voidgrubs` and `record_atakhan` to EventFilterSettings**

In `src-tauri/src/settings/models.rs`, add after line 106 (`record_steal`):

```rust
    #[serde(default = "default_true")]
    pub record_voidgrubs: bool,
    #[serde(default = "default_true")]
    pub record_atakhan: bool,
```

> **Note:** If `fn default_true() -> bool { true }` does not already exist in this file, define it alongside the other serde default helpers.

And in the `Default` impl after `record_steal: true,`:

```rust
    record_voidgrubs: true,
    record_atakhan: true,
```

- [ ] **Step 2: Add new EventTrigger variants**

In `src-tauri/src/recording/live_client.rs`, add to `EventTrigger` enum after `GameEnd`:

```rust
    ElderDragonKill,  // Elder Dragon specifically
    VoidgrubsKill,    // Voidgrubs/Horde
    AtakhanKill,      // Atakhan (Season 15)
    Shutdown,         // Ending a 3+ kill streak
```

- [ ] **Step 3: Add priority/duration values for new variants**

In `priority()` add before `_ => 1`:
```rust
    EventTrigger::ElderDragonKill => 4,
    EventTrigger::VoidgrubsKill => 2,
    EventTrigger::AtakhanKill => 3,
    EventTrigger::Shutdown => 3,
```

In `pre_duration()` add before `_ => 10`:
```rust
    EventTrigger::ElderDragonKill => 15,
    EventTrigger::AtakhanKill => 15,
    EventTrigger::Shutdown => 10,
    EventTrigger::VoidgrubsKill => 10,
```

In `post_duration()` add before `_ => 3`:
```rust
    EventTrigger::ElderDragonKill => 5,
    EventTrigger::AtakhanKill => 5,
    EventTrigger::Shutdown => 5,
    EventTrigger::VoidgrubsKill => 3,
```

- [ ] **Step 4: Add detection logic in `detect_trigger`**

In `detect_trigger()`, add before the `"Ace"` arm:

```rust
    "HordeKill" => {
        if event.killer_name.as_deref() == Some(player_name) {
            Some(EventTrigger::VoidgrubsKill)
        } else {
            None
        }
    }
    "AtakhanKill" => {
        if event.killer_name.as_deref() == Some(player_name) {
            Some(EventTrigger::AtakhanKill)
        } else {
            None
        }
    }
```

First, add a `dragon_type` field to the `GameEvent` struct (Live Client API provides this as `"DragonType"`):
```rust
    #[serde(rename = "DragonType", default)]
    pub dragon_type: Option<String>,
```

Then modify the existing `"DragonKill"` arm to check for Elder via `dragon_type`:
```rust
    "DragonKill" => {
        if event.killer_name.as_deref() == Some(player_name) {
            // Check if it's an Elder Dragon via the DragonType field
            if event.dragon_type.as_deref().map(|t| t.contains("Elder")).unwrap_or(false) {
                Some(EventTrigger::ElderDragonKill)
            } else {
                Some(EventTrigger::DragonKill)
            }
        } else {
            None
        }
    }
```

> **Important:** The Live Client API always returns `"DragonKill"` as the `event_name` for all dragon kills. The dragon type (Infernal, Mountain, Elder, etc.) is in the separate `"DragonType"` field, NOT in `event_name`.

Add Shutdown detection: add a `kill_streak_tracker: Arc<tokio::sync::Mutex<HashMap<String, u32>>>` field to `LiveClientMonitor`. In ChampionKill handler, increment killer's streak count. In Death handler, if victim's streak >= 3, emit `EventTrigger::Shutdown`. Reset victim's streak to 0.

- [ ] **Step 5: Run `cargo check` AFTER completing Task 2**

> **DEPENDENCY:** `should_record_event` and `trigger_to_event_type` in `auto_clip_manager.rs` use exhaustive matches (no wildcard `_` arm). Adding new `EventTrigger` variants in this task WITHOUT adding corresponding match arms in Task 2 will cause a compilation error. You MUST complete Task 2 Steps 1-2 before running `cargo check`.

Run: `cd src-tauri && cargo check` (only after Task 2 Steps 1-2 are done)
Expected: Compilation success

- [ ] **Step 6: Commit (combined with Task 2)**

```bash
git add src-tauri/src/recording/live_client.rs src-tauri/src/settings/models.rs src-tauri/src/recording/auto_clip_manager.rs
git commit -m "feat: add Elder Dragon, Voidgrubs, Atakhan, Shutdown event detection and clip manager support"
```

### Task 2: Update auto_clip_manager for new events + game mode filtering

**Files:**
- Modify: `src-tauri/src/recording/auto_clip_manager.rs`
- Modify: `src-tauri/src/recording/game_monitor.rs`

- [ ] **Step 1: Add new match arms to `should_record_event`**

In `auto_clip_manager.rs`, in `should_record_event` match block, add before the closing `};`:

```rust
    EventTrigger::ElderDragonKill => settings.event_filter.record_elder,
    EventTrigger::VoidgrubsKill => settings.event_filter.record_voidgrubs,
    EventTrigger::AtakhanKill => settings.event_filter.record_atakhan,
    EventTrigger::Shutdown => settings.event_filter.record_shutdown,
```

- [ ] **Step 2: Add new match arms to `trigger_to_event_type`**

```rust
    EventTrigger::ElderDragonKill => EventType::Custom("ElderDragonKill".to_string()),
    EventTrigger::VoidgrubsKill => EventType::Custom("VoidgrubsKill".to_string()),
    EventTrigger::AtakhanKill => EventType::Custom("AtakhanKill".to_string()),
    EventTrigger::Shutdown => EventType::Custom("Shutdown".to_string()),
```

- [ ] **Step 3: Add game mode field to AutoClipManager**

Add fields to `AutoClipManager` struct:
```rust
    current_game_mode: Arc<TokioRwLock<String>>,
    current_queue_id: Arc<TokioRwLock<Option<u32>>>,
```

Initialize in `new()`:
```rust
    current_game_mode: Arc::new(TokioRwLock::new(String::new())),
    current_queue_id: Arc::new(TokioRwLock::new(None)),
```

Add setter method:
```rust
    pub async fn set_game_mode(&self, mode: String, queue_id: Option<u32>) {
        *self.current_game_mode.write().await = mode;
        *self.current_queue_id.write().await = queue_id;
    }
```

- [ ] **Step 4: Add game mode check to `should_record_event`**

At the start of `should_record_event`, before priority check:

```rust
    // Check game mode filter
    let game_mode = self.current_game_mode.read().await;
    let queue_id = self.current_queue_id.read().await;
    if !game_mode.is_empty() {
        let mode_settings = &settings.game_mode;
        let mode_allowed = match game_mode.as_str() {
            "CLASSIC" => match *queue_id {
                Some(420) => mode_settings.record_ranked_solo,  // RANKED_SOLO_5x5
                Some(440) => mode_settings.record_ranked_flex,  // RANKED_FLEX_SR
                Some(430) => mode_settings.record_normal,       // NORMAL_BLIND
                Some(400) => mode_settings.record_normal,       // NORMAL_DRAFT
                Some(490) => mode_settings.record_quick_play,   // QUICKPLAY
                _ => true, // Unknown queue, allow
            },
            "ARAM" => mode_settings.record_aram,
            "URF" | "ARURF" => mode_settings.record_special,
            _ => true, // Unknown mode, allow
        };
        if !mode_allowed {
            return Ok(false);
        }
    }
    drop(game_mode);
    drop(queue_id);
```

- [ ] **Step 5: Call `set_game_mode` from game_monitor**

In `game_monitor.rs`, where game start is detected, add:
```rust
    clip_manager.set_game_mode(game_mode_str, queue_id).await;
```

- [ ] **Step 6: Run `cargo check` and `cargo test`**

Run: `cd src-tauri && cargo check && cargo test`
Expected: Compilation success, all tests pass

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/recording/auto_clip_manager.rs src-tauri/src/recording/game_monitor.rs
git commit -m "feat: add game mode filtering and new event types to clip manager"
```

### Task 3: Steal detection

**Files:**
- Modify: `src-tauri/src/recording/live_client.rs`

- [ ] **Step 1: Add recent objective kills tracking**

Add field to `LiveClientMonitor`:
```rust
    recent_objective_fights: Arc<tokio::sync::Mutex<Vec<ObjectiveFight>>>,
```

Add struct:
```rust
#[derive(Debug, Clone)]
struct ObjectiveFight {
    timestamp: SystemTime,
    teams_involved: Vec<String>, // Team IDs with kills near the event
}
```

- [ ] **Step 2: Implement steal detection in `detect_trigger`**

In the DragonKill/BaronKill handlers, determine the killer's team and compare with the player's team. Only emit `Steal` if the objective was taken by the player's team AND the enemy team was contesting (had kills near the objective within 10s):

```rust
    // Get killer team from all_players list
    let killer_team = all_players.iter()
        .find(|p| p.summoner_name == event.killer_name.as_deref().unwrap_or_default())
        .map(|p| p.team.as_str());
    let player_team = all_players.iter()
        .find(|p| p.summoner_name == player_name)
        .map(|p| p.team.as_str());

    let same_team = killer_team.is_some() && killer_team == player_team;
    if !same_team {
        return None; // Enemy team took the objective, not our steal
    }

    // Check if contested (enemy team had kills near objective within 10s)
    let fights = self.recent_objective_fights.lock().await;
    let now = SystemTime::now();
    let enemy_contested = fights.iter().any(|f| {
        now.duration_since(f.timestamp)
            .unwrap_or(Duration::from_secs(100))
            < Duration::from_secs(10)
            && f.teams_involved.iter().any(|t| Some(t.as_str()) != player_team)
    });
    if enemy_contested {
        Some(EventTrigger::Steal)
    } else {
        // return normal DragonKill/BaronKill
    }
```

- [ ] **Step 3: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: Compilation success

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/recording/live_client.rs
git commit -m "feat: add contested objective steal detection heuristic"
```

### Task 4: FFmpeg process health monitoring

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Take stderr handle before storing child process**

After the FFmpeg process spawn (around line 169), before `self.ffmpeg_process = Some(child)`:

```rust
    let stderr = child.stderr.take();
    self.ffmpeg_process = Some(child);

    // Spawn stderr consumer task (reads until EOF when FFmpeg exits)
    if let Some(stderr) = stderr {
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.contains("error") || line.contains("Error") {
                    tracing::error!("FFmpeg: {}", line);
                } else {
                    tracing::debug!("FFmpeg: {}", line);
                }
            }
            tracing::info!("FFmpeg stderr monitor exited");
        });
    }
```

- [ ] **Step 2: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: Compilation success

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/recording/integration_backend/segment_recorder.rs
git commit -m "fix: consume FFmpeg stderr to prevent pipe deadlock and monitor health"
```

### Task 5: Segment sorting fix

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Replace mtime sort with filename-based sort**

Find the segment sorting code (around line 288-296) and replace:

```rust
    segments.sort_by_key(|path| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("segment_"))
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0)
    });
```

- [ ] **Step 2: Run `cargo check` and `cargo test`**

Run: `cd src-tauri && cargo check && cargo test`
Expected: All pass

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/recording/integration_backend/segment_recorder.rs
git commit -m "fix: sort segments by filename number instead of mtime"
```

### Task 6: WASAPI loopback audio capture

**Files:**
- Create: `src-tauri/src/recording/wasapi_audio.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/recording/mod.rs`
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Add cpal dependency**

In `src-tauri/Cargo.toml`, add under `[dependencies]`:
```toml
cpal = "0.15"
```

- [ ] **Step 2: Create wasapi_audio.rs**

```rust
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

pub struct WasapiCapture {
    stream: Option<cpal::Stream>,
    output_path: PathBuf,
    is_capturing: Arc<RwLock<bool>>,
}

impl WasapiCapture {
    pub fn new(output_dir: &std::path::Path) -> Result<Self> {
        let output_path = output_dir.join("system_audio.wav");
        Ok(Self {
            stream: None,
            output_path,
            is_capturing: Arc::new(RwLock::new(false)),
        })
    }

    pub fn start(&mut self) -> Result<()> {
        let host = cpal::host_from_id(cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Wasapi)
            .expect("WASAPI host not available"))
            .expect("Failed to initialize WASAPI host");

        // Get default output device for loopback
        let device = host.default_output_device()
            .context("No default output audio device found")?;

        info!("WASAPI loopback device: {}", device.name().unwrap_or_default());

        let config = device.default_output_config()
            .context("Failed to get default output config")?;

        let spec = hound::WavSpec {
            channels: config.channels(),
            sample_rate: config.sample_rate().0,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let writer = Arc::new(std::sync::Mutex::new(Some(
            hound::WavWriter::create(&self.output_path, spec)
                .context("Failed to create WAV writer")?
        )));

        let writer_clone = writer.clone();
        let err_fn = |err| error!("WASAPI audio capture error: {}", err);

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut guard) = writer_clone.lock() {
                    if let Some(ref mut w) = *guard {
                        for &sample in data {
                            let s = (sample * i16::MAX as f32) as i16;
                            let _ = w.write_sample(s);
                        }
                    }
                }
            },
            err_fn,
            None,
        ).context("Failed to build WASAPI loopback stream")?;

        stream.play().context("Failed to start WASAPI stream")?;
        self.stream = Some(stream);
        info!("WASAPI loopback capture started: {:?}", self.output_path);
        Ok(())
    }

    pub fn stop(&mut self) -> Option<PathBuf> {
        if let Some(stream) = self.stream.take() {
            drop(stream);
            info!("WASAPI loopback capture stopped");
            Some(self.output_path.clone())
        } else {
            None
        }
    }

    pub fn output_path(&self) -> &PathBuf {
        &self.output_path
    }
}
```

- [ ] **Step 3: Add `hound` crate for WAV writing**

In `src-tauri/Cargo.toml`:
```toml
hound = "3.5"
```

- [ ] **Step 4: Register module in recording/mod.rs**

Add: `pub mod wasapi_audio;`

- [ ] **Step 5: Integrate into segment_recorder**

In `segment_recorder.rs`, add a `wasapi: Option<WasapiCapture>` field. In `start()`:

```rust
    // Try WASAPI loopback first
    let mut wasapi = WasapiCapture::new(&self.output_dir).ok();
    if let Some(ref mut w) = wasapi {
        if let Err(e) = w.start() {
            warn!("WASAPI loopback failed, falling back to DirectShow: {}", e);
            wasapi = None;
        }
    }
    self.wasapi = wasapi;
```

In clip extraction, if `self.wasapi` has an `output_path`, add `-i <wav_path>` to FFmpeg args.

- [ ] **Step 6: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: Compilation success

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/recording/wasapi_audio.rs src-tauri/src/recording/mod.rs src-tauri/src/recording/integration_backend/segment_recorder.rs
git commit -m "feat: add WASAPI loopback audio capture via cpal"
```

---

## Chunk 2: Infrastructure and Stability (Tasks 7-12)

### Task 7: Auto-updater setup

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add tauri-plugin-updater dependency**

In `Cargo.toml`:
```toml
tauri-plugin-updater = "2"
```

- [ ] **Step 2: Generate updater keys**

Run: `cd src-tauri && cargo tauri signer generate -w ~/.tauri/lolshorts.key`

Save the public key for the next step.

- [ ] **Step 3: Replace placeholder pubkey in tauri.conf.json**

Replace the base64 placeholder at line 97 with the generated public key.

- [ ] **Step 4: Register plugin in main.rs**

Add before `.run()`:
```rust
    .plugin(tauri_plugin_updater::Builder::new().build())
```

- [ ] **Step 5: Add update check on startup**

In the `setup` closure in main.rs:
```rust
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        match tauri_plugin_updater::UpdaterExt::updater(&handle).check().await {
            Ok(Some(update)) => {
                tracing::info!("Update available: {}", update.version);
                // Emit event to frontend for user confirmation
                let _ = handle.emit("update-available", update.version);
            }
            Ok(None) => tracing::info!("App is up to date"),
            Err(e) => tracing::warn!("Update check failed: {}", e),
        }
    });
```

- [ ] **Step 6: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: Compilation success

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/src/main.rs
git commit -m "feat: add auto-updater with tauri-plugin-updater"
```

### Task 8: OS autostart

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add tauri-plugin-autostart dependency**

```toml
tauri-plugin-autostart = "2"
```

- [ ] **Step 2: Register plugin in main.rs**

```rust
    .plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent, None
    ))
```

- [ ] **Step 3: Add command to toggle autostart**

In main.rs, add a Tauri command:
```rust
#[tauri::command]
async fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

Register in `.invoke_handler()`.

- [ ] **Step 4: Run `cargo check`**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/main.rs
git commit -m "feat: wire auto_start_with_league to OS autostart"
```

### Task 9: Sentry crash reporting

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/main.rs`
- Modify: `package.json`
- Modify: `src/App.tsx`
- Modify: `src/components/settings/GeneralSettings.tsx`
- Modify: `src-tauri/src/settings/models.rs`

- [ ] **Step 1: Add Sentry Rust crate**

```toml
sentry = "0.34"
```

- [ ] **Step 2: Add crash_reporting_enabled to settings**

In `settings/models.rs`, add to the general settings struct:
```rust
    #[serde(default)]
    pub crash_reporting_enabled: bool,
```

- [ ] **Step 3: Init Sentry in main.rs**

At the very start of `main()` (use `_sentry_guard` to avoid shadowing the tracing `_guard`):
```rust
    let _sentry_guard = sentry::init(("https://YOUR_DSN@sentry.io/PROJECT", sentry::ClientOptions {
        release: sentry::release_name!(),
        auto_session_tracking: true,
        ..Default::default()
    }));
```

- [ ] **Step 4: Install @sentry/react**

Run: `npm install @sentry/react`

- [ ] **Step 5: Init Sentry in frontend**

In `src/main.tsx` before `ReactDOM.createRoot`:
```typescript
import * as Sentry from '@sentry/react';
Sentry.init({
    dsn: "https://YOUR_DSN@sentry.io/PROJECT",
    enabled: false, // Enabled via settings toggle
});
```

- [ ] **Step 6: Wire ErrorBoundary in App.tsx**

Replace ONLY the commented-out error tracking line, preserving the existing structure and adding a PROD guard:
```typescript
onError={(error, errorInfo) => {
    logger.error('App-level error caught:', error, errorInfo);
    if (import.meta.env.PROD) {
        Sentry.captureException(error, { extra: { componentStack: errorInfo.componentStack } });
    }
}}
```

- [ ] **Step 7: Add toggle in GeneralSettings**

Add a Switch for "크래시 리포트 전송" wired to `crash_reporting_enabled`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/main.rs src-tauri/src/settings/models.rs src/App.tsx src/main.tsx src/components/settings/GeneralSettings.tsx package.json
git commit -m "feat: integrate Sentry crash reporting (opt-in)"
```

### Task 10: Settings page error handling

**Files:**
- Modify: `src/pages/Settings.tsx`
- Modify: `src/locales/en/translation.json` (+ 19 other locales)

- [ ] **Step 1: Replace empty catch blocks**

> **Note:** This project uses `@/components/ui/use-toast` (radix/shadcn pattern), NOT `sonner`. Use the `useToast` hook with `toast({ title, variant })`.

In Settings.tsx, add import and hook:
```typescript
import { useToast } from '@/components/ui/use-toast';
// inside the component:
const { toast } = useToast();
```

Replace each `catch {}` with:
```typescript
catch (error) {
    toast({ title: t('settings.error.loadFailed'), variant: 'destructive' });
    console.error('Settings operation failed:', error);
}
```

Use appropriate keys: `loadFailed`, `saveFailed`, `resetFailed`, `licenseFailed`.

- [ ] **Step 2: Add i18n keys**

In `src/locales/en/translation.json`:
```json
"settings": {
    "error": {
        "loadFailed": "Failed to load settings",
        "saveFailed": "Failed to save settings",
        "resetFailed": "Failed to reset settings",
        "licenseFailed": "Failed to load license info"
    }
}
```

Add Korean translations and other locales.

- [ ] **Step 3: Run `npx tsc --noEmit`**

Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/pages/Settings.tsx src/locales/
git commit -m "fix: add toast error feedback to Settings page"
```

### Task 11: Toast notifications across all pages

**Files:**
- Modify: `src/pages/Dashboard.tsx`
- Modify: `src/pages/Editor.tsx`
- Modify: `src/pages/YouTube.tsx`
- Modify: `src/locales/*/translation.json`

- [ ] **Step 1: Add toast imports to each page**

> **Note:** This project uses `@/components/ui/use-toast` (radix/shadcn pattern), NOT `sonner`.

```typescript
import { useToast } from '@/components/ui/use-toast';
import { useTranslation } from 'react-i18next';
// inside the component:
const { toast } = useToast();
```

- [ ] **Step 2: Wrap all async operations with toast feedback**

Pattern for each operation:
```typescript
try {
    await invoke('start_recording');
    toast({ title: t('recording.started') });
} catch (error) {
    toast({ title: t('recording.startFailed'), variant: 'destructive' });
}
```

Apply to: recording start/stop, clip save, export, upload start/complete/fail, auto-edit.

- [ ] **Step 3: Add i18n keys for all operations**

Add to all 20 locale files: `recording.started`, `recording.stopped`, `recording.startFailed`, `clip.saved`, `clip.saveFailed`, `export.completed`, `export.failed`, `upload.started`, `upload.completed`, `upload.failed`, `autoEdit.completed`, `autoEdit.failed`.

- [ ] **Step 4: Run `npx tsc --noEmit`**

- [ ] **Step 5: Commit**

```bash
git add src/pages/ src/locales/
git commit -m "feat: add toast notifications to all user-facing operations"
```

### Task 12: Cleanup on shutdown

**Files:**
- Modify: `src-tauri/src/tray.rs`

- [ ] **Step 1: Add cleanup call in tray quit handler**

In `tray.rs` at line 33-35, change:
```rust
    "quit" => {
        info!("트레이 메뉴에서 앱 종료 요청");
        app.exit(0);
    }
```
To:
```rust
    "quit" => {
        info!("트레이 메뉴에서 앱 종료 요청");
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let state = handle.state::<crate::AppState>();
            if let Err(e) = state.cleanup_manager.cleanup_on_shutdown().await {
                tracing::error!("Shutdown cleanup failed: {}", e);
            }
            handle.exit(0);
        });
    }
```

- [ ] **Step 2: Run `cargo check`**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tray.rs
git commit -m "fix: call cleanup_on_shutdown when quitting from tray"
```

---

## Chunk 3: YouTube Enhancement + Social Platforms (Tasks 13-18)

### Task 13: Switch to resumable YouTube upload

**Files:**
- Modify: `src-tauri/src/youtube/commands.rs`

- [ ] **Step 1: Replace upload_video with upload_video_resumable**

At line 273 in `youtube_upload_video`, change:
```rust
    let video = upload_client.upload_video(&video_path, metadata, thumbnail_path.as_deref())
```
To (clone `metadata` before the match so the fallback branch can use it after the first branch moves the original):
```rust
    let metadata_clone = metadata.clone();
    let video = match youtube.upload_client.upload_video_resumable(&video_path, metadata, thumbnail_path.as_deref()).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Resumable upload failed, falling back to multipart: {}", e);
            youtube.upload_client.upload_video(&video_path, metadata_clone, thumbnail_path.as_deref()).await?
        }
    };
```

- [ ] **Step 2: Run `cargo check`**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/youtube/commands.rs
git commit -m "feat: switch YouTube upload to resumable with multipart fallback"
```

### Task 14: YouTube quota tracking

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/youtube/commands.rs`

- [ ] **Step 1: ~~Add chrono-tz dependency~~ SKIP**

> **Note:** `chrono-tz 0.8` is already present in `Cargo.toml` -- no action needed. Do not add a duplicate or bump version.

- [ ] **Step 2: Increment quota after successful upload**

After the successful upload return in `youtube_upload_video`, add. **Note:** All storage calls must go through `youtube.storage`, not bare `storage` (which is not in scope):
```rust
    // Increment quota (videos.insert costs 1600 units)
    use chrono::Utc;
    use chrono_tz::America::Los_Angeles;
    let today_pst = Utc::now().with_timezone(&Los_Angeles).format("%Y-%m-%d").to_string();
    let quota_key = youtube_quota_key(user_id, &today_pst);
    let current: u64 = youtube.storage.get_setting(&quota_key).await.ok().and_then(|s| s.parse().ok()).unwrap_or(0u64);
    let _ = youtube.storage.set_setting(&quota_key, &(current + 1600).to_string()).await;
```

- [ ] **Step 3: Update `youtube_get_quota_info` to use date-based keys**

```rust
    let today_pst = Utc::now().with_timezone(&Los_Angeles).format("%Y-%m-%d").to_string();
    let quota_key = youtube_quota_key(user_id, &today_pst);
    let used: u64 = youtube.storage.get_setting(&quota_key).await.ok().and_then(|s| s.parse().ok()).unwrap_or(0u64);
```

- [ ] **Step 4: Run `cargo check`**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/youtube/commands.rs
git commit -m "feat: track YouTube API quota with daily PST reset"
```

### Task 15: Scheduled upload background executor

**Files:**
- Modify: `src-tauri/src/youtube/commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Create background scheduler function**

In `youtube/commands.rs`:
```rust
pub async fn start_upload_scheduler(app_handle: AppHandle) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        // Read queue, find pending items where scheduled_time <= now
        // Execute uploads, update status, emit events
        // Max 1 concurrent upload
    }
}
```

- [ ] **Step 2: Spawn scheduler in main.rs `.setup()` closure**

> **Note:** The scheduler needs access to `YouTubeManager` state. Spawn inside the `.setup()` closure so that `app_handle.state::<YouTubeManager>()` is available.

```rust
    // Inside .setup(|app| { ... })
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let youtube = handle.state::<YouTubeManager>();
        youtube::commands::start_upload_scheduler(handle.clone()).await;
    });
```

- [ ] **Step 3: Run `cargo check`**

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/youtube/commands.rs src-tauri/src/main.rs
git commit -m "feat: add background scheduled upload executor"
```

### Task 16: TikTok upload integration, archived obsolete direct-upload scope

> **Not current scope:** This task is preserved as historical context only. The current plan does not revive TikTok direct upload; TikTok remains preset/export guidance only.

**Files:**
- Create: `src-tauri/src/social/mod.rs`, `src-tauri/src/social/tiktok/mod.rs`, `auth.rs`, `upload.rs`, `commands.rs` (archived obsolete direct-upload scope)
- Create: `src/components/social/TikTokAuth.tsx`, `TikTokUpload.tsx` (archived obsolete direct-upload scope)
- Create: `src/api/tiktok.ts`, `src/types/tiktok.ts` (archived obsolete direct-upload scope)
- Modify: `src-tauri/src/main.rs` — register commands
- Modify: `src-tauri/tauri.conf.json` — add CSP domains

- [ ] **Step 1: Create social module structure**

Create `src-tauri/src/social/mod.rs`:
```rust
pub mod tiktok;
// NOTE: `pub mod instagram;` is added later in Task 17 when instagram/ is created.
// Do NOT declare it here yet or cargo check will fail.
```

Create `src-tauri/src/social/tiktok/mod.rs`:
```rust
pub mod auth;
pub mod upload;
pub mod commands;
```

- [ ] **Step 1b: Register social module in lib.rs**

Add to `src-tauri/src/lib.rs`:
```rust
pub mod social;
```

> **Important:** Without this, the social module tree is unreachable and commands won't compile.

- [ ] **Step 2: Implement TikTok OAuth (auth.rs)** — archived obsolete direct-upload scope

OAuth 2.0 PKCE flow with local HTTP callback server. Pattern mirrors existing YouTube OAuth.

- [ ] **Step 3: Implement TikTok upload (upload.rs)** — archived obsolete direct-upload scope

POST to `https://open.tiktokapis.com/v2/post/publish/video/init/` for chunk upload.

- [ ] **Step 4: Create Tauri commands (commands.rs)**

```rust
#[tauri::command]
pub async fn tiktok_authenticate(app: AppHandle) -> Result<String, String> { ... }

#[tauri::command]
pub async fn tiktok_upload_video(path: String, title: String, ...) -> Result<String, String> { ... }
```

- [ ] **Step 5: Add CSP domains to tauri.conf.json**

Add `open.tiktokapis.com` and `www.tiktok.com` to the connect-src CSP.

- [ ] **Step 6: Create frontend components**

`TikTokAuth.tsx` — OAuth button and status display, archived obsolete direct-upload scope.
`TikTokUpload.tsx` — Upload form with title, description, privacy settings, archived obsolete direct-upload scope.
`src/api/tiktok.ts` — Tauri invoke wrappers.
`src/types/tiktok.ts` — TypeScript interfaces.

- [ ] **Step 7: Register commands in main.rs**

- [ ] **Step 8: Run `cargo check` and `npx tsc --noEmit`**

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/social/ src/components/social/ src/api/tiktok.ts src/types/tiktok.ts
git commit -m "feat: add TikTok upload integration" # archived obsolete direct-upload scope
```

### Task 17: Instagram Reels upload integration, archived obsolete direct-upload scope

> **Not current scope:** This task is preserved as historical context only. The current plan does not revive Instagram direct upload; Instagram remains preset/export guidance only.

**Files:**
- Create: `src-tauri/src/social/instagram/mod.rs`, `auth.rs`, `upload.rs`, `commands.rs` (archived obsolete direct-upload scope)
- Create: `src/components/social/InstagramAuth.tsx`, `InstagramUpload.tsx` (archived obsolete direct-upload scope)
- Create: `src/api/instagram.ts`, `src/types/instagram.ts` (archived obsolete direct-upload scope)

- [ ] **Step 0: Add instagram module to social/mod.rs**

In `src-tauri/src/social/mod.rs`, add:
```rust
pub mod instagram;
```

> **Note:** This was deferred from Task 16 to avoid a compile error before the instagram directory exists.

- [ ] **Step 1-7: Mirror TikTok pattern for Instagram Graph API** — archived obsolete direct-upload scope

Same structure as Task 16 but using Facebook Login OAuth and Instagram Graph API's two-phase upload (container -> publish). Archived obsolete direct-upload scope.

Add CSP domains: `graph.facebook.com`, `graph.instagram.com`, `www.facebook.com`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/social/instagram/ src/components/social/Instagram* src/api/instagram.ts src/types/instagram.ts # archived obsolete direct-upload scope
git commit -m "feat: add Instagram Reels upload integration" # archived obsolete direct-upload scope
```

### Task 18: Enhanced local export

**Files:**
- Modify: `src/components/editor/ExportModal.tsx`
- Modify: `src-tauri/src/video/commands.rs`

- [ ] **Step 1: Add format/resolution selector to ExportModal**

Add state:
```typescript
const [format, setFormat] = useState<'mp4' | 'webm' | 'mov'>('mp4');
const [resolution, setResolution] = useState<'1080x1920' | '720x1280'>('1080x1920');
```

Add Select components for format and resolution.

- [ ] **Step 2: Add "Open in Explorer" and "Copy Path" buttons**

```typescript
<Button onClick={() => invoke('plugin:shell|open', { path: outputDir })}>
    {t('export.openInExplorer')}
</Button>
<Button onClick={() => navigator.clipboard.writeText(outputPath)}>
    {t('export.copyPath')}
</Button>
```

- [ ] **Step 3: Add export_video command with format params**

In `video/commands.rs`:
```rust
#[tauri::command]
pub async fn export_video(input: String, output: String, format: String, width: u32, height: u32) -> Result<String, String> {
    // Map format to FFmpeg codec/container
    // Execute FFmpeg with specified params
}
```

- [ ] **Step 4: Run `npx tsc --noEmit`**

- [ ] **Step 5: Commit**

```bash
git add src/components/editor/ExportModal.tsx src-tauri/src/video/commands.rs
git commit -m "feat: enhance local export with format/resolution options"
```

---

## Chunk 4: Video Processing + Editor (Tasks 19-22)

### Task 19: Hardware encoding everywhere

**Files:**
- Modify: `src-tauri/src/video/processor/pipeline.rs` (3 locations)
- Modify: `src-tauri/src/video/processor/effects.rs` (4 locations)
- Modify: `src-tauri/src/video/auto_composer/processing.rs` (2 locations)

- [ ] **Step 1: Replace all 9 libx264 hardcodes with hardware encoder args**

> **IMPORTANT:** `VideoEncoder::get_name()` returns display names like "NVIDIA H.264 (Hardware)" which are NOT valid FFmpeg codec identifiers. Use `get_ffmpeg_args()` instead, which returns `Vec<&'static str>` containing `-c:v`, the codec name, and quality flags.
>
> **Note:** `effects.rs` uses `.arg()` chaining syntax, while `pipeline.rs` and `processing.rs` use array/args syntax. Both patterns are shown below.

**For `pipeline.rs` (array syntax -- 3 locations):** Remove the `"-c:v", "libx264"` and any adjacent preset/crf args from the args array, and instead add encoder args via loop:
```rust
// Instead of: .args([..., "-c:v", "libx264", ...preset/crf args...])
// Use:
let encoder_args = self.optimal_encoder.get_ffmpeg_args();
for arg in &encoder_args {
    cmd.arg(arg);
}
```

**For `effects.rs` (`.arg()` chain syntax -- 4 locations):** Replace `.arg("-c:v").arg("libx264")` with:
```rust
for arg in &self.optimal_encoder.get_ffmpeg_args() {
    cmd.arg(arg);
}
```

**For `processing.rs` (2 locations):** Use the public accessor `self.video_processor.get_optimal_encoder()` (defined at pipeline.rs:107) -- `optimal_encoder` is `pub(super)` and NOT accessible from `auto_composer`:
```rust
for arg in &self.video_processor.get_optimal_encoder().get_ffmpeg_args() {
    cmd.arg(arg);
}
```

- [ ] **Step 2: Implement `execute_with_encoder_fallback()` helper**

> **IMPORTANT:** Per spec, each hardware encoding site must fall back to software encoding on failure.

Add a helper function in `pipeline.rs` (or a shared utility):
```rust
/// Try executing FFmpeg with the given command. If it fails and the encoder
/// is hardware, retry with libx264 as a software fallback.
async fn execute_with_encoder_fallback(
    cmd: &mut TokioCommand,
    encoder: &VideoEncoder,
    fallback_cmd_builder: impl FnOnce() -> TokioCommand,
) -> Result<(), VideoError> {
    let status = cmd.status().await
        .map_err(|e| VideoError::FfmpegSpawn(e.to_string()))?;
    if status.success() {
        return Ok(());
    }
    // If we were already using software encoder, no fallback available
    if encoder.get_name().contains("Software") {
        return Err(VideoError::EncodingFailed);
    }
    tracing::warn!("Hardware encoder failed, retrying with libx264");
    let mut fallback = fallback_cmd_builder();
    fallback.arg("-c:v").arg("libx264").arg("-preset").arg("fast").arg("-crf").arg("23");
    let status = fallback.status().await
        .map_err(|e| VideoError::FfmpegSpawn(e.to_string()))?;
    if !status.success() {
        return Err(VideoError::EncodingFailed);
    }
    Ok(())
}
```

Wrap each of the 9 FFmpeg execution sites in the try-fallback pattern above.

- [ ] **Step 3: Run `cargo check` and `cargo test`**

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/video/
git commit -m "perf: use hardware encoder in all video processing paths with software fallback"
```

### Task 20: Expose effects to frontend

**Files:**
- Modify: `src-tauri/src/video/commands.rs`
- Create: `src/components/editor/EffectsPanel.tsx`
- Modify: `src-tauri/src/main.rs` — register new commands

- [ ] **Step 1: Add Tauri commands for effects**

> **IMPORTANT:** All new commands MUST accept `State<'_, AppState>`, call `require_auth()`, validate paths with `security::validate_video_input_path()` / `security::validate_video_output_path()`, and return `AppResult<T>`.
>
> **Note:** `TextPosition` is an enum (TopLeft, TopRight, BottomLeft, BottomRight, Center), not a struct with x/y. `TextStyle` uses `size` (not `font_size`). `ColorGrading` fields are `f64`, not `f32`.

```rust
#[tauri::command]
pub async fn apply_slow_motion(
    state: State<'_, AppState>,
    input: String,
    output: String,
    speed_factor: f64,
) -> AppResult<String> {
    require_auth(&state)?;
    security::validate_video_input_path(&input)?;
    security::validate_video_output_path(&output)?;

    let processor = state.video_processor.clone();
    processor.apply_slow_motion(&input, &output, speed_factor)
        .await
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| AppError::VideoProcessing(e.to_string()))
}

#[tauri::command]
pub async fn apply_color_grading(
    state: State<'_, AppState>,
    input: String,
    output: String,
    brightness: f64,
    contrast: f64,
    saturation: f64,
) -> AppResult<String> {
    require_auth(&state)?;
    security::validate_video_input_path(&input)?;
    security::validate_video_output_path(&output)?;

    let grading = ColorGrading { brightness, contrast, saturation, ..Default::default() };
    let processor = state.video_processor.clone();
    processor.apply_color_grading(&input, &output, grading)
        .await
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| AppError::VideoProcessing(e.to_string()))
}

#[tauri::command]
pub async fn apply_text_overlay_cmd(
    state: State<'_, AppState>,
    input: String,
    output: String,
    text: String,
    position: String, // "top_left", "top_right", "bottom_left", "bottom_right", "center"
    size: u32,
    color: String,
) -> AppResult<String> {
    require_auth(&state)?;
    security::validate_video_input_path(&input)?;
    security::validate_video_output_path(&output)?;

    let pos = match position.as_str() {
        "top_left" => TextPosition::TopLeft,
        "top_right" => TextPosition::TopRight,
        "bottom_left" => TextPosition::BottomLeft,
        "bottom_right" => TextPosition::BottomRight,
        _ => TextPosition::Center,
    };
    let style = TextStyle { size, color, ..Default::default() };

    let processor = state.video_processor.clone();
    processor.add_text_overlay(&input, &output, &text, pos, style)
        .await
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| AppError::VideoProcessing(e.to_string()))
}
```

- [ ] **Step 2: Create EffectsPanel.tsx**

Panel with: slow-motion slider (0.25x-0.75x), brightness/contrast/saturation sliders, text input with size and color.

> **Note:** The `apply_slow_motion` backend rejects speed values >= 1.0. The slider must be constrained to 0.25-0.75 range for slow-motion only.

- [ ] **Step 3: Register commands in main.rs**

- [ ] **Step 4: Run `cargo check` and `npx tsc --noEmit`**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/video/commands.rs src-tauri/src/main.rs src/components/editor/EffectsPanel.tsx
git commit -m "feat: expose video effects (slow-motion, color grading, text overlay) to frontend"
```

### Task 21: GIF export

**Files:**
- Modify: `src-tauri/src/video/commands.rs`
- Modify: `src/components/editor/ExportModal.tsx`

- [ ] **Step 1: Add export_as_gif command**

> **Note:** Must use `State<'_, AppState>`, `require_auth()`, path validation, and `AppResult<T>` pattern.

```rust
#[tauri::command]
pub async fn export_as_gif(
    state: State<'_, AppState>,
    input: String,
    output: String,
    max_duration: f64,
) -> AppResult<String> {
    require_auth(&state)?;
    security::validate_video_input_path(&input)?;
    security::validate_video_output_path(&output)?;

    let ffmpeg_path = get_ffmpeg_path().map_err(|e| AppError::VideoProcessing(e.to_string()))?;
    let duration = if max_duration > 15.0 { 15.0 } else { max_duration };

    let status = TokioCommand::new(ffmpeg_path)
        .args([
            "-i", &input,
            "-vf", "fps=15,scale=480:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
            "-t", &duration.to_string(),
            "-y", &output,
        ])
        .status().await.map_err(|e| AppError::VideoProcessing(e.to_string()))?;

    if status.success() {
        Ok(output)
    } else {
        Err(AppError::VideoProcessing("GIF export failed".to_string()))
    }
}
```

- [ ] **Step 2: Add GIF option to ExportModal**

Add "GIF" to format selector. When selected, call `export_as_gif` instead of `export_video`.

- [ ] **Step 3: Register command, run checks**

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/video/commands.rs src/components/editor/ExportModal.tsx
git commit -m "feat: add GIF export with palette optimization"
```

### Task 22: Dynamic hotkey registration

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

- [ ] **Step 0: Add required imports**

Add at the top of `hotkey.rs`:
```rust
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_CONTROL, MOD_ALT, MOD_SHIFT, MOD_NOREPEAT,
    VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12,
    VIRTUAL_KEY,
};
```

- [ ] **Step 1: Add key string parser**

```rust
fn parse_hotkey(key_str: &str) -> Option<(HOT_KEY_MODIFIERS, VIRTUAL_KEY)> {
    let parts: Vec<&str> = key_str.split('+').map(|s| s.trim()).collect();
    let mut modifiers = HOT_KEY_MODIFIERS(0);
    let mut vk = None;

    for part in &parts {
        match part.to_uppercase().as_str() {
            "CTRL" | "CONTROL" => modifiers |= MOD_CONTROL,
            "ALT" => modifiers |= MOD_ALT,
            "SHIFT" => modifiers |= MOD_SHIFT,
            key => {
                vk = match key {
                    "F1" => Some(VK_F1), "F2" => Some(VK_F2), /* ... */ "F12" => Some(VK_F12),
                    _ if key.len() == 1 => Some(VIRTUAL_KEY(key.bytes().next()? as u16)),
                    _ => None,
                };
            }
        }
    }
    modifiers |= MOD_NOREPEAT;
    vk.map(|k| (modifiers, k))
}
```

- [ ] **Step 2: Replace hardcoded VK codes with parsed settings**

Read `HotkeySettings` and use `parse_hotkey()` for each binding.

> **IMPORTANT:** `HotkeySettings` field names do NOT match `HotkeyEvent` variants directly. Use this mapping:
> - `manual_save_clip` (settings) maps to `HotkeyEvent::SaveReplay60` (current F8 behavior is `ToggleAutoCapture` -- needs reconciliation)
> - `toggle_recording` (settings) maps to `HotkeyEvent::ToggleAutoCapture`
> - `delete_last_clip` (settings) maps to `HotkeyEvent::SaveReplay30` (currently F10)

- [ ] **Step 3: Run `cargo check`**

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/hotkey.rs
git commit -m "feat: wire hotkey settings to dynamic key registration"
```

---

## Chunk 5: In-Game Overlay + UX (Tasks 23-24)

### Task 23: In-game overlay

**Files:**
- Create: `src-tauri/src/overlay/mod.rs`, `click_through.rs`
- Create: `src/pages/Overlay.tsx`
- Create: `src/components/overlay/RecordingIndicator.tsx`, `ClipSavedToast.tsx`, `EventFeed.tsx`
- Modify: `src-tauri/tauri.conf.json` — add overlay window
- Modify: `src-tauri/src/main.rs` — overlay setup
- Modify: `src-tauri/src/main.rs` — overlay show/hide in game start/end callbacks (game_monitor.rs has no AppHandle)
- Modify: `src/App.tsx` — add /overlay route
- Modify: `src-tauri/src/settings/models.rs` — overlay_enabled field

- [ ] **Step 1: Add overlay_enabled setting**

In `settings/models.rs`:
```rust
    #[serde(default = "default_true")]
    pub overlay_enabled: bool,
```

> **Note:** If `fn default_true() -> bool { true }` does not already exist in `models.rs`, define it alongside the other serde default helpers.

- [ ] **Step 2: Add overlay window to tauri.conf.json**

In the `windows` array:
```json
{
    "label": "overlay",
    "title": "",
    "url": "/overlay",
    "width": 400,
    "height": 200,
    "x": 20,
    "y": 20,
    "resizable": false,
    "decorations": false,
    "transparent": true,
    "alwaysOnTop": true,
    "visible": false,
    "skipTaskbar": true,
    "shadow": false
}
```

- [ ] **Step 3: Create click_through.rs**

> **IMPORTANT:** `window.hwnd()` returns a Tauri HWND wrapper, not a raw Win32 HWND. Accept a raw `isize` pointer and convert it.

```rust
#[cfg(target_os = "windows")]
pub fn make_click_through(raw_hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::*;
    let hwnd = HWND(raw_hwnd as *mut _);
    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            ex_style | WS_EX_LAYERED.0 as i32 | WS_EX_TRANSPARENT.0 as i32,
        );
    }
}
```

- [ ] **Step 3.5: Add `pub mod overlay;` to `src-tauri/src/lib.rs`**

This is required for the overlay module to be accessible from other modules (e.g., `crate::overlay::show_overlay`).

- [ ] **Step 4: Create overlay/mod.rs**

> **Note:** The `window.hwnd()` call returns a Tauri HWND wrapper. Extract the raw pointer with `.0 as isize` before passing to `make_click_through`.

```rust
pub mod click_through;

use tauri::{AppHandle, Manager};
use tracing::info;

pub fn show_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.show();
        #[cfg(target_os = "windows")]
        {
            if let Ok(hwnd) = window.hwnd() {
                click_through::make_click_through(hwnd.0 as isize);
            }
        }
        info!("Overlay shown");
    }
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
        info!("Overlay hidden");
    }
}
```

- [ ] **Step 5: Wire show/hide to game start/end callbacks**

> **IMPORTANT:** `game_monitor.rs` does NOT have access to `AppHandle`. The overlay show/hide calls must be placed in the `game_start_callback` and `game_end_callback` closures defined in `main.rs` (around lines 311-351), which run in the context where `AppHandle` is available via the setup closure.

In `main.rs`, modify the `game_start_callback` closure (around line 311) to capture `app_handle` and call:
```rust
    let settings = app_handle.state::<AppState>().settings.read().await;
    if settings.overlay_enabled {
        crate::overlay::show_overlay(&app_handle);
    }
```

In `main.rs`, modify the `game_end_callback` closure (around line 340) to call:
```rust
    crate::overlay::hide_overlay(&app_handle);
```

Both closures run in the context where `AppHandle` is available via the setup closure.

- [ ] **Step 6: Create Overlay.tsx**

Minimal React page with transparent background:
```tsx
export default function Overlay() {
    return (
        <div className="bg-transparent w-full h-full">
            <RecordingIndicator />
            <ClipSavedToast />
            <EventFeed />
        </div>
    );
}
```

- [ ] **Step 7: Create overlay components**

`RecordingIndicator.tsx` — Red pulsing dot + game timer, listens to `recording-status` event.
`ClipSavedToast.tsx` — Animated toast on `clip-saved` event, auto-dismiss after 3s.
`EventFeed.tsx` — Shows last 3 game events from `game-event` Tauri events.

- [ ] **Step 8: Add overlay rendering to App.tsx**

> **IMPORTANT:** This project uses TanStack Router, NOT React Router. Do not use `<Route>` JSX syntax. Additionally, the overlay MUST NOT be rendered inside `rootRoute` which includes `AppShell` (sidebar, navigation, etc.).

Use conditional rendering based on window label to bypass the router entirely for the overlay window:
```tsx
// In App.tsx, add import:
import Overlay from './pages/Overlay';

// Detect if this is the overlay window:
const isOverlay = window.__TAURI__?.window?.getCurrent()?.label === 'overlay';

// Render overlay directly without AppShell if isOverlay:
if (isOverlay) {
    return <Overlay />;
}
// Otherwise render normal app with TanStack router...
```

- [ ] **Step 9: Run `cargo check` and `npx tsc --noEmit`**

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/overlay/ src/pages/Overlay.tsx src/components/overlay/ src-tauri/tauri.conf.json src-tauri/src/main.rs src-tauri/src/recording/game_monitor.rs src/App.tsx src-tauri/src/settings/models.rs
git commit -m "feat: add in-game transparent overlay (recording status, clip toast, event feed)"
```

### Task 24: Accessibility (a11y)

**Files:**
- Modify: All page components (`src/pages/*.tsx`)
- Modify: Custom interactive components
- Modify: `src/components/settings/GeneralSettings.tsx` — keyboard shortcuts panel

- [ ] **Step 1: Add ARIA landmarks to page layouts**

Wrap main content in `<main>`, navigation in `<nav>`, sidebars in `<aside>`.

- [ ] **Step 2: Add tabIndex and aria-label to interactive elements**

All custom buttons, toggles, and interactive elements that aren't native HTML buttons.

- [ ] **Step 3: Add focus trap to modals**

Ensure all Dialog components trap focus while open (Radix Dialog already does this, verify custom modals).

- [ ] **Step 4: Add auto-focus on page navigation**

```typescript
useEffect(() => {
    const firstFocusable = document.querySelector('[data-autofocus]');
    if (firstFocusable) (firstFocusable as HTMLElement).focus();
}, []);
```

- [ ] **Step 5: Verify color contrast**

Ensure all text meets WCAG AA 4.5:1 ratio against backgrounds.

- [ ] **Step 6: Run `npx tsc --noEmit`**

- [ ] **Step 7: Commit**

```bash
git add src/pages/ src/components/
git commit -m "feat: add a11y keyboard navigation, ARIA landmarks, and focus management"
```

---

## Final Verification

> **Historical verification only:** Completing this checklist would have verified this archived plan's engineering tasks, not current public/commercial readiness. Current readiness still requires E5 Field QA and excludes TikTok/Instagram direct upload.

- [ ] **Run full Rust test suite**: `cd src-tauri && cargo test`
- [ ] **Run TypeScript check**: `npx tsc --noEmit`
- [ ] **Run Rust compilation**: `cd src-tauri && cargo check`
- [ ] **Verify all 24 items are implemented** against spec checklist
