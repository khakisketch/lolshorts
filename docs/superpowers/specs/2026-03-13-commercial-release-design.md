# LoLShorts Commercial Release Design Specification

**Date**: 2026-03-13
**Status**: Archived and superseded by the current non-payment readiness plan. Do not treat as active implementation scope.
**Scope**: Historical 24-item commercial-service design. Production-ready and direct-upload goals require current Field QA and are not active claims.

> **Superseded notice:** This spec predates the current LoLShorts commercial readiness plan. Payment/Toss remains deferred, TikTok/Instagram direct upload is not revived, and commercial readiness claims require E5 Field QA evidence.

---

## Overview

LoLShorts is a Tauri 2.0 desktop app (React 18 + TypeScript + Rust + FFmpeg) that auto-records League of Legends gameplay and creates YouTube Shorts. This spec covers all improvements needed to reach commercial release quality.

**Target**: Korean LoL players
**Monetization**: Freemium with PRO subscription (KRW 9,900/month)
**Platforms**: Windows 10/11 (primary)

---

## Domain 1: Recording Pipeline

### 1.1 WASAPI Loopback Audio Capture

**Problem**: Current DirectShow `Stereo Mix` fallback fails silently on most modern Windows PCs. Users get silent clips.

**Solution**: Replace DirectShow audio with WASAPI loopback capture via `cpal` crate.

**Architecture**:
```
cpal loopback stream (default output device)
    → PCM samples → WAV file writer (ring buffer)
        → FFmpeg muxes video + audio WAV as second -i input
```

**Files to modify**:
- `src-tauri/Cargo.toml` — add `cpal = "0.15"`
- `src-tauri/src/recording/audio.rs` — replace `list_audio_devices()` and `AudioDeviceManager` DirectShow logic with WASAPI loopback initialization via cpal
- `src-tauri/src/recording/integration_backend/segment_recorder.rs:68-93` — replace `list_audio_devices_ffmpeg()` DirectShow device lookup and `-f dshow` audio args with pre-captured WAV file as `-i <wav_path>` second input

**Audio synchronization strategy**: Use a post-capture mux approach rather than live pipe:
1. cpal loopback writes PCM to a temp WAV file independently
2. FFmpeg records video-only segments as current behavior
3. On clip extraction, FFmpeg muxes video segment + WAV audio with `-itsoffset` for sync alignment
4. This avoids pipe synchronization issues between cpal writer and FFmpeg reader

**Behavior**:
1. On recording start, open cpal loopback stream on default output device (requires Windows 10 1903+)
2. Write PCM samples to a rolling temp WAV file (aligned with segment boundaries)
3. On clip save, FFmpeg command includes `-i <wav_path>` as second input with timestamp alignment
4. On recording stop, close cpal stream and finalize WAV
5. If no audio device available, log warning and proceed video-only (existing fallback behavior preserved)
6. Fallback: if cpal initialization fails, attempt legacy DirectShow path before going video-only

**Testing**: Manual test on machine without Stereo Mix (user's dev environment confirms this scenario). Verify cpal 0.15 WASAPI loopback on Windows 10 1903+ and Windows 11.

### 1.2 FFmpeg Process Health Monitoring

**Problem**: FFmpeg stderr is piped (`segment_recorder.rs:160`, `cmd.stderr(Stdio::piped())`) but never consumed. The child process is stored at line 169-170 (`self.ffmpeg_process = Some(child)`) without extracting the stderr handle. Process can deadlock on full OS pipe buffer. Silent recording failures go undetected.

**Solution**: Spawn a tokio task to continuously read stderr. Monitor process exit.

**Files to modify**:
- `src-tauri/src/recording/integration_backend/segment_recorder.rs` — after line 170 (child process storage), take stderr handle before storing child

**Behavior**:
1. After spawning FFmpeg at line 169, take ownership of `stderr` handle via `child.stderr.take()`
2. Store child (without stderr) in `self.ffmpeg_process`
3. Spawn `tokio::spawn` task that reads stderr line-by-line via `BufReader::new(stderr).lines()`
4. Log each line at `debug!` level, detect error patterns at `error!` level
5. If process exits unexpectedly (exit code != 0), emit `recording_error` event to frontend
6. Attempt automatic restart once if recording was in progress

### 1.3 Missing LoL Event Detection

**Problem**: Elder Dragon, Voidgrubs/Horde, Atakhan, and shutdown kills are not detected despite settings toggles existing in the frontend.

**Solution**: Extend `EventTrigger` enum and `detect_trigger()` logic.

**Files to modify**:
- `src-tauri/src/recording/live_client.rs` — EventTrigger enum + detect_trigger()
- `src-tauri/src/recording/auto_clip_manager.rs` — should_record_event + trigger_to_event_type
- `src-tauri/src/settings/models.rs` — verify all filter fields exist

**New EventTrigger variants**:
```rust
ElderDragonKill,  // DragonKill where dragon type contains "Elder"
VoidgrubsKill,    // EventName == "HordeKill"
AtakhanKill,      // EventName == "AtakhanKill"
Shutdown,         // ChampionKill where victim had 3+ kill streak
```

**New settings fields required** (in `settings/models.rs` `EventFilterSettings`):
```rust
#[serde(default = "default_true")]
pub record_voidgrubs: bool,
#[serde(default = "default_true")]
pub record_atakhan: bool,
```
Note: `record_elder` (line 95) and `record_shutdown` (line 87) already exist. All new fields MUST use `#[serde(default)]` for backward compatibility with existing settings files on disk.

**New match arms in `auto_clip_manager.rs` `should_record_event`**:
```rust
EventTrigger::ElderDragonKill => settings.event_filter.record_elder,
EventTrigger::VoidgrubsKill => settings.event_filter.record_voidgrubs,
EventTrigger::AtakhanKill => settings.event_filter.record_atakhan,
EventTrigger::Shutdown => settings.event_filter.record_shutdown,
```

**New match arms in `trigger_to_event_type`**:
```rust
EventTrigger::ElderDragonKill => EventType::Custom("ElderDragonKill".to_string()),
EventTrigger::VoidgrubsKill => EventType::Custom("VoidgrubsKill".to_string()),
EventTrigger::AtakhanKill => EventType::Custom("AtakhanKill".to_string()),
EventTrigger::Shutdown => EventType::Custom("Shutdown".to_string()),
```

**Frontend**: Add toggles for Voidgrubs and Atakhan in `EventFilterSettings.tsx` (Elder and Shutdown toggles already exist).

**Detection logic**:
- `ElderDragonKill`: Within DragonKill handler, check event data for "Elder" dragon type. Live Client API includes dragon type in the event.
- `VoidgrubsKill`: Match `EventName == "HordeKill"` with killer == player_name
- `AtakhanKill`: Match `EventName == "AtakhanKill"` (Season 15 objective)
- `Shutdown`: Track per-player kill counts via a `HashMap<String, u32>` in LiveClientMonitor. When a player with 3+ consecutive kills dies, trigger Shutdown event

**Priority/duration values for new variants** (in `live_client.rs` `priority()`, `pre_duration()`, `post_duration()`):

| Variant | priority | pre_duration | post_duration | Rationale |
|---------|----------|-------------|---------------|-----------|
| `ElderDragonKill` | 4 | 15s | 5s | High-impact late-game objective |
| `VoidgrubsKill` | 2 | 10s | 3s | Early-game objective, moderate impact |
| `AtakhanKill` | 3 | 15s | 5s | New major objective |
| `Shutdown` | 3 | 10s | 5s | Exciting kill streak end |

Note: `priority()`, `pre_duration()`, `post_duration()` all have catch-all `_ => default` arms, so compilation won't break — but these explicit values ensure optimal clip timing rather than generic defaults.

### 1.4 Game Mode Filtering

**Problem**: `game_monitor.rs:290-301` already filters by broad game type (TFT, Replay), but the granular `GameModeSettings` toggles (ranked_solo, ranked_flex, aram, normal_blind, normal_draft, etc.) are never checked in the clip recording path.

**Solution**: Add granular game mode check to `should_record_event` in auto_clip_manager.

**Files to modify**:
- `src-tauri/src/recording/auto_clip_manager.rs:311-338` — add game mode check
- `src-tauri/src/recording/game_monitor.rs` — pass current game_mode string to AutoClipManager

**Game mode mapping** (Live Client API `gameData.gameMode` → `GameModeSettings` field):
| API gameMode | Queue type (from LCU) | Settings field |
|---|---|---|
| "CLASSIC" | RANKED_SOLO_5x5 | `ranked_solo` |
| "CLASSIC" | RANKED_FLEX_SR | `ranked_flex` |
| "CLASSIC" | NORMAL_BLIND | `normal_blind` |
| "CLASSIC" | NORMAL_DRAFT | `normal_draft` |
| "ARAM" | * | `aram` |
| "URF" / "ARURF" | * | `special_modes` |

Note: "CLASSIC" maps to multiple queue types. Differentiation requires LCU API queue ID (already available from `game_monitor.rs` LCU integration).

**Behavior**:
1. AutoClipManager stores `current_game_mode: Arc<RwLock<String>>` and `current_queue_id: Arc<RwLock<Option<u32>>>`
2. GameMonitor sets both when game starts (gameMode from Live Client API, queue ID from LCU)
3. `should_record_event` maps (gameMode, queue_id) to the correct `GameModeSettings` toggle
4. If mode is disabled, event is silently dropped

### 1.5 Steal Detection

**Problem**: `EventTrigger::Steal` exists but `detect_trigger()` never returns it.

**Solution**: Compare killer team vs player team on DragonKill/BaronKill events.

**Files to modify**:
- `src-tauri/src/recording/live_client.rs` — detect_trigger() DragonKill/BaronKill handlers

**Detection logic** (simplified heuristic — Live Client API does not provide player positions):
1. On DragonKill/BaronKill, get killer_name from event
2. Look up killer's team from `all_players` data (cached in GameStateCache)
3. Look up player's team
4. If killer is on player's team AND the other team had any ChampionKill events within 10 seconds before the objective kill → classify as Steal (contested objective won by player's team)
5. If killer IS the player specifically in a contested scenario → higher priority Steal
6. **Limitation**: This heuristic may produce false positives (e.g., unrelated kills in lane near objective timing). This is acceptable for a highlight tool — false positive steals are still exciting clips. No "near the objective" spatial filtering is possible without position data.

### 1.6 Segment Sorting Fix

**Problem**: Segments sorted by filesystem mtime (`segment_recorder.rs:288-296`), which can be identical on fast operations.

**Solution**: Parse segment number from filename.

**Files to modify**:
- `src-tauri/src/recording/integration_backend/segment_recorder.rs:288-296`

**Implementation**:
```rust
segments.sort_by_key(|path| {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("segment_"))
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(0)
});
```

---

## Domain 2: Infrastructure and Stability

### 2.1 Auto-Updater

**Problem**: Updater pubkey in tauri.conf.json is a placeholder. No `tauri-plugin-updater` dependency.

**Solution**: Generate real keys and integrate updater plugin.

**Files to modify**:
- `src-tauri/Cargo.toml` — add `tauri-plugin-updater`
- `src-tauri/tauri.conf.json:97` — replace placeholder pubkey
- `src-tauri/src/main.rs` — add update check on startup

**Behavior**:
1. Run `scripts/setup-updater-keys.ps1` to generate Ed25519 keypair
2. Store private key in CI secrets, public key in tauri.conf.json
3. On app startup, check for updates via configured endpoint
4. If update available, show dialog: "새 버전이 있습니다. 업데이트하시겠습니까?"
5. Download + install in background, apply on next restart

### 2.2 OS Autostart

**Problem**: `auto_start_with_league: true` setting exists but does nothing.

**Solution**: Integrate `tauri-plugin-autostart`.

**Files to modify**:
- `src-tauri/Cargo.toml` — add `tauri-plugin-autostart`
- `src-tauri/src/main.rs` — register plugin, sync with settings
- `src/components/settings/GeneralSettings.tsx` — wire toggle to actual behavior

**Behavior**:
1. On settings load, read `auto_start_with_league` value
2. Call `AutoLaunch::enable()` or `disable()` accordingly
3. When user toggles in UI, immediately apply change
4. Autostart entry is created in Windows registry `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`

### 2.3 Sentry Crash Reporting

**Problem**: No crash visibility in production. ErrorBoundary's error tracking is commented out.

**Solution**: Integrate Sentry on both frontend and backend.

**Files to modify**:
- `package.json` — add `@sentry/react`
- `src/App.tsx:261-265` — wire ErrorBoundary to Sentry.captureException
- `src/main.tsx` — Sentry.init() with DSN
- `src-tauri/Cargo.toml` — add `sentry = "0.34"`
- `src-tauri/src/main.rs` — sentry::init() with panic handler
- `src/components/settings/GeneralSettings.tsx` — add "크래시 리포트 전송" consent toggle
- `src-tauri/src/settings/models.rs` — add `crash_reporting_enabled: bool`

**Privacy**:
- Default: disabled (opt-in)
- Settings toggle with explanation text
- No PII sent (only stack traces, OS version, app version)
- Sentry free tier: 5,000 events/month

### 2.4 Settings Page Error Handling

**Problem**: `Settings.tsx:53-58,65-68,77-80,91-93` have empty catch blocks.

**Solution**: Replace with toast notifications.

**Files to modify**:
- `src/pages/Settings.tsx` — all catch blocks

**Implementation**:
```typescript
catch (error) {
    toast.error(t('settings.error.loadFailed'));
    console.error('Settings load failed:', error);
}
```

Add i18n keys: `settings.error.loadFailed`, `settings.error.saveFailed`, `settings.error.resetFailed`, `settings.error.licenseFailed` to all 20 locale files.

### 2.5 Toast Notifications Across All Pages

**Problem**: Only 3 pages use toast. Dashboard, Editor, AutoEdit, YouTube have zero user feedback.

**Solution**: Add toast to every async operation.

**Files to modify**:
- `src/pages/Dashboard.tsx` — recording start/stop
- `src/pages/Editor.tsx` — export, save
- `src/pages/AutoEdit.tsx` — auto-edit start/complete/fail
- `src/pages/YouTube.tsx` — upload start/complete/fail
- `src/pages/Games.tsx` — game detection status
- All 20 locale files — new toast i18n keys

**Pattern**:
```typescript
try {
    await operation();
    toast.success(t('operation.success'));
} catch (error) {
    toast.error(t('operation.failed'));
}
```

### 2.6 Cleanup on Shutdown

**Problem**: `cleanup_on_shutdown()` exists in `utils/cleanup.rs:91-107` but is never called. The main window's `CloseRequested` event is intercepted by `tray.rs:67` (`setup_close_to_tray()`) which calls `api.prevent_close()` to minimize to tray — so a builder-level `on_window_event` for `CloseRequested` would never fire.

**Solution**: Wire cleanup to the tray "Quit" menu handler and Tauri's `RunEvent::Exit`.

**Files to modify**:
- `src-tauri/src/tray.rs` — in the tray menu event handler, call `cleanup_on_shutdown()` when user clicks "Quit" before exiting
- `src-tauri/src/main.rs` — use `Builder::build()` + `app.run()` pattern with `RunEvent::Exit` handler as a safety net

**Implementation (tray.rs — Quit menu item handler)**:
```rust
"quit" => {
    let _ = crate::utils::cleanup::cleanup_on_shutdown();
    app.exit(0);
}
```

**Note**: `main.rs:517` currently uses `.run(tauri::generate_context!())` which does not accept a RunEvent closure. Adding a `RunEvent::Exit` handler would require migrating from `.run()` to `.build()` + `app.run()` — a significant refactor. **Therefore, rely solely on the tray Quit handler for cleanup.** The tray handler is the primary and expected exit path for this app. System-level process kills (Task Manager, shutdown) cannot be intercepted by any mechanism, so the tray handler alone is sufficient.

This ensures cleanup runs when users exit through the expected path (tray → Quit).

---

## Domain 3: YouTube Enhancement + Social Platform Expansion

### 3.1 Resumable Upload

**Problem**: `youtube_upload_video` at `commands.rs:271` uses `upload_client.upload_video()` (multipart). Large files fail on network interruption. `upload_video_resumable()` exists at `upload.rs:456` but is unused.

**Solution**: Switch to existing `upload_video_resumable()` implementation with multipart fallback.

**Files to modify**:
- `src-tauri/src/youtube/commands.rs:273` — change `upload_video()` call to `upload_video_resumable()`
- Frontend upload component — listen for progress events

**Behavior**:
1. Replace `upload_client.upload_video()` with `upload_client.upload_video_resumable()`
2. Both methods share the same argument signature, so the swap is direct
3. Resumable upload emits progress events via Tauri event system
4. Frontend shows progress bar with percentage
5. On network failure, upload automatically retries from last successful chunk
6. Fallback: if resumable init fails (e.g., quota error 400), fall back to multipart `upload_video()`

### 3.2 Quota Tracking

**Problem**: `youtube_quota_used` is never incremented after uploads.

**Solution**: Increment quota counter on successful upload.

**Files to modify**:
- `src-tauri/src/youtube/commands.rs` — after successful upload
- `src-tauri/src/youtube/commands.rs:367-380` — quota read logic

**Behavior**:
1. After successful upload (after `Ok(video)` return in `youtube_upload_video`, before the final return statement), increment stored quota by 1,600 units (YouTube Data API v3 `videos.insert` cost)
2. Store quota with user-scoped date key: `youtube_quota_{hex_user_id}_{YYYY-MM-DD}`, where `hex_user_id` is the raw UTF-8 app user ID bytes hex-encoded, using `chrono_tz::America::Los_Angeles` timezone (YouTube quota resets at midnight Pacific Time, not UTC)
3. On read, check if stored date matches today (Pacific Time). If not, reset to 0
4. QuotaDisplay component reflects real usage
5. Add `chrono-tz` to Cargo.toml dependencies if not already present

### 3.3 Scheduled Upload Background Executor

**Problem**: Scheduled uploads are saved to JSON but never executed.

**Solution**: Background tokio task that polls the queue.

**Files to modify**:
- `src-tauri/src/youtube/commands.rs` — add `start_upload_scheduler()` function
- `src-tauri/src/main.rs` — spawn scheduler on app start

**Behavior**:
1. On app start, spawn a tokio task that runs every 60 seconds
2. Read scheduled upload queue from storage
3. For items where `scheduled_time <= Utc::now()` and status == "pending":
   - Set status to "uploading"
   - Execute `upload_video_resumable()`
   - On success: set status to "completed", emit event
   - On failure: set status to "failed", store error message, emit event
4. Frontend receives events and updates queue UI in real-time

### 3.4 TikTok Upload Integration, archived obsolete direct-upload scope

> **Archived:** This TikTok direct upload section is superseded. Current scope keeps TikTok as preset/manual guidance only and does not revive direct upload.

**Problem**: No TikTok support. TikTok is a major Shorts distribution platform. Archived obsolete direct-upload scope.

**Solution**: New `social/tiktok/` module using TikTok Creator API. Archived obsolete direct-upload scope.

**New files**:
- `src-tauri/src/social/mod.rs` — social module root
- `src-tauri/src/social/tiktok/mod.rs`
- `src-tauri/src/social/tiktok/auth.rs` — OAuth 2.0 PKCE flow
- `src-tauri/src/social/tiktok/upload.rs` — video upload via Content Posting API
- `src-tauri/src/social/tiktok/commands.rs` — Tauri commands
- `src/components/social/TikTokAuth.tsx`, archived obsolete direct-upload scope
- `src/components/social/TikTokUpload.tsx`, archived obsolete direct-upload scope
- `src/api/tiktok.ts`
- `src/types/tiktok.ts`

**API flow**:
1. OAuth: `https://www.tiktok.com/v2/auth/authorize/` with `video.upload` scope
2. Upload: POST to `https://open.tiktokapis.com/v2/post/publish/video/init/` (chunk upload)
3. Status check: poll publish status endpoint

**Frontend**: Mirror YouTube upload UI pattern. TikTok tab in social upload page. Archived obsolete direct-upload scope.

### 3.5 Instagram Reels Upload Integration, archived obsolete direct-upload scope

> **Archived:** This Instagram direct upload section is superseded. Current scope keeps Instagram as preset/manual guidance only and does not revive direct upload.

**Problem**: No Instagram Reels support. Archived obsolete direct-upload scope.

**Solution**: New `social/instagram/` module using Instagram Graph API. Archived obsolete direct-upload scope.

**New files**:
- `src-tauri/src/social/instagram/mod.rs`
- `src-tauri/src/social/instagram/auth.rs` — Facebook Login OAuth
- `src-tauri/src/social/instagram/upload.rs` — two-phase upload (container → publish)
- `src-tauri/src/social/instagram/commands.rs`
- `src/components/social/InstagramAuth.tsx`, archived obsolete direct-upload scope
- `src/components/social/InstagramUpload.tsx`, archived obsolete direct-upload scope
- `src/api/instagram.ts`
- `src/types/instagram.ts`

**API flow**:
1. Facebook Login OAuth with `instagram_content_publish` permission
2. Create media container: POST `/me/media` with `media_type=REELS`, `video_url`
3. Poll container status until ready
4. Publish: POST `/me/media_publish` with container ID

### 3.6 Enhanced Local Export

**Problem**: Limited export options.

**Solution**: Expand ExportModal with format, resolution, and sharing options.

**Files to modify**:
- `src/components/editor/ExportModal.tsx` — add format/resolution selectors
- `src-tauri/src/video/commands.rs` — add `export_video` command with format params

**Options**:
- Format: MP4 (H.264), WebM (VP9), MOV (ProRes)
- Resolution: 1080x1920 (Full HD Shorts), 720x1280 (lightweight), custom
- Actions: "클립보드에 경로 복사", "파일 탐색기에서 열기"
- Tauri plugins: `tauri-plugin-clipboard-manager`, `tauri-plugin-shell`

---

## Domain 4: Video Processing + Editor Enhancement

### 4.1 Hardware Encoding in compose_shorts

**Problem**: Multiple locations hardcode `libx264` ignoring detected hardware encoder:
- `pipeline.rs:287` in `compose_shorts` (multi-clip concat path)
- `pipeline.rs:~506` in `scale_and_crop_clip` (single-clip composition path)
- `pipeline.rs:~460` in `compose_montage` fallback path

**Solution**: Use `self.optimal_encoder` in all three locations.

**Files to modify**:
- `src-tauri/src/video/processor/pipeline.rs` — all three `"-c:v", "libx264"` occurrences

**Implementation**:
```rust
"-c:v", self.optimal_encoder.get_name(),
// Add encoder-specific quality params per encoder type
```
With fallback: if hardware encode fails (non-zero exit code), retry the same operation with `"-c:v", "libx264"`.

**Additionally**: `effects.rs` also hardcodes `libx264` in 4 locations:
- `effects.rs:116` in `apply_transition`
- `effects.rs:173` in `apply_color_grading`
- `effects.rs:216` in `apply_slow_motion`
- `effects.rs:267` in `add_text_overlay`

All 4 must also use `self.optimal_encoder.get_name()` with the same software fallback. These methods are on `VideoProcessor` which already has `self.optimal_encoder` available, so the change is straightforward.

**Additionally**: `auto_composer/processing.rs` also hardcodes `libx264` in 2 locations:
- `processing.rs:248` in `apply_canvas_overlay`
- `processing.rs:298` in `apply_watermark_only`

`AutoComposer` holds `Arc<VideoProcessor>` (line 15), so it can access `self.video_processor.optimal_encoder.get_name()` for the encoder name.

**Total**: 9 `libx264` hardcodes across `pipeline.rs` (3), `effects.rs` (4), and `processing.rs` (2).

### 4.2 Frontend Effects Exposure

**Problem**: `apply_slow_motion()`, `apply_color_grading()`, `apply_text_overlay()` exist in `effects.rs` but have no Tauri commands.

**Solution**: Add Tauri commands and editor UI.

**New Tauri commands** in `src-tauri/src/video/commands.rs`:
- `apply_slow_motion(input, output, speed_factor)` — calls `effects.rs:186` (`apply_slow_motion`)
- `apply_color_grading(input, output, grading)` — calls `effects.rs:129` (`apply_color_grading`)
- `apply_text_overlay(input, output, text, style, position)` — calls `effects.rs:229` (`add_text_overlay`, note: function name is `add_text_overlay` not `apply_text_overlay`)

Each Tauri command wrapper converts `Result<PathBuf, VideoError>` to `Result<String, String>` matching existing command patterns in `video/commands.rs`.

**Frontend**:
- `src/components/editor/TimelineClip.tsx` — right-click context menu "효과 적용"
- `src/components/editor/EffectsPanel.tsx` (new) — speed slider, color grading controls, text input
- Wire to Tauri commands via `src/api/video.ts`

### 4.3 GIF Export

**Problem**: No GIF export for social sharing.

**Solution**: FFmpeg palette-based high-quality GIF generation.

**New files**:
- `src-tauri/src/video/commands.rs` — `export_as_gif` command

**FFmpeg pipeline**:
```
ffmpeg -i input.mp4 -vf "fps=15,scale=480:-1:flags=lanczos,split[s0][s1];
  [s0]palettegen[p];[s1][p]paletteuse" -t 15 output.gif
```

**Constraints**: Max 15 seconds, 480px width, 15fps. Estimated output: 5-10MB for a 10s clip.

**Frontend**: "GIF" button in ExportModal next to MP4/WebM/MOV options.

### 4.4 Dynamic Hotkey Registration

**Problem**: `hotkey.rs` hardcodes F8/F9/F10, ignoring `HotkeySettings`.

**Solution**: Read settings and dynamically register.

**Files to modify**:
- `src-tauri/src/hotkey.rs:140-152` — dynamic registration
- `src-tauri/src/hotkey.rs` — add `parse_hotkey(key_string) -> VirtualKey` function

**Implementation**:
1. On start, read `HotkeySettings` from settings
2. Parse key strings ("F8", "Ctrl+F9", etc.) to Win32 VK codes + modifiers
3. Register with `RegisterHotKey()` using parsed values
4. On settings change (via Tauri event), unregister old → register new
5. Support modifier keys: Ctrl, Alt, Shift

**Key mapping** supports: F1-F12, 0-9, A-Z, plus modifiers.

---

## Domain 5: In-Game Overlay + UX Polish

### 5.1 In-Game Overlay

**Problem**: No visual feedback during gameplay. Users don't know if recording is active.

**Solution**: Tauri transparent window overlay, validated safe by Manasight (same stack).

**New files**:
- `src/pages/Overlay.tsx` — overlay React page
- `src/components/overlay/RecordingIndicator.tsx` — red dot + timer
- `src/components/overlay/ClipSavedToast.tsx` — "클립 저장됨!" animation
- `src/components/overlay/EventFeed.tsx` — recent 3 events
- `src-tauri/src/overlay/mod.rs` — overlay window management
- `src-tauri/src/overlay/click_through.rs` — WS_EX_TRANSPARENT setup

**tauri.conf.json addition**:
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

**Click-through (Rust)**:
```rust
use windows::Win32::UI::WindowsAndMessaging::*;
fn make_click_through(hwnd: HWND) {
    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE,
            ex_style | WS_EX_LAYERED.0 as i32 | WS_EX_TRANSPARENT.0 as i32);
    }
}
```

**Show/hide integration** in `game_monitor.rs`:
- Game start → `overlay_window.show()`
- Game end → `overlay_window.hide()`

**Data flow**: LiveClientMonitor → Tauri events → Overlay React page

**Vanguard safety**: Zero game process interaction. Uses only Live Client API (port 2999) HTTP endpoint. No DLL injection, no memory reading, no DirectX hooking.

### 5.2 Accessibility (a11y)

**Problem**: Custom views lack keyboard navigation, ARIA landmarks, focus management.

**Solution**: Systematic a11y pass across all interactive components.

**Files to modify**: All page components and custom interactive elements.

**Checklist**:
1. All interactive elements have `tabIndex={0}` or native focusability
2. Custom components have appropriate `role` and `aria-label`
3. Page sections wrapped in `<main>`, `<nav>`, `<aside>` landmarks
4. Modals implement focus trap (focus stays within modal while open)
5. First focusable element auto-focused on page navigation
6. Color contrast meets WCAG AA (4.5:1 for text)
7. Keyboard shortcuts documented in Settings > Accessibility section

---

## New Dependencies Summary

### Rust (Cargo.toml)
| Crate | Version | Purpose |
|-------|---------|---------|
| `cpal` | 0.15 | WASAPI loopback audio capture |
| `tauri-plugin-updater` | latest | Auto-update mechanism |
| `tauri-plugin-autostart` | latest | OS autostart registration |
| `sentry` | 0.34 | Backend crash reporting |
| `chrono-tz` | 0.9 | Timezone-aware quota reset (Pacific Time) |

### JavaScript (package.json)
| Package | Purpose |
|---------|---------|
| `@sentry/react` | Frontend crash reporting |

### New Module Structure
```
src-tauri/src/
  social/
    mod.rs
    tiktok/
      mod.rs, auth.rs, upload.rs, commands.rs
    instagram/
      mod.rs, auth.rs, upload.rs, commands.rs
  overlay/
    mod.rs, click_through.rs

src/
  pages/
    Overlay.tsx
  components/
    social/
TikTokAuth.tsx, TikTokUpload.tsx, archived obsolete direct-upload scope
InstagramAuth.tsx, InstagramUpload.tsx, archived obsolete direct-upload scope
    overlay/
      RecordingIndicator.tsx, ClipSavedToast.tsx, EventFeed.tsx
    editor/
      EffectsPanel.tsx
  api/
    tiktok.ts, instagram.ts
  types/
    tiktok.ts, instagram.ts
```

---

## Settings Migration Strategy

Adding new fields to Rust settings structs (`EventFilterSettings`, `GeneralSettings`) requires backward compatibility with settings files already on disk.

**Rule**: ALL new fields MUST use `#[serde(default)]` or `#[serde(default = "default_fn")]` to ensure existing JSON files deserialize without errors.

**New fields across all items**:
| Struct | Field | Default | Item |
|--------|-------|---------|------|
| `EventFilterSettings` | `record_voidgrubs: bool` | `true` | 1.3 |
| `EventFilterSettings` | `record_atakhan: bool` | `true` | 1.3 |
| `GeneralSettings` | `crash_reporting_enabled: bool` | `false` | 2.3 |

Existing fields that already have serde defaults: `record_elder`, `record_shutdown`, `record_first_blood`, `record_deaths`, `record_assists`, `record_herald`, `record_game_end`.

**Testing**: Deserialize a settings JSON from a previous version (without new fields) and verify defaults are applied correctly.

---

## Tauri Plugin Registration

All new Tauri plugins must be registered in the builder chain in `main.rs`:

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_updater::Builder::new().build())    // 2.1
    .plugin(tauri_plugin_autostart::init(              // 2.2
        tauri_plugin_autostart::MacosLauncher::LaunchAgent, None
    ))
    .plugin(tauri_plugin_clipboard_manager::init())          // 3.6
    .plugin(tauri_plugin_shell::init())                      // 3.6
    // ... existing plugins
```

---

## Social Platform Credential Management

TikTok and Instagram require API app credentials (client_id, client_secret). Archived obsolete direct-upload scope.

**Storage**: Environment variables for development, Tauri resource file for production builds.
- `TIKTOK_CLIENT_KEY` / `TIKTOK_CLIENT_SECRET`
- `INSTAGRAM_APP_ID` / `INSTAGRAM_APP_SECRET`

**OAuth redirect**: Both platforms use `http://localhost:{port}/callback` for desktop apps. Each auth module spins up a temporary local HTTP server (like the existing YouTube OAuth flow) to capture the redirect.

**Token refresh**: Both TikTok and Instagram tokens expire. Each auth module stores refresh tokens in the app's secure storage (via Tauri's `tauri-plugin-store`) and implements automatic refresh on 401 responses. Archived obsolete direct-upload scope.

**Rate limiting**: TikTok Content Posting API has daily upload limits. Instagram Graph API has rate limits per user. Each upload command checks rate limit headers and surfaces errors to the user via toast. Archived obsolete direct-upload scope.

**CSP update required**: `tauri.conf.json` line 28 has a strict Content Security Policy. Add the following domains:
- TikTok: `open.tiktokapis.com`, `www.tiktok.com`, archived obsolete direct-upload scope
- Instagram/Facebook: `graph.facebook.com`, `graph.instagram.com`, `www.facebook.com`, archived obsolete direct-upload scope

**API approval lead time**: Both TikTok Creator API and Instagram Graph API require app review/approval that can take weeks. These items should be started early in the implementation timeline. If approval is delayed, the modules can be built and tested against sandbox endpoints while awaiting production access. Archived obsolete direct-upload scope.

---

## Overlay Feature Flag

The in-game overlay is a new, unproven feature. Include a feature flag for safety.

**Settings field**: `overlay_enabled: bool` (default: `true`)
**Behavior**: If disabled, overlay window is never created or shown, regardless of game state.
**Contingency**: If any Vanguard-related reports emerge, the overlay can be disabled via settings or remote config without an app update.

---

## Frontend Routing for Overlay

Add overlay route to `src/App.tsx` router configuration:

```tsx
<Route path="/overlay" element={<Overlay />} />
```

The overlay window's URL is set to `/overlay` in `tauri.conf.json`. This route renders a minimal React page with no sidebar/shell — just the overlay components (RecordingIndicator, ClipSavedToast, EventFeed) on a transparent background.

---

## Concurrent Upload Handling (Scheduled Uploads)

**Queue locking**: Use a file-based lock (`upload_queue.lock`) to prevent concurrent scheduler runs.
**In-progress persistence**: Before starting an upload, write status `"uploading"` to the queue JSON. On app restart, check for `"uploading"` items and retry them.
**Concurrent limit**: Maximum 1 upload at a time. Queue items are processed sequentially.
**App close during upload**: The upload task respects Tauri's shutdown signal. If interrupted, the resumable upload session URI is stored for retry on next launch.

---

## Success Criteria

1. All 42 existing Rust tests pass
2. All 109 E2E tests pass
3. `cargo check` compiles with zero errors
4. `tsc --noEmit` passes with zero errors
5. WASAPI audio produces audible clips on machine without Stereo Mix
6. YouTube resumable upload completes 500MB+ file with simulated network interruption
7. TikTok and Instagram OAuth flow completes and uploads a test video, obsolete historical direct-upload criterion. Not current scope.
8. Overlay appears during game, disappears after, zero Vanguard warnings
9. Auto-updater successfully applies an update from test endpoint
10. Sentry receives test crash report from both frontend and backend
