# LoLShorts Commercial Completeness (Phase 2) Design Specification

**Date:** 2026-03-14
**Status:** Archived and superseded by the current non-payment readiness plan. Do not treat as active implementation scope.
**Scope:** 69 items across 7 cross-cutting domains (6 verified as already implemented)
**Goal:** Historical design goal only. The 100% commercial readiness target is superseded and requires current E5 Field QA before any public claim.

> **Superseded notice:** This spec predates the current readiness plan. Payment/Toss remains deferred, TikTok/Instagram direct upload is obsolete/not current direct-upload scope, and commercial readiness claims require Field QA evidence.

---

## Context

Phase 1 (24 tasks) described core features: WASAPI audio, auto-updater plugin, YouTube resumable upload, Sentry, new LoL events, TikTok/Instagram upload, overlay, hotkeys, hardware encoding, etc. TikTok/Instagram upload is obsolete historical scope and is not part of the current direct-upload plan.

> **Archived direct-upload note:** Later TikTok/Instagram mentions in this spec are historical design references only. They are superseded by the current plan, which keeps TikTok and Instagram out of active direct-upload scope.

Phase 2 addresses all remaining gaps identified by a 4-agent deep analysis covering:
- Rust backend completeness
- React frontend quality
- Recording/video pipeline robustness
- Build/deploy/infrastructure readiness

---

## Domain 1: Security & Input Validation (12 items)

### 1.1 Tauri Command Input Validation

**Problem:** `security.rs` has comprehensive validation functions (`validate_game_id`, `validate_video_input_path`, `validate_target_duration`, `validate_id`) but they are NOT called from any Tauri command handler.

**Solution:** Apply validation to every Tauri command that accepts user input:

| Command | Parameter | Validator |
|---------|-----------|-----------|
| `get_game_metadata` | `game_id` | `validate_game_id()` |
| `save_game_metadata` | `game_id` | `validate_game_id()` |
| `delete_game` | `game_id` | `validate_game_id()` |
| `save_replay` | `duration_secs` | `validate_target_duration()` |
| `set_recording_target` ★ | `summoner_name` | New: length 1-32, alphanumeric + spaces + unicode |
| Video export commands | `output_path` | `validate_video_output_path()` |

★ = already uses `AppResult<T>`. Commands without ★ still use `Result<T, String>` and need migration (see item 4.7).

Note: `delete_clip` in `video/commands.rs:192` already has `validate_video_input_path` and `validate_game_id` validation applied. In `recording/commands.rs`, `set_recording_target`, `start_recording`, `stop_recording`, and `get_recording_status` already use `AppResult`. The remaining ~12 commands in that file still use `Result<T, String>`. The remaining gap is **directory containment check** (verify canonical path is within recordings directory).

**Files:** `src-tauri/src/recording/commands.rs`, `src-tauri/src/storage/commands.rs`, `src-tauri/src/video/commands.rs`

### 1.2 Settings Value Bounds Checking

**Problem:** Settings use enums for most fields (`Resolution`, `FrameRate`, `BitratePreset`, `SampleRate`) which serde already constrains. However, some fields accept raw numbers without bounds: `BitratePreset::Custom(u32)`, `microphone_volume: u8`, `system_audio_volume: u8`, `min_priority: u8`, `auto_delete_days: u32`, `max_storage_gb: u32`.

**Solution:** Add `validate()` method to each settings struct, called after deserialization. Only validate fields that accept raw numeric values:

```
VideoSettings: BitratePreset::Custom(kbps) → 100-50_000
AudioSettings: microphone_volume 0-200, system_audio_volume 0-200
EventSettings: min_priority 1-5
StorageSettings: auto_delete_days 1-365, max_storage_gb 1-10_000
HotkeySettings: parse_hotkey() called on load, invalid → default
```

**Files:** `src-tauri/src/settings/models.rs`

### 1.3 Path Traversal Prevention

**Problem:** `delete_game` and `delete_clip` use `clip.file_path` directly from storage without verifying it's within the recordings directory.

**Solution:** Before any `fs::remove_file()`, verify the canonical path starts with the app's recordings directory. Reject paths containing `..` or pointing outside allowed directories.

**Files:** `src-tauri/src/storage/commands.rs`, `src-tauri/src/recording/commands.rs`

### 1.4 Frontend Form Validation (zod)

**Problem:** Auth forms use only HTML5 validation. No schema validation. Password confirmation not checked.

**Solution:**
- Add `zod` schemas for: LoginForm, SignupForm, YouTubeUploadMetadata, RecordingSettings
- Password confirmation: real-time match validation
- Required field indicators (asterisk + aria-required)
- Validate before API call, show inline errors

**Files:** `src/lib/validation.ts` (new), `src/components/auth/LoginForm.tsx`, `src/components/auth/SignupForm.tsx`, `src/components/youtube/YouTubeUpload.tsx`

### 1.5 OAuth Redirect URI Validation

**Problem:** YouTube/TikTok/Instagram OAuth redirect URIs from env vars not validated at startup. TikTok/Instagram entries are archived obsolete direct-upload scope.

**Solution:** Validate URI format and localhost restriction at manager initialization. Log warning if empty (feature disabled) vs error if malformed.

**Files:** `src-tauri/src/main.rs`, `src-tauri/src/social/tiktok/auth.rs`, `src-tauri/src/social/instagram/auth.rs`

### 1.6 Command Rate Limiting

**Problem:** No rate limiting on Tauri commands. Frontend could spam recording/upload commands.

**Solution:** Simple debounce guard per command group:
- Recording commands: 1 call per 2 seconds
- Upload commands: 1 call per 5 seconds
- Settings save: 1 call per 1 second

Implementation: `HashMap<CommandGroup, Instant>` checked at command entry.

**Files:** `src-tauri/src/utils/rate_limit.rs` (new)

---

## Domain 2: Resource Management & Process Lifecycle (10 items)

### 2.1 FFmpeg Process Pool

**Problem:** No limit on concurrent FFmpeg processes. User could spawn unlimited processes causing OOM.

**Solution:** `FfmpegProcessPool` with tokio Semaphore:
- Max 3 concurrent FFmpeg processes (configurable)
- Queue additional requests
- Drop trait kills all managed processes
- Timeout: 10 minutes per process, then force kill

**Files:** `src-tauri/src/utils/ffmpeg_pool.rs` (new), `src-tauri/src/video/processor/pipeline.rs`, `src-tauri/src/recording/integration_backend/segment_recorder.rs`

### 2.2 Continuous Disk Space Monitoring

**Problem:** Disk space only checked at recording start (2GB minimum). Not monitored during recording.

**Solution:**
- Spawn monitoring task when recording starts
- Check every 60 seconds
- < 1GB: emit warning event to frontend
- < 500MB: auto-stop recording, emit critical event
- Frontend shows toast with space info

**Files:** `src-tauri/src/recording/integration_backend/segment_recorder.rs`, `src-tauri/src/recording/commands.rs`

### ~~2.3 Event Queue Size Limit~~ — VERIFIED: Already Implemented

`auto_clip_manager.rs:294` already has `const MAX_QUEUE_SIZE: usize = 1000` with overflow handling and logging. No action needed.

### 2.4 Mutex/RwLock Timeout

**Problem:** All `.lock().await` calls can wait indefinitely, causing deadlocks.

**Solution:** Wrap critical lock acquisitions with `tokio::time::timeout(Duration::from_secs(5), mutex.lock())`. On timeout, log error and return `AppError::Timeout`.

**Files:** All files using `TokioMutex`/`TokioRwLock` — primarily `auto_clip_manager.rs`, `segment_recorder.rs`, `main.rs`

### ~~2.5 WASAPI Cleanup on Drop~~ — VERIFIED: Already Implemented

`wasapi_audio.rs:125` already has `impl Drop for WasapiCapture` that sets `is_capturing` to false and joins the capture thread. Remaining minor gap: Drop does not explicitly finalize WAV writer (uses `let _ = w.finalize()` which swallows errors). This is LOW priority — no separate task needed.

### 2.6 Frontend Polling Cleanup

**Problem:** Multiple components poll without checking `isMounted` before setState. Causes React warnings and potential memory leaks.

**Solution:** Audit all `setInterval`/polling patterns. Ensure:
- `useEffect` cleanup clears interval
- Check `isMounted` ref before every `setState`
- Components: Dashboard, YouTubeUpload, RecordingControls, ClipLibrary thumbnail generation
- Note: Two ClipLibrary files exist — audit both

**Files:** `src/pages/Dashboard.tsx`, `src/components/youtube/YouTubeUpload.tsx`, `src/components/RecordingControls.tsx`, `src/components/ClipLibrary.tsx`, `src/components/editor/ClipLibrary.tsx`

### 2.7 Atomic File Operations

**Problem:** Settings, concat lists, and metadata written directly without atomicity. Crash mid-write = corruption.

**Solution:** Write to `.tmp` file first, then `fs::rename()` to final path. Apply to:
- Settings JSON save
- Concat list generation
- Clip metadata save

**Files:** `src-tauri/src/settings/mod.rs`, `src-tauri/src/recording/integration_backend/segment_recorder.rs`

### ~~2.8 FFmpeg Process Drop Trait~~ — VERIFIED: Already Implemented

`segment_recorder.rs:418` already has `impl Drop for SegmentRecorder` that stops WASAPI capture and kills FFmpeg process via `start_kill()`. No action needed.

### 2.9 Video Processor Kill-on-Drop

**Problem:** `TokioCommand` spawned FFmpeg in video processor has no kill-on-drop. Cancelled tasks leave FFmpeg running.

**Solution:** Use `tokio::process::Command` with `.kill_on_drop(true)`.

**Files:** `src-tauri/src/video/processor/pipeline.rs`, `src-tauri/src/video/processor/effects.rs`, `src-tauri/src/video/commands.rs`

### 2.10 Circuit Breaker for Live Client Data API

**Problem:** Live Client Data API (port 2999) requests retry indefinitely on failure. No backoff. Note: this is the in-game API, not the LCU API.

**Solution:** Circuit breaker pattern:
- 5 consecutive failures → OPEN state (30s cooldown)
- After cooldown → HALF-OPEN (1 test request)
- Success → CLOSED (normal operation)
- Log state transitions

**Files:** `src-tauri/src/recording/live_client.rs`

---

## Domain 3: Recording Pipeline Robustness (14 items)

### 3.1 Multi-Monitor Support

**Problem:** gdigrab always captures `desktop` (primary display). Users on secondary monitor get wrong content.

**Solution:**
- Enumerate monitors via `EnumDisplayMonitors` Win32 API
- Store monitor list in RecordingSettings with user selection
- Pass selected monitor offset to gdigrab: `-offset_x` / `-offset_y` / `-video_size`
- Default: primary monitor
- Frontend: dropdown in recording settings

**Files:** `src-tauri/src/recording/integration_backend/segment_recorder.rs` (gdigrab command construction), `src-tauri/src/recording/integration_backend/windows_capture.rs` (coordinator — pass monitor config through), `src-tauri/src/settings/models.rs`, `src/components/settings/VideoSettings.tsx`

### 3.2 Window State Verification

**Problem:** No check if game window is visible before starting capture. Minimized window = black recording.

**Solution:** Before gdigrab start:
1. `FindWindowW` for League of Legends window
2. `IsWindowVisible` + `!IsIconic` check
3. If not visible: return error "게임 창이 최소화되어 있습니다"
4. During recording: periodic check (every 30s), warn if minimized

**Files:** `src-tauri/src/recording/integration_backend/windows_capture.rs`

### 3.3 Audio Device Enumeration & Fallback

**Problem:** WASAPI gets default device only. USB headset, Bluetooth, device switching mid-game not handled.

**Solution:**
- Enumerate all WASAPI output devices, expose to frontend settings
- User selects preferred device (default: system default)
- `IMMNotificationClient` callback for device change events
- On device disconnect: attempt switch to new default, log warning
- If all audio fails: continue video-only with user notification

**Files:** `src-tauri/src/recording/wasapi_audio.rs`, `src-tauri/src/settings/models.rs`

### 3.4 Audio-Video Sync Verification

**Problem:** WASAPI and gdigrab capture independently with no timestamp alignment. Drift possible over long recordings.

**Solution:**
- Record WASAPI start timestamp relative to gdigrab start
- FFmpeg mux: use `-itsoffset` for audio alignment
- Post-mux validation: ffprobe check audio/video duration match (tolerance: 100ms)
- If mismatch > 500ms: log error, attempt re-sync

**Files:** `src-tauri/src/recording/integration_backend/segment_recorder.rs`, `src-tauri/src/recording/wasapi_audio.rs`

### 3.5 Sample Rate Validation

**Problem:** WASAPI may capture at 44.1kHz while gdigrab expects 48kHz. Mismatch causes FFmpeg mux failure.

**Solution:** Before recording start:
1. Query WASAPI device sample rate
2. Set FFmpeg audio input to match
3. If mismatch detected: add `-ar 48000` resample in FFmpeg command
4. Log actual sample rates used

**Files:** `src-tauri/src/recording/integration_backend/segment_recorder.rs`

### 3.6 Segment Integrity Verification

**Problem:** No validation that segment .mp4 files are valid. Corrupted segments break concat.

**Solution:**
- After segment write: quick `ffprobe -v error -select_streams v:0 -show_entries stream=codec_type` check
- Invalid segments: remove from concat list, log warning
- Before clip save: verify all segments in concat list are valid

**Files:** `src-tauri/src/recording/integration_backend/segment_recorder.rs`

### 3.7 Hardware Encoder Failure Recovery

**Problem:** If hardware encoder fails mid-encoding, no automatic fallback.

**Solution:**
- Detect FFmpeg exit code != 0 with hardware encoder
- Retry once with software encoder (libx264)
- Log encoder switch
- Notify user: "하드웨어 인코더 오류로 소프트웨어 인코더로 전환되었습니다"

**Files:** `src-tauri/src/video/processor/pipeline.rs`, `src-tauri/src/video/auto_composer/processing.rs`

### 3.8 FFmpeg Crash Recovery

**Problem:** FFmpeg crash during recording = silent failure. No retry.

**Solution:**
- Monitor FFmpeg process exit during recording
- On unexpected exit: 1 automatic restart within 5 seconds
- If restart also fails: stop recording, notify user
- Preserve existing segments (don't cleanup on crash)

**Files:** `src-tauri/src/recording/integration_backend/segment_recorder.rs`

### 3.9 Remake/Surrender Detection

**Problem:** GameEnd treated uniformly. User gets "victory" clip for a 15-minute surrender.

**Solution:**
The Live Client Data API `GameEnd` event does NOT include a `gameResult` field. Instead, infer game result from available data:
- Check `game_time` from `GameData` at GameEnd: < 300s (5 min) → likely remake, < 1200s (20 min) → possible early surrender
- Optionally query LCU API endpoint `/lol-end-of-game/v1/eog-stats-block` post-game for definitive Win/Lose/Remake status (requires separate LCU connection)
- Add `GameResult` enum: `Victory`, `Defeat`, `EarlySurrender`, `Remake`, `Unknown`
- Add to EventTrigger: `GameEnd { result: GameResult, game_duration_secs: f64 }`
- Allow user to filter: "don't clip surrenders before 20 min" in settings

**Files:** `src-tauri/src/recording/live_client.rs`, `src-tauri/src/settings/models.rs`

### 3.10 Steal Detection Tuning

**Problem:** Fixed 10-second window for contested objective detection causes false positives.

**Solution:**
- Make contest window configurable in AdvancedSettings (default: 10s, range: 5-20s)
- Note: kill data retention uses a separate 15s window (line 678) — adjust proportionally if contest window changes
- Add minimum enemy proximity check: at least 2 enemy champions killed/assisted near objective
- Log confidence score with each steal detection

**Files:** `src-tauri/src/recording/live_client.rs`, `src-tauri/src/settings/models.rs`

### 3.11 Spectator Mode Detection

**Problem:** Spectator detection relies on `activePlayer` being empty/spectator. Not reliable across all client versions.

**Solution:** Additional detection:
- Queue ID 1300 = spectator
- `gameFlow/phase` = "WatchInProgress"
- If any signal indicates spectator: disable event clipping

**Files:** `src-tauri/src/recording/live_client.rs`

### 3.12 Video Aspect Ratio Handling

**Problem:** Widescreen (16:9) clips pillarboxed in Shorts format. No automatic crop/pad.

**Solution:**
- Detect clip aspect ratio via ffprobe
- For Shorts (9:16): center-crop from 16:9 with configurable focus area
- For standard: maintain original ratio
- User preference: crop vs pad (black bars)

**Files:** `src-tauri/src/video/processor/pipeline.rs`, `src-tauri/src/video/auto_composer/processing.rs`

### 3.13 FFmpeg Stderr Buffer Safety

**Problem:** Piped stderr with bounded OS buffer (65KB). Verbose FFmpeg output could deadlock.

**Solution:** Current async BufReader line consumption is correct. Add:
- Line count limit (max 10000 lines per process)
- Only log ERROR/WARNING lines, skip INFO/verbose
- Drop reader on process exit

**Files:** `src-tauri/src/recording/integration_backend/segment_recorder.rs`

### 3.14 Event Session Scoping

**Problem:** Event IDs from Live Client API may reset between games. Same event could be processed twice.

**Solution:** Track `session_id` (game start timestamp). Reset `last_event_id` on new session. Ignore events from previous session.

**Files:** `src-tauri/src/recording/live_client.rs`

---

## Domain 4: Error Handling & Graceful Degradation (11 items)

### 4.1 Graceful Startup Failure Handling

**Problem:** `main.rs` contains 9 `process::exit(1)` calls (lines 39, 76, 98, 159, 634, 662, 679, 715, 748) covering: data directory resolution, storage init, recordings directory creation, FFmpeg not found, and social platform manager failures. ALL cause hard crash without user feedback.

**Solution:** Audit all 9 exit points and classify:

**Truly unrecoverable (keep exit but show dialog first):**
- Data directory not found (line 39) — no writable location, can't proceed
- Storage initialization failure (line 76) — database unavailable

**Degradable (remove exit, disable feature):**
- FFmpeg not found (line 159) → set `recording_available = false`, show banner
- Recordings directory creation failure (line 98) → disable recording, allow editor/uploads
- YouTube/TikTok/Instagram manager init failures (lines 634, 662, 679, 715, 748) → disable that platform, log warning, continue. TikTok/Instagram entries are archived obsolete direct-upload scope.

For degradable cases:
- Set feature flags in AppState: `recording_available`, `youtube_available`, `tiktok_available`, `instagram_available`
- Frontend queries flags to disable/enable UI sections
- Show persistent banner for disabled features

**Files:** `src-tauri/src/main.rs`, `src-tauri/src/utils/ffmpeg.rs`

### 4.2 Settings Corruption Recovery

**Problem:** Corrupted settings → silent reset to defaults. User's customizations lost.

**Solution:**
1. Before every save: create `settings.json.bak`
2. On load failure: try `.bak` file first
3. If both fail: use defaults + show toast "설정 파일이 손상되어 기본값으로 복원되었습니다"
4. Log corrupted file content for debugging

**Files:** `src-tauri/src/settings/mod.rs`

### 4.3 Network Failure → Exponential Backoff

**Problem:** Upload failures don't use backoff. Could retry forever.

**Solution:**
- Retry delays: 1s, 2s, 4s, 8s, 16s (max 5 retries)
- After max retries: save to offline queue, notify user
- Resume queue when network detected

**Files:** `src-tauri/src/youtube/commands.rs`, `src-tauri/src/social/tiktok/upload.rs`, `src-tauri/src/social/instagram/upload.rs`

### 4.4 Disk Full → Recording Stop

Cross-reference: Fully covered by item 2.2 (Continuous Disk Space Monitoring). No separate implementation needed.

### 4.5 Background Task Health Check

**Problem:** game_monitor, upload_scheduler background tasks can panic silently.

**Solution:**
- Wrap spawned tasks in `catch_unwind`
- On panic: log error, restart task, increment failure counter
- After 3 restarts: disable feature, notify user
- Health status queryable from frontend

**Files:** `src-tauri/src/main.rs`

### 4.6 Error Type Expansion

**Problem:** Missing error variants for common failure scenarios.

**Solution:** Add to `AppError` enum:
- `OutOfMemory` — allocation/buffer failures
- `ProcessTimeout` — FFmpeg/external process timeouts
- `CorruptedFile(String)` — file integrity failures
- `DeviceDisconnected(String)` — audio/video device removal
- `RateLimited` — command rate limit exceeded
- `ServiceUnavailable(String)` — LCU/upload service down

**Files:** `src-tauri/src/error.rs`

### 4.7 Structured API Errors

**Problem:** The majority of Tauri commands return `Result<T, String>` with unstructured error messages. This includes nearly ALL commands in `storage/commands.rs` (~15 commands) and most in `recording/commands.rs` (~12 commands). Only `video/commands.rs` consistently uses `AppResult<T>`.

**Solution:** Migrate all commands to `AppResult<T>`. Use `video/commands.rs` as the reference pattern. Frontend receives:
```json
{ "code": "DISK_FULL", "message": "디스크 공간이 부족합니다", "details": "500MB 남음" }
```

**Scope:** ~23 command functions need migration (4 in `recording/commands.rs` already use `AppResult`):
- `storage/commands.rs`: ~15 commands (all use `Result<T, String>`)
- `recording/commands.rs`: ~8 remaining commands
- `youtube/commands.rs`: all use `Result<T, String>` with `.map_err(|e| format!(...))` pattern
- `social/tiktok/commands.rs`: mixed patterns

**Files:** `src-tauri/src/storage/commands.rs`, `src-tauri/src/recording/commands.rs`, `src-tauri/src/youtube/commands.rs`, `src-tauri/src/social/tiktok/commands.rs` (all `commands.rs` files for consistency)

### 4.8 Realistic Disk Space Fallback

**Problem:** Falls back to 500GB total, 100GB free if Windows API fails. User thinks they have space when they don't.

**Solution:** Return `DiskInfo { known: false, total: None, free: None }` when API fails. Frontend shows "디스크 공간을 확인할 수 없습니다" instead of fake values.

**Files:** `src-tauri/src/recording/commands.rs`

### 4.9 Audit Logging

**Problem:** No logging for user-facing operations (uploads, deletions, setting changes).

**Solution:** Add structured audit log for:
- Video uploads (platform, status, file size)
- Clip/game deletions (what was deleted)
- Settings changes (old → new values, only changed fields)
- Auth events (login, logout, token refresh)
Write to separate `audit.log` file with daily rotation.

**Files:** `src-tauri/src/utils/audit.rs` (new)

### 4.10 Settings Migration Validation

**Problem:** Settings migration can produce invalid values. No post-migration check.

**Solution:** After any migration/platform optimization:
1. Run `validate()` on resulting settings
2. If invalid: revert to pre-migration values
3. Log migration result (success/failure/partial)

**Files:** `src-tauri/src/settings/mod.rs`

### 4.11 Idempotent Recording Commands

**Problem:** Calling `stop_recording` twice returns error. Should be idempotent.

**Solution:** `start_recording` when already recording → return Ok (no-op). `stop_recording` when not recording → return Ok (no-op). Log warning for duplicate calls.

**Files:** `src-tauri/src/recording/commands.rs`

---

## Domain 5: Frontend Quality & UX (13 items)

### 5.1 Extend Error Boundary Coverage

**Problem:** `ErrorBoundary`, `VideoErrorBoundary`, and `FormErrorBoundary` components already exist in `src/components/ErrorBoundary.tsx` and are used at the App level. However, individual feature panels (Editor, AutoEdit, YouTube, TikTok, Instagram) are NOT individually wrapped, a crash in one panel takes down the entire app. TikTok/Instagram entries are archived obsolete direct-upload scope.

**Solution:** Extend existing ErrorBoundary infrastructure to wrap each major panel:
- Editor panel → `VideoErrorBoundary`
- AutoEdit panel → `ErrorBoundary`
- YouTube upload → `FormErrorBoundary`
- TikTok upload → `FormErrorBoundary`, archived obsolete direct-upload scope
- Instagram upload → `FormErrorBoundary`, archived obsolete direct-upload scope
- EffectsPanel → `ErrorBoundary`

Use existing specialized variants where appropriate. Each shows localized error + retry button.

**Files:** `src/pages/Editor.tsx`, `src/components/youtube/YouTubeUpload.tsx`, `src/components/social/TikTokUpload.tsx`, `src/components/social/InstagramUpload.tsx`, TikTok/Instagram entries archived obsolete direct-upload scope

### 5.2 Modal Focus Management

**Problem:** Modals don't trap focus. Tab navigates behind modal.

**Solution:** Use Radix Dialog's built-in focus trap (already available via shadcn/ui). Ensure:
- `aria-modal="true"` on all dialogs
- Focus moves to first interactive element on open
- Focus returns to trigger on close
- Escape key closes modal

**Files:** `src/components/PaymentModal.tsx`, `src/components/editor/ExportModal.tsx`

### 5.3 Keyboard Navigation

**Problem:** VideoPlayer and Timeline have no keyboard controls.

**Solution:**
- VideoPlayer: Space=play/pause, Left/Right=±5s, Up/Down=volume, M=mute, F=fullscreen
- Timeline: Tab between clips, Enter to select, Delete to remove
- Add `tabIndex={0}` and `onKeyDown` handlers
- Show keyboard shortcut hints in tooltip

**Files:** `src/components/video/VideoPlayer.tsx`, `src/components/editor/Timeline.tsx`

### ~~5.4 Skip-to-Content Link~~ — VERIFIED: Already Implemented

`AppShell.tsx:36-41` already has a skip-to-content link with `sr-only focus:not-sr-only` styling and i18n support (`t('common.skipToContent')`). No action needed.

### 5.5 Empty State Coverage

**Problem:** Editor "no clips" and AutoEdit "no results" show blank areas.

**Solution:** Add EmptyState components:
- Editor: "이 게임의 클립이 없습니다" + "자동 클립 설정으로 이동" action
- AutoEdit: "아직 자동 편집 결과가 없습니다" + "자동 편집 시작" action

**Files:** `src/pages/Editor.tsx`, `src/components/editor/auto-edit/`

### 5.6 Success Feedback

**Problem:** Settings save, recording state changes complete silently.

**Solution:** Add toast notifications:
- Settings save → "설정이 저장되었습니다" (success)
- Recording start → "녹화가 시작되었습니다" (info)
- Recording stop → "녹화가 중지되었습니다" (info)
- Hotkey use → brief visual indicator (1s fade)
- Clip save → "클립이 저장되었습니다" (success) — already exists, verify

**Files:** `src/pages/Settings.tsx`, `src/components/RecordingControls.tsx`

### 5.7 Error Message Improvement

**Problem:** Generic messages like "Error loading games" don't help users.

**Solution:** Extend the existing `src/lib/errorMapper.ts` which already handles Supabase auth errors (`AUTH_ERROR_MAP`, `ERROR_MESSAGE_MAP`, `ERROR_PATTERN_MAP`). Preserve existing mappings and add a new `BACKEND_ERROR_MAP` section for `AppError` codes:
- `DISK_FULL` → "디스크 공간이 부족합니다. 설정 > 저장소에서 정리해주세요."
- `FFMPEG_NOT_FOUND` → "FFmpeg를 찾을 수 없습니다. 앱을 재설치해주세요."
- `NETWORK_ERROR` → "인터넷 연결을 확인해주세요."
- `AUTH_EXPIRED` → "로그인이 만료되었습니다. 다시 로그인해주세요."
- `PROCESS_TIMEOUT` → "작업 시간이 초과되었습니다. 다시 시도해주세요."
- `RATE_LIMITED` → "너무 많은 요청입니다. 잠시 후 다시 시도해주세요."

**Files:** `src/lib/errorMapper.ts` (extend, do NOT recreate)

### 5.8 Hardcoded String Removal

**Problem:** 5 hardcoded English strings found.

**Solution:**
- ExportModal:106 `"No clip source path available"` → `t('editor.export.noClipPath')`
- TimelineClip `"No thumbnail"` → `t('editor.timeline.noThumbnail')`
- loading-state:89 `'Loading...'` → `t('common.loading')`
- App.tsx:25 `"Loading..."` → `t('common.loading')` (Suspense fallback spinner)
- LanguageSelector.tsx:95 `"Loading..."` → `t('common.loading')`

**Files:** `src/App.tsx`, `src/components/editor/ExportModal.tsx`, `src/components/editor/TimelineClip.tsx`, `src/components/ui/loading-state.tsx`, `src/components/settings/LanguageSelector.tsx`, `src/locales/en/translation.json`, `src/locales/ko/translation.json`

### 5.9 Destructive Action Confirmation

**Problem:** No confirmation for subscription cancellation, bulk deletion.

**Solution:** Add confirmation dialog before:
- Account deletion
- Subscription cancellation
- Bulk clip deletion
- Settings reset to defaults

Use existing Dialog component with warning variant.

**Files:** `src/components/PaymentModal.tsx`, `src/pages/Settings.tsx`

### 5.10 Color Contrast Fix

**Problem:** EmptyState uses `text-muted-foreground` which may fail WCAG AA on dark backgrounds.

**Solution:** Audit contrast ratios. Ensure minimum 4.5:1 for normal text, 3:1 for large text. Adjust `muted-foreground` CSS variable if needed.

**Files:** `src/components/ui/empty-state.tsx`, `src/index.css`

### 5.11 ARIA Label Audit

**Problem:** Many interactive elements missing aria-label.

**Solution:** Systematic audit: all buttons, links, inputs, selects must have accessible names via:
- Visible label text
- `aria-label` for icon-only buttons
- `aria-labelledby` for complex widgets

Priority: recording controls, video player, timeline, settings toggles.

**Files:** Multiple component files

### 5.12 Upload Metadata Validation

**Problem:** YouTube upload allows empty title. TikTok/Instagram similar. TikTok/Instagram entries are archived obsolete direct-upload scope.

**Solution:**
- Title: required, 1-100 chars
- Description: max 5000 chars
- Tags: max 500 chars total
- Validate before upload button enables
- Show character count

**Files:** `src/components/youtube/YouTubeUpload.tsx`, `src/components/social/TikTokUpload.tsx`, `src/components/social/InstagramUpload.tsx`, TikTok/Instagram entries archived obsolete direct-upload scope

### 5.13 Long Operation Progress

**Problem:** Video export, encoding show no stage-specific progress.

**Solution:**
- Multi-stage progress bar: "인코딩 중..." → "효과 적용 중..." → "저장 중..."
- Percentage from FFmpeg output (frame count / total frames)
- Estimated time remaining
- Cancel button

**Files:** `src/components/editor/ExportModal.tsx`

---

## Domain 6: Infrastructure & Deployment (11 items)

### 6.1 Code Signing Configuration

**Problem:** `certificateThumbprint: null`, `timestampUrl: ""` in tauri.conf.json.

**Solution:**
- Set `timestampUrl` to `"http://timestamp.digicert.com"`
- Document certificate thumbprint setup in CI/CD vars
- Add signing step to release workflow
- Note: Actual certificate purchase is out of scope for code changes

**Files:** `src-tauri/tauri.conf.json`, `.github/workflows/release.yml`

### 6.2 Auto-Updater Key Generation

**Problem:** Placeholder public key in updater config.

**Solution:**
- Run `setup-updater-keys.ps1` to generate real keypair
- Store private key as GitHub Secret `TAURI_SIGNING_PRIVATE_KEY`
- Set public key in `tauri.conf.json`
- Verify update manifest endpoint returns valid JSON

**Files:** `src-tauri/tauri.conf.json`, `.github/workflows/release.yml`

### 6.3 Sentry DSN from Environment

**Problem:** Hardcoded `"https://placeholder@sentry.io/0"` in both frontend and backend.

**Solution:**
- Backend: read from `SENTRY_DSN` env var, skip init if empty
- Frontend: read from `VITE_SENTRY_DSN`, skip init if empty
- Remove all placeholder strings
- Add to `.env.production.example`

**Files:** `src-tauri/src/main.rs`, `src/main.tsx`, `.env.production.example`

### 6.4 Pre-commit Hooks

**Problem:** No pre-commit hooks. Developers can commit broken code.

**Solution:**
- Install `husky` + `lint-staged`
- Pre-commit: `lint-staged` runs ESLint + Prettier on staged `.ts/.tsx` files
- Pre-push: `cargo clippy` on Rust changes
- Configuration in `package.json`

**Files:** `package.json`, `.husky/pre-commit` (new), `.husky/pre-push` (new)

### 6.5 Environment Variable Validation

**Problem:** Missing/malformed env vars discovered only at runtime.

**Solution:** At app startup:
1. Check required vars exist: `SUPABASE_URL`, `SUPABASE_ANON_KEY`
2. Check format: URLs are valid, keys are non-empty
3. Check optional vars format if present: `YOUTUBE_CLIENT_ID`, `SENTRY_DSN`
4. Missing required → dialog with setup instructions
5. Missing optional → log info, disable feature

**Files:** `src-tauri/src/utils/env_validation.rs` (new), `src-tauri/src/main.rs`

### 6.6 SBOM Generation

**Problem:** No Software Bill of Materials for license compliance.

**Solution:**
- Add npm script: `"sbom": "license-report --output=table > THIRD_PARTY_LICENSES.txt"`
- Add cargo script: `cargo about generate about.hbs > CARGO_LICENSES.txt`
- Include in CI build artifacts
- Bundle in installer

**Files:** `package.json`, `.github/workflows/release.yml`

### 6.7 Settings Backup/Restore

**Problem:** No backup mechanism. Corrupted settings = lost customizations.

**Solution:** Auto-backup mechanism shared with 4.2 (Settings Corruption Recovery). This item adds user-facing export/import on top of that foundation:
- Auto-backup before every save
- Manual export: `export_settings` Tauri command → JSON file via save dialog
- Manual import: `import_settings` Tauri command → validate + apply
- UI: Settings page "백업" / "복원" buttons

**Files:** `src-tauri/src/settings/commands.rs`, `src/pages/Settings.tsx`

### 6.8 User Data Export

**Problem:** No data portability feature (GDPR requirement).

**Solution:** `export_user_data` command:
- Exports: settings, clip metadata, upload history, game records
- Format: ZIP containing JSON files
- Excludes: video files (too large), auth tokens (security)
- UI: Settings > Account > "데이터 내보내기" button

**Files:** `src-tauri/src/utils/data_export.rs` (new), `src/pages/Settings.tsx`

### 6.9 Minimum System Requirements Check

**Problem:** App doesn't verify hardware before enabling recording.

**Solution:** At startup:
- Check available RAM (warn < 4GB)
- Check GPU (warn if no hardware encoder detected)
- Check available disk space (warn < 10GB)
- Show system info in Settings > About
- Don't block app, just warn

**Files:** `src-tauri/src/utils/system_check.rs` (new), `src-tauri/src/main.rs`

### 6.10 Code Coverage in CI

**Problem:** Jest coverage configured but not tracked.

**Solution:**
- Add `--coverage` flag to CI test step
- Upload to Codecov (free for open source)
- Fail CI if coverage drops below threshold (initially 50%, increase over time)

**Files:** `.github/workflows/ci.yml`, `jest.config.js`

### 6.11 Rust Backend Unit Tests

**Problem:** No visible test suite for Rust backend.

**Solution:** Add unit tests for critical modules:
- `settings/models.rs`: validate() bounds checking (10+ cases)
- `utils/security.rs`: path traversal, ID validation (10+ cases)
- `recording/live_client.rs`: event parsing, steal detection logic (15+ cases)
- `utils/rate_limit.rs`: rate limit enforcement (5+ cases)
- `error.rs`: error serialization (5+ cases)

Target: 45+ Rust unit tests.

Note: This project already uses `#[cfg(test)] mod tests` inline convention (45+ existing files). All new tests MUST follow this pattern — do NOT create separate `_test.rs` files.

**Files:** `src-tauri/src/settings/models.rs`, `src-tauri/src/utils/security.rs`, `src-tauri/src/recording/live_client.rs`, `src-tauri/src/utils/rate_limit.rs`, `src-tauri/src/error.rs` (add `#[cfg(test)] mod tests` blocks)

---

## Domain 7: Observability & Monitoring (4 items)

### 7.1 Structured Operation Logging

**Problem:** FFmpeg commands, encoder selection, settings source not logged.

**Solution:**
- Log full FFmpeg command before execution (args joined)
- Log encoder detection result: "Selected encoder: hevc_nvenc (NVIDIA)"
- Log settings load source: "Settings loaded from: platform-optimized"
- Log recording start/stop with config summary

**Files:** `src-tauri/src/video/processor/pipeline.rs`, `src-tauri/src/recording/integration_backend/segment_recorder.rs`, `src-tauri/src/settings/mod.rs`

### 7.2 Recording Quality Metrics

**Problem:** No tracking of dropped frames, actual FPS, bitrate variance.

**Solution:**
- Parse FFmpeg stats output: `frame=`, `fps=`, `bitrate=`, `drop_frames=`
- Store per-recording quality report
- Emit to frontend for display in recording status
- Warn if dropped frames > 5%

**Files:** `src-tauri/src/recording/integration_backend/segment_recorder.rs`

### 7.3 Performance Metrics

**Problem:** No tracking of app startup time, video processing time, upload duration.

**Solution:**
- App startup: log time from process start to window visible
- Video processing: log encode time per clip
- Upload: log upload duration and throughput
- Store in metrics log for debugging

**Files:** `src-tauri/src/main.rs`, `src-tauri/src/video/processor/pipeline.rs`, `src-tauri/src/youtube/commands.rs`

### 7.4 Background Task Status

**Problem:** No visibility into background task health from frontend.

**Solution:**
- `get_system_health` Tauri command returning:
  ```json
  {
    "game_monitor": "running",
    "upload_scheduler": "running",
    "disk_space_monitor": "stopped",
    "ffmpeg_processes": 2,
    "event_queue_size": 15
  }
  ```
- Display in Settings > About or debug panel

**Files:** `src-tauri/src/utils/health.rs` (new), `src/pages/Settings.tsx`

---

## Implementation Notes

### New Files (10)
1. `src/lib/validation.ts` — zod schemas
2. `src-tauri/src/utils/rate_limit.rs` — command rate limiting
3. `src-tauri/src/utils/ffmpeg_pool.rs` — process pool
4. `src-tauri/src/utils/audit.rs` — audit logging
5. `src-tauri/src/utils/env_validation.rs` — env var checking
6. `src-tauri/src/utils/data_export.rs` — user data export
7. `src-tauri/src/utils/system_check.rs` — hardware requirements
8. `src-tauri/src/utils/health.rs` — background task status
9. `.husky/pre-commit` — git hook
10. `.husky/pre-push` — git hook
(Rust tests use inline `#[cfg(test)] mod tests` — no separate test files)

### Modified Files (~35)
Primary targets: `commands.rs` files, `settings/models.rs`, `segment_recorder.rs`, `live_client.rs`, `wasapi_audio.rs`, `pipeline.rs`, `main.rs`, `error.rs`, frontend pages/components, locale files.

### Dependencies
- `zod` (npm) — frontend validation (~13KB minified+gzipped, tree-shakeable)
- `husky` + `lint-staged` (npm dev) — pre-commit hooks
- No new Rust crates required (all functionality implementable with existing deps)

### Implementation Dependency Ordering

Tasks have cross-domain dependencies. Implement in this order:

1. **Foundation layer** (no dependencies):
   - 4.6 Error Type Expansion (new `AppError` variants used by everything)
   - 1.6 Command Rate Limiting (standalone utility)
   - 2.1 FFmpeg Process Pool (standalone utility)
   - 5.8 Hardcoded String Removal (standalone i18n fix)

2. **Core infrastructure** (depends on foundation):
   - 4.7 Structured API Errors (depends on 4.6 error types)
   - 1.2 Settings Value Bounds Checking (standalone but informs 4.10)
   - 1.3 Path Traversal Prevention (standalone)
   - 2.7 Atomic File Operations (standalone)

3. **Feature layer** (depends on core):
   - 1.1 Tauri Command Input Validation (depends on 4.7 for error returns)
   - 2.2 Disk Space Monitoring (depends on 4.6 for error types)
   - 2.10 Circuit Breaker (depends on 4.6)
   - 4.1 Graceful Startup (depends on 4.6, 4.7)
   - 4.2 Settings Corruption Recovery (depends on 2.7 atomic writes)

4. **Recording pipeline** (depends on feature layer):
   - 3.x items (depend on 2.1 FFmpeg pool, 2.2 disk monitoring, 4.6 errors)

5. **Frontend** (depends on backend APIs being stable):
   - 5.x items (depend on 4.7 structured errors, 1.4 zod schemas)
   - 1.4 Frontend Form Validation (depends on zod install)

6. **Infrastructure & testing** (last, validates everything):
   - 6.x items (depend on all code being written)
   - 7.x Observability items (instrument finished code)

### Settings Schema Versioning

When adding new fields to settings structs (e.g., `monitor_index`, `audio_device_id`, `contest_window_secs`), ALL new fields MUST use `#[serde(default)]` to maintain backward compatibility with existing `settings.json` files. Without this, users upgrading from Phase 1 will get deserialization errors on first launch.

### Testing Strategy
- 45+ new Rust unit tests
- Frontend: validate zod schemas compile and reject invalid inputs
- E2E: verify error boundaries render, settings validation works
- Manual: verify recording with monitor selection, audio device switching

---

## Success Criteria

1. All Tauri commands validate inputs before processing
2. All settings values bounded and validated
3. FFmpeg processes managed with pool (max 3 concurrent)
4. Disk space monitored during recording
5. All modals have focus trap and keyboard dismissal
6. All user operations provide success/error feedback
7. No hardcoded strings in UI
8. Pre-commit hooks prevent broken commits
9. 45+ Rust unit tests passing
10. Sentry DSN configurable via environment
11. Settings backup/restore functional
12. System requirements checked and displayed
