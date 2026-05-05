# LoLShorts Commercial Completeness (Phase 2) Implementation Plan

> **Archived / superseded plan:** This historical implementation plan is superseded by the current non-payment commercial readiness plan. Do not treat its 100% commercial readiness target, TikTok upload, or Instagram upload references as current implementation scope. Current scope keeps TikTok/Instagram to preset/export guidance only, keeps payment/Toss deferred, and requires E5 Field QA evidence before commercial or production-readiness claims.

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Historical goal only: close all 64 gaps identified in Phase 2 spec to bring LoLShorts toward 100% commercial readiness. This target is superseded and Field-QA-gated, not a current readiness claim.

**Architecture:** Layered implementation following dependency ordering — foundation utilities first (error types, rate limiting, FFmpeg pool), then core infrastructure (API error migration, settings validation), then feature-level changes (recording pipeline, frontend UX), and finally infrastructure/observability. Each task is independently testable and committable.

**Tech Stack:** Rust (Tauri 2.0 backend), React 18 + TypeScript (frontend), FFmpeg (video processing), zod (validation), husky (git hooks), Jest (testing)

**Spec:** `docs/superpowers/specs/2026-03-14-commercial-completeness-design.md`

---

## File Structure

### New Files (Rust)
| File | Responsibility |
|------|---------------|
| `src-tauri/src/utils/rate_limit.rs` | Command rate limiting with HashMap<CommandGroup, Instant> |
| `src-tauri/src/utils/ffmpeg_pool.rs` | FFmpeg process pool with tokio Semaphore |
| `src-tauri/src/utils/audit.rs` | Structured audit logging with daily rotation |
| `src-tauri/src/utils/env_validation.rs` | Startup environment variable validation |
| `src-tauri/src/utils/data_export.rs` | User data export (GDPR) as ZIP |
| `src-tauri/src/utils/system_check.rs` | Hardware requirements check at startup |
| `src-tauri/src/utils/health.rs` | Background task health status API |

### New Files (Frontend)
| File | Responsibility |
|------|---------------|
| `src/lib/validation.ts` | zod schemas for all forms |

### New Files (Config)
| File | Responsibility |
|------|---------------|
| `.husky/pre-commit` | lint-staged on staged .ts/.tsx |
| `.husky/pre-push` | cargo clippy on Rust changes |

### Key Modified Files
| File | Changes |
|------|---------|
| `src-tauri/src/error.rs` | 6 new AppError variants |
| `src-tauri/src/utils/mod.rs` | Register new modules |
| `src-tauri/src/settings/models.rs` | validate() methods, new settings fields |
| `src-tauri/src/recording/commands.rs` | Input validation, AppResult migration |
| `src-tauri/src/storage/commands.rs` | AppResult migration |
| `src-tauri/src/youtube/commands.rs` | AppResult migration |
| `src-tauri/src/main.rs` | Graceful startup, env validation, health checks |
| `src-tauri/src/recording/live_client.rs` | Circuit breaker, session scoping, event detection |
| `src-tauri/src/recording/integration_backend/segment_recorder.rs` | Disk monitoring, crash recovery, integrity checks |
| `src/lib/errorMapper.ts` | Backend error code mappings |
| `src/locales/en/translation.json` | New i18n keys |
| `src/locales/ko/translation.json` | Korean translations |

---

## Chunk 1: Foundation Layer

### Task 1: Error Type Expansion (Spec 4.6)

**Files:**
- Modify: `src-tauri/src/error.rs`

- [ ] **Step 1: Write failing tests for new error variants**

Add to `src-tauri/src/error.rs` at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_out_of_memory_error_serializes() {
        let err = AppError::OutOfMemory("buffer allocation failed".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "OUT_OF_MEMORY");
        assert_eq!(json["message"], "buffer allocation failed");
    }

    #[test]
    fn test_process_timeout_error_serializes() {
        let err = AppError::ProcessTimeout("FFmpeg exceeded 10min".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "PROCESS_TIMEOUT");
    }

    #[test]
    fn test_corrupted_file_error_serializes() {
        let err = AppError::CorruptedFile("segment_003.mp4".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "CORRUPTED_FILE");
    }

    #[test]
    fn test_device_disconnected_error_serializes() {
        let err = AppError::DeviceDisconnected("USB Headset".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "DEVICE_DISCONNECTED");
    }

    #[test]
    fn test_rate_limited_error_serializes() {
        let err = AppError::RateLimited("recording commands".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "RATE_LIMITED");
    }

    #[test]
    fn test_service_unavailable_error_serializes() {
        let err = AppError::ServiceUnavailable("LCU API".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "SERVICE_UNAVAILABLE");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib error::tests -- --nocapture`
Expected: FAIL — variants don't exist yet

- [ ] **Step 3: Add new variants to AppError enum**

In `src-tauri/src/error.rs`, add to the `AppError` enum:

```rust
    #[error("Out of memory: {0}")]
    OutOfMemory(String),

    #[error("Process timeout: {0}")]
    ProcessTimeout(String),

    #[error("Corrupted file: {0}")]
    CorruptedFile(String),

    #[error("Device disconnected: {0}")]
    DeviceDisconnected(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
```

Update the `Serialize` impl match arms:

```rust
    AppError::OutOfMemory(msg) => ("OUT_OF_MEMORY", msg.clone()),
    AppError::ProcessTimeout(msg) => ("PROCESS_TIMEOUT", msg.clone()),
    AppError::CorruptedFile(msg) => ("CORRUPTED_FILE", msg.clone()),
    AppError::DeviceDisconnected(msg) => ("DEVICE_DISCONNECTED", msg.clone()),
    AppError::RateLimited(msg) => ("RATE_LIMITED", msg.clone()),
    AppError::ServiceUnavailable(msg) => ("SERVICE_UNAVAILABLE", msg.clone()),
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib error::tests -- --nocapture`
Expected: 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/error.rs
git commit -m "feat(error): add 6 new AppError variants for commercial completeness"
```

---

### Task 2: Command Rate Limiting (Spec 1.6)

**Files:**
- Create: `src-tauri/src/utils/rate_limit.rs`
- Modify: `src-tauri/src/utils/mod.rs`

- [ ] **Step 1: Create rate_limit.rs with tests**

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum CommandGroup {
    Recording,
    Upload,
    Settings,
}

pub struct RateLimiter {
    last_calls: Mutex<HashMap<CommandGroup, Instant>>,
    limits: HashMap<CommandGroup, Duration>,
}

impl RateLimiter {
    pub fn new() -> Self {
        let mut limits = HashMap::new();
        limits.insert(CommandGroup::Recording, Duration::from_secs(2));
        limits.insert(CommandGroup::Upload, Duration::from_secs(5));
        limits.insert(CommandGroup::Settings, Duration::from_secs(1));
        Self {
            last_calls: Mutex::new(HashMap::new()),
            limits,
        }
    }

    pub fn check(&self, group: CommandGroup) -> Result<(), Duration> {
        let mut last_calls = self.last_calls.lock().unwrap();
        let limit = self.limits.get(&group).copied().unwrap_or(Duration::from_secs(1));
        if let Some(last) = last_calls.get(&group) {
            let elapsed = last.elapsed();
            if elapsed < limit {
                return Err(limit - elapsed);
            }
        }
        last_calls.insert(group, Instant::now());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_first_call_always_passes() {
        let limiter = RateLimiter::new();
        assert!(limiter.check(CommandGroup::Recording).is_ok());
    }

    #[test]
    fn test_rapid_second_call_rejected() {
        let limiter = RateLimiter::new();
        limiter.check(CommandGroup::Recording).unwrap();
        assert!(limiter.check(CommandGroup::Recording).is_err());
    }

    #[test]
    fn test_different_groups_independent() {
        let limiter = RateLimiter::new();
        limiter.check(CommandGroup::Recording).unwrap();
        assert!(limiter.check(CommandGroup::Upload).is_ok());
    }

    #[test]
    fn test_call_after_cooldown_passes() {
        let limiter = RateLimiter::new();
        limiter.check(CommandGroup::Settings).unwrap();
        thread::sleep(Duration::from_millis(1100));
        assert!(limiter.check(CommandGroup::Settings).is_ok());
    }

    #[test]
    fn test_returns_remaining_wait_time() {
        let limiter = RateLimiter::new();
        limiter.check(CommandGroup::Upload).unwrap();
        if let Err(remaining) = limiter.check(CommandGroup::Upload) {
            assert!(remaining.as_secs() <= 5);
            assert!(remaining.as_millis() > 0);
        } else {
            panic!("Should have been rate limited");
        }
    }
}
```

- [ ] **Step 2: Register module in mod.rs**

Add `pub mod rate_limit;` to `src-tauri/src/utils/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib utils::rate_limit::tests -- --nocapture`
Expected: 5 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/utils/rate_limit.rs src-tauri/src/utils/mod.rs
git commit -m "feat(utils): add command rate limiter with per-group cooldowns"
```

---

### Task 3: FFmpeg Process Pool (Spec 2.1)

**Files:**
- Create: `src-tauri/src/utils/ffmpeg_pool.rs`
- Modify: `src-tauri/src/utils/mod.rs`

- [ ] **Step 1: Create ffmpeg_pool.rs with tests**

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::process::Child;
use tokio::sync::Mutex;
use tracing::{info, warn, error};

const DEFAULT_MAX_CONCURRENT: usize = 3;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600); // 10 minutes

pub struct FfmpegPool {
    semaphore: Arc<Semaphore>,
    active_processes: Arc<Mutex<Vec<u32>>>, // PIDs
    max_concurrent: usize,
}

impl FfmpegPool {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            active_processes: Arc::new(Mutex::new(Vec::new())),
            max_concurrent,
        }
    }

}

impl Default for FfmpegPool {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CONCURRENT)
    }

    pub async fn acquire(&self) -> Result<FfmpegPermit, crate::error::AppError> {
        let permit = tokio::time::timeout(
            Duration::from_secs(30),
            self.semaphore.clone().acquire_owned(),
        )
        .await
        .map_err(|_| crate::error::AppError::ProcessTimeout(
            format!("FFmpeg pool full ({} concurrent). Timed out waiting for slot.", self.max_concurrent)
        ))?
        .map_err(|_| crate::error::AppError::Internal("Semaphore closed".into()))?;

        Ok(FfmpegPermit {
            _permit: permit,
            active_processes: self.active_processes.clone(),
            pid: None,
        })
    }

    pub async fn active_count(&self) -> usize {
        self.active_processes.lock().await.len()
    }

    pub async fn kill_all(&self) {
        let pids = self.active_processes.lock().await.clone();
        for pid in pids {
            #[cfg(windows)]
            {
                let _ = tokio::process::Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .output()
                    .await;
            }
            warn!(pid = pid, "Force-killed FFmpeg process from pool");
        }
        self.active_processes.lock().await.clear();
    }
}

impl Drop for FfmpegPool {
    fn drop(&mut self) {
        // Best-effort sync kill
        let pids: Vec<u32> = self.active_processes.try_lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        for pid in pids {
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .output();
            }
        }
    }
}

pub struct FfmpegPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    active_processes: Arc<Mutex<Vec<u32>>>,
    pid: Option<u32>,
}

impl FfmpegPermit {
    pub async fn register_process(&mut self, child: &Child) {
        if let Some(pid) = child.id() {
            self.pid = Some(pid);
            self.active_processes.lock().await.push(pid);
            info!(pid = pid, "Registered FFmpeg process in pool");
        }
    }
}

impl Drop for FfmpegPermit {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            if let Ok(mut procs) = self.active_processes.try_lock() {
                procs.retain(|&p| p != pid);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_allows_up_to_max() {
        let pool = FfmpegPool::new(2);
        let _p1 = pool.acquire().await.unwrap();
        let _p2 = pool.acquire().await.unwrap();
        assert_eq!(pool.semaphore.available_permits(), 0);
    }

    #[tokio::test]
    async fn test_pool_releases_on_drop() {
        let pool = FfmpegPool::new(1);
        {
            let _p = pool.acquire().await.unwrap();
            assert_eq!(pool.semaphore.available_permits(), 0);
        }
        assert_eq!(pool.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn test_active_count() {
        let pool = FfmpegPool::new(3);
        assert_eq!(pool.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_default_pool_has_3_slots() {
        let pool = FfmpegPool::default();
        assert_eq!(pool.max_concurrent, 3);
    }
}
```

- [ ] **Step 2: Register module**

Add `pub mod ffmpeg_pool;` to `src-tauri/src/utils/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib utils::ffmpeg_pool::tests -- --nocapture`
Expected: 4 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/utils/ffmpeg_pool.rs src-tauri/src/utils/mod.rs
git commit -m "feat(utils): add FFmpeg process pool with semaphore-based concurrency limit"
```

---

### Task 4: Hardcoded String Removal (Spec 5.8)

**Files:**
- Modify: `src/App.tsx:25`
- Modify: `src/components/editor/ExportModal.tsx:106`
- Modify: `src/components/editor/TimelineClip.tsx` (find "No thumbnail")
- Modify: `src/components/ui/loading-state.tsx:89`
- Modify: `src/components/settings/LanguageSelector.tsx:95`
- Modify: `src/locales/en/translation.json`
- Modify: `src/locales/ko/translation.json`

- [ ] **Step 1: Add i18n keys to English locale**

Add to `src/locales/en/translation.json` under `common`:
```json
"loading": "Loading..."
```

Add to `editor.export`:
```json
"noClipSourcePath": "No clip source path available"
```

Add to `editor.timeline`:
```json
"noThumbnail": "No thumbnail"
```

- [ ] **Step 2: Add Korean translations**

Add to `src/locales/ko/translation.json`:
```json
"loading": "로딩 중..."
"noClipSourcePath": "클립 소스 경로를 찾을 수 없습니다"
"noThumbnail": "썸네일 없음"
```

- [ ] **Step 3: Replace hardcoded strings**

In `src/App.tsx:25`:
```tsx
// Before: <p className="mt-2 text-sm text-muted-foreground">Loading...</p>
// After:  Need to use useTranslation in LoadingSpinner or keep as simple non-translated fallback
// Note: LoadingSpinner is outside React i18n provider, so use a static Korean string or keep English
// Best approach: Since this is a Suspense fallback before i18n loads, keep as visual spinner only
```

Actually for App.tsx, remove the text entirely since it's a pre-i18n Suspense fallback — just show the spinner animation. For the other files, use `t('common.loading')`.

In `src/components/editor/ExportModal.tsx:106`:
```tsx
// Before: setExportError('No clip source path available');
// After:  setExportError(t('editor.export.noClipSourcePath'));
```

In `src/components/editor/TimelineClip.tsx`:
```tsx
// Before: "No thumbnail"
// After:  t('editor.timeline.noThumbnail')
```

In `src/components/ui/loading-state.tsx:89`:
```tsx
// Before: 'Loading...'
// After:  loadingLabel ?? t('common.loading')
```

In `src/components/settings/LanguageSelector.tsx:95`:
```tsx
// Before: 'Loading...'
// After:  t('common.loading')
```

- [ ] **Step 4: Add all matching keys to remaining 18 locale files**

For each locale in `src/locales/{cs,de,el,es,fil,fr,hu,it,ja,pl,pt-BR,ro,ru,th,tr,vi,zh-CN,zh-TW}/translation.json`, add the same keys. Use English as fallback — native speakers can update later.

- [ ] **Step 5: Run frontend tests**

Run: `npx jest --no-coverage`
Expected: All existing tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx src/components/editor/ExportModal.tsx src/components/editor/TimelineClip.tsx \
  src/components/ui/loading-state.tsx src/components/settings/LanguageSelector.tsx \
  src/locales/
git commit -m "fix(i18n): replace 5 hardcoded English strings with translation keys"
```

---

## Chunk 2: Core Infrastructure

### Task 5: Structured API Errors Migration (Spec 4.7)

**Files:**
- Modify: `src-tauri/src/storage/commands.rs` (~15 commands)
- Modify: `src-tauri/src/recording/commands.rs` (~8 remaining commands)
- Modify: `src-tauri/src/youtube/commands.rs`
- Modify: `src-tauri/src/social/tiktok/commands.rs`

- [ ] **Step 1: Write test for error format consistency**

Add to `src-tauri/src/error.rs` tests:

```rust
#[test]
fn test_all_variants_have_code_and_message() {
    let variants: Vec<AppError> = vec![
        AppError::Database("test".into()),
        AppError::Network("test".into()),
        AppError::Io("test".into()),
        AppError::Video("test".into()),
        AppError::Auth("test".into()),
        AppError::Validation("test".into()),
        AppError::NotFound("test".into()),
        AppError::Recording("test".into()),
        AppError::Internal("test".into()),
        AppError::Lcu("test".into()),
        AppError::OutOfMemory("test".into()),
        AppError::ProcessTimeout("test".into()),
        AppError::CorruptedFile("test".into()),
        AppError::DeviceDisconnected("test".into()),
        AppError::RateLimited("test".into()),
        AppError::ServiceUnavailable("test".into()),
    ];
    for err in variants {
        let json = serde_json::to_value(&err).unwrap();
        assert!(json["code"].is_string(), "Missing code for {:?}", err);
        assert!(json["message"].is_string(), "Missing message for {:?}", err);
    }
}
```

- [ ] **Step 2: Migrate storage/commands.rs**

Pattern for each command:
```rust
// Before:
pub async fn some_command(...) -> Result<T, String> {
    something.map_err(|e| format!("Error: {}", e))?
}

// After:
pub async fn some_command(...) -> AppResult<T> {
    something.map_err(|e| AppError::Database(e.to_string()))?
}
```

Add import: `use crate::error::{AppError, AppResult};`

- [ ] **Step 3: Migrate recording/commands.rs remaining commands**

Same pattern. ~8 commands still using `Result<T, String>`.

- [ ] **Step 4: Migrate youtube/commands.rs**

Replace `.map_err(|e| format!(...))` with `.map_err(|e| AppError::Network(e.to_string()))` for network operations, `AppError::Auth(...)` for auth operations.

- [ ] **Step 5: Migrate social/tiktok/commands.rs**

Same pattern as YouTube.

- [ ] **Step 6: Run cargo check**

Run: `cd src-tauri && cargo check`
Expected: No errors

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/storage/commands.rs src-tauri/src/recording/commands.rs \
  src-tauri/src/youtube/commands.rs src-tauri/src/social/tiktok/commands.rs \
  src-tauri/src/error.rs
git commit -m "refactor(commands): migrate all Tauri commands to AppResult<T>"
```

---

### Task 6: Settings Value Bounds Checking (Spec 1.2)

**Files:**
- Modify: `src-tauri/src/settings/models.rs`

- [ ] **Step 1: Write failing tests**

Add to `src-tauri/src/settings/models.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_audio_volume_in_range() {
        let mut s = AudioSettings::default();
        s.microphone_volume = 150;
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
}
```

- [ ] **Step 2: Run tests — should fail (no validate methods)**

Run: `cd src-tauri && cargo test --lib settings::models::tests -- --nocapture`

- [ ] **Step 3: Add validate() to each settings struct**

```rust
impl VideoSettings {
    pub fn validate(&self) -> Result<(), String> {
        if let BitratePreset::Custom(kbps) = &self.bitrate_preset {
            if *kbps < 100 || *kbps > 50_000 {
                return Err(format!("Custom bitrate {} out of range 100-50000", kbps));
            }
        }
        Ok(())
    }
}

impl AudioSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.microphone_volume > 200 {
            return Err(format!("Microphone volume {} exceeds max 200", self.microphone_volume));
        }
        if self.system_audio_volume > 200 {
            return Err(format!("System audio volume {} exceeds max 200", self.system_audio_volume));
        }
        Ok(())
    }
}

impl StorageSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.auto_delete_days == 0 || self.auto_delete_days > 365 {
            return Err(format!("auto_delete_days {} out of range 1-365", self.auto_delete_days));
        }
        if self.max_storage_gb == 0 || self.max_storage_gb > 10_000 {
            return Err(format!("max_storage_gb {} out of range 1-10000", self.max_storage_gb));
        }
        Ok(())
    }
}

impl EventFilterSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.min_priority < 1 || self.min_priority > 5 {
            return Err(format!("min_priority {} out of range 1-5", self.min_priority));
        }
        Ok(())
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
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib settings::models::tests -- --nocapture`
Expected: 7 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings/models.rs
git commit -m "feat(settings): add bounds validation for numeric settings fields"
```

---

### Task 7: Path Traversal Prevention (Spec 1.3)

**Files:**
- Modify: `src-tauri/src/utils/security.rs`
- Modify: `src-tauri/src/storage/commands.rs`

- [ ] **Step 1: Write test for directory containment check**

Add to `src-tauri/src/utils/security.rs` tests:

```rust
#[test]
fn test_path_within_directory_passes() {
    let base = std::path::PathBuf::from("C:\\Users\\test\\recordings");
    let target = std::path::PathBuf::from("C:\\Users\\test\\recordings\\clip001.mp4");
    assert!(validate_path_within_directory(&target, &base).is_ok());
}

#[test]
fn test_path_traversal_rejected() {
    let base = std::path::PathBuf::from("C:\\Users\\test\\recordings");
    let target = std::path::PathBuf::from("C:\\Users\\test\\recordings\\..\\passwords.txt");
    assert!(validate_path_within_directory(&target, &base).is_err());
}

#[test]
fn test_path_outside_directory_rejected() {
    let base = std::path::PathBuf::from("C:\\Users\\test\\recordings");
    let target = std::path::PathBuf::from("C:\\Windows\\System32\\config.sys");
    assert!(validate_path_within_directory(&target, &base).is_err());
}
```

- [ ] **Step 2: Implement validate_path_within_directory**

```rust
pub fn validate_path_within_directory(
    path: &std::path::Path,
    allowed_dir: &std::path::Path,
) -> Result<std::path::PathBuf> {
    let canonical = path.canonicalize().map_err(|_| SecurityError::PathNotFound {
        path: path.display().to_string(),
    })?;
    let canonical_dir = allowed_dir.canonicalize().map_err(|_| SecurityError::PathNotFound {
        path: allowed_dir.display().to_string(),
    })?;
    if !canonical.starts_with(&canonical_dir) {
        return Err(SecurityError::PathTraversal {
            path: path.display().to_string(),
        });
    }
    Ok(canonical)
}
```

- [ ] **Step 3: Apply to delete_game in storage/commands.rs**

Before `fs::remove_file`, call:
```rust
let recordings_dir = state.recordings_dir.read().await;
security::validate_path_within_directory(&clip_path, &recordings_dir)
    .map_err(|e| AppError::Validation(e.to_string()))?;
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib utils::security::tests -- --nocapture`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/utils/security.rs src-tauri/src/storage/commands.rs
git commit -m "feat(security): add directory containment check for file operations"
```

---

### Task 8: Atomic File Operations (Spec 2.7)

**Files:**
- Modify: `src-tauri/src/settings/mod.rs`

- [ ] **Step 1: Write test for atomic write**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_atomic_write_creates_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        atomic_write(&path, b"{}").unwrap();
        assert!(path.exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    }

    #[test]
    fn test_atomic_write_no_tmp_file_remains() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        atomic_write(&path, b"{}").unwrap();
        let tmp_path = path.with_extension("json.tmp");
        assert!(!tmp_path.exists());
    }
}
```

- [ ] **Step 2: Implement atomic_write function**

```rust
pub fn atomic_write(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp_path = path.with_extension(
        format!("{}.tmp", path.extension().unwrap_or_default().to_string_lossy())
    );
    std::fs::write(&tmp_path, data)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}
```

- [ ] **Step 3: Replace direct writes in settings save**

In the settings save function, replace `std::fs::write(path, json_bytes)` with `atomic_write(path, json_bytes)`.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib settings -- --nocapture`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings/mod.rs
git commit -m "feat(settings): use atomic write (tmp+rename) for settings persistence"
```

---

## Chunk 3: Security & Startup

### Task 9: Tauri Command Input Validation (Spec 1.1)

**Files:**
- Modify: `src-tauri/src/recording/commands.rs`
- Modify: `src-tauri/src/storage/commands.rs`
- Modify: `src-tauri/src/video/commands.rs`

- [ ] **Step 1: Add validation calls to each command per spec table**

For `get_game_metadata(game_id)`:
```rust
security::validate_game_id(&game_id).map_err(|e| AppError::Validation(e.to_string()))?;
```

For `save_replay(duration_secs)`:
```rust
security::validate_target_duration(duration_secs).map_err(|e| AppError::Validation(e.to_string()))?;
```

For `set_recording_target(summoner_name)`:
```rust
if summoner_name.is_empty() || summoner_name.len() > 32 {
    return Err(AppError::Validation("Summoner name must be 1-32 characters".into()));
}
```

For video export commands with `output_path`:
```rust
security::validate_video_output_path(&output_path).map_err(|e| AppError::Validation(e.to_string()))?;
```

- [ ] **Step 2: Run cargo check**

Run: `cd src-tauri && cargo check`

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/recording/commands.rs src-tauri/src/storage/commands.rs src-tauri/src/video/commands.rs
git commit -m "feat(security): apply input validation to all Tauri commands"
```

---

### Task 10: OAuth Redirect URI Validation (Spec 1.5)

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add URI validation at manager initialization**

Before YouTube/TikTok/Instagram manager creation in `main.rs`, add. TikTok/Instagram manager references are archived obsolete direct-upload scope:
```rust
fn validate_redirect_uri(uri: &str, platform: &str) -> bool {
    if uri.is_empty() {
        tracing::info!("{} OAuth disabled (no redirect URI)", platform);
        return false;
    }
    if !uri.starts_with("http://localhost") && !uri.starts_with("http://127.0.0.1") {
        tracing::error!("{} redirect URI must be localhost: {}", platform, uri);
        return false;
    }
    true
}
```

- [ ] **Step 2: Apply to each platform init**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(security): validate OAuth redirect URIs at startup"
```

---

### Task 11: Graceful Startup Failure Handling (Spec 4.1)

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add feature availability flags to AppState**

```rust
pub struct FeatureFlags {
    pub recording_available: bool,
    pub youtube_available: bool,
    pub tiktok_available: bool,
    pub instagram_available: bool,
}
```

- [ ] **Step 2: Replace process::exit for degradable failures**

For FFmpeg not found (line 159):
```rust
// Before: process::exit(1)
// After:
tracing::warn!("FFmpeg not found - recording disabled");
feature_flags.recording_available = false;
```

For social platform failures (lines 634, 662, 679, 715, 748):
```rust
// Before: process::exit(1)
// After:
tracing::warn!("{} init failed: {} - platform disabled", platform, e);
feature_flags.youtube_available = false; // etc.
```

- [ ] **Step 3: Keep process::exit for truly unrecoverable (lines 39, 76) but log clearly**

For fatal errors where no window exists yet, use `eprintln!` (consistent with current pattern):
```rust
// Before: process::exit(1)
// After:
eprintln!("[FATAL] {}", error_msg);
tracing::error!("Unrecoverable startup failure: {}", error_msg);
process::exit(1);
```

Note: Native dialog (e.g. `rfd` crate) is NOT available — the project uses `tauri_plugin_dialog` which requires an active app handle. Since these failures occur before window creation, `eprintln!` + `tracing::error!` is the correct approach.

- [ ] **Step 4: Add get_feature_flags Tauri command**

```rust
#[tauri::command]
pub fn get_feature_flags(state: State<'_, AppState>) -> AppResult<FeatureFlags> {
    Ok(state.feature_flags.clone())
}
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(startup): graceful degradation for non-critical init failures"
```

---

### Task 12: Settings Corruption Recovery (Spec 4.2)

**Files:**
- Modify: `src-tauri/src/settings/mod.rs`

- [ ] **Step 1: Add backup-before-save logic**

```rust
pub fn save_settings(path: &Path, settings: &RecordingSettings) -> std::io::Result<()> {
    // Backup current file
    if path.exists() {
        let backup = path.with_extension("json.bak");
        std::fs::copy(path, &backup)?;
    }
    // Atomic write
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    atomic_write(path, json.as_bytes())
}
```

- [ ] **Step 2: Add fallback-to-backup on load failure**

```rust
pub fn load_settings(path: &Path) -> RecordingSettings {
    match std::fs::read_to_string(path).and_then(|s| {
        serde_json::from_str(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }) {
        Ok(settings) => settings,
        Err(e) => {
            tracing::warn!("Settings load failed: {}. Trying backup.", e);
            let backup = path.with_extension("json.bak");
            match std::fs::read_to_string(&backup).and_then(|s| {
                serde_json::from_str(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            }) {
                Ok(settings) => {
                    tracing::info!("Restored settings from backup");
                    settings
                }
                Err(_) => {
                    tracing::warn!("Backup also failed. Using defaults.");
                    RecordingSettings::default()
                }
            }
        }
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/settings/mod.rs
git commit -m "feat(settings): backup before save, fallback to .bak on corruption"
```

---

### Task 13: Settings Migration Validation (Spec 4.10)

**Files:**
- Modify: `src-tauri/src/settings/mod.rs`

- [ ] **Step 1: Call validate() after every load/migration**

After `load_settings` returns, call:
```rust
let settings = load_settings(path);
if let Err(e) = settings.validate() {
    tracing::error!("Settings validation failed after load: {}. Using defaults.", e);
    return RecordingSettings::default();
}
settings
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/settings/mod.rs
git commit -m "feat(settings): validate after load/migration, revert to defaults if invalid"
```

---

### Task 14: Idempotent Recording Commands (Spec 4.11)

**Files:**
- Modify: `src-tauri/src/recording/commands.rs`

- [ ] **Step 1: Make start/stop idempotent**

```rust
#[tauri::command]
pub async fn start_recording(state: State<'_, AppState>) -> AppResult<()> {
    let manager = state.recording_manager.read().await;
    if manager.is_recording() {
        tracing::warn!("start_recording called while already recording — no-op");
        return Ok(());
    }
    drop(manager);
    state.recording_manager.write().await.start_recording().await
        .map_err(|e| AppError::Recording(e.to_string()))
}

#[tauri::command]
pub async fn stop_recording(state: State<'_, AppState>) -> AppResult<()> {
    let manager = state.recording_manager.read().await;
    if !manager.is_recording() {
        tracing::warn!("stop_recording called while not recording — no-op");
        return Ok(());
    }
    drop(manager);
    state.recording_manager.write().await.stop_recording().await
        .map_err(|e| AppError::Recording(e.to_string()))
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/recording/commands.rs
git commit -m "feat(recording): make start/stop commands idempotent"
```

---

## Chunk 4: Resource Management

### Task 15: Continuous Disk Space Monitoring (Spec 2.2)

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Add disk monitor task spawned on recording start**

```rust
Note: Do NOT use `fs2` crate — the project already has `get_free_disk_space()` in `recording/commands.rs` (lines 549, 621) using Windows APIs directly. Extract and reuse that function.

```rust
async fn disk_monitor_task(app_handle: tauri::AppHandle, recordings_dir: PathBuf) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        if let Some(space) = get_free_disk_space(&recordings_dir) {
            let gb = space / (1024 * 1024 * 1024);
            let mb = space / (1024 * 1024);
            if mb < 500 {
                tracing::error!("Disk critically low: {}MB", mb);
                let _ = app_handle.emit("disk-critical", mb);
                break; // Signal recording stop
            } else if gb < 1 {
                tracing::warn!("Disk space low: {}MB", mb);
                let _ = app_handle.emit("disk-warning", mb);
            }
        }
    }
}
```

- [ ] **Step 2: Spawn on recording start, abort on stop**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/recording/integration_backend/segment_recorder.rs
git commit -m "feat(recording): continuous disk space monitoring during recording"
```

---

### Task 16: Mutex/RwLock Timeout (Spec 2.4)

**Files:**
- Modify: `src-tauri/src/recording/auto_clip_manager.rs`
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Wrap critical lock acquisitions with timeout**

```rust
// Before:
let guard = self.state.lock().await;

// After:
let guard = tokio::time::timeout(Duration::from_secs(5), self.state.lock())
    .await
    .map_err(|_| AppError::ProcessTimeout("Lock acquisition timed out".into()))?;
```

- [ ] **Step 2: Apply to all critical paths in auto_clip_manager.rs and segment_recorder.rs**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/recording/auto_clip_manager.rs \
  src-tauri/src/recording/integration_backend/segment_recorder.rs
git commit -m "feat(robustness): add 5s timeout to critical mutex/rwlock acquisitions"
```

---

### Task 17: Video Processor Kill-on-Drop (Spec 2.9)

**Files:**
- Modify: `src-tauri/src/video/processor/pipeline.rs`
- Modify: `src-tauri/src/video/processor/effects.rs`
- Modify: `src-tauri/src/video/commands.rs`

- [ ] **Step 1: Add .kill_on_drop(true) to all TokioCommand spawns**

```rust
// Before:
let child = command.spawn()?;

// After:
let child = command.kill_on_drop(true).spawn()?;
```

- [ ] **Step 2: Apply to all ~12 TokioCommand instances across 3 files**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/video/processor/pipeline.rs \
  src-tauri/src/video/processor/effects.rs src-tauri/src/video/commands.rs
git commit -m "fix(video): add kill_on_drop to all FFmpeg TokioCommand spawns"
```

---

### Task 18: Circuit Breaker for Live Client API (Spec 2.10)

**Files:**
- Modify: `src-tauri/src/recording/live_client.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod circuit_breaker_tests {
    use super::*;

    #[test]
    fn test_circuit_starts_closed() {
        let cb = CircuitBreaker::new(5, Duration::from_secs(30));
        assert!(cb.is_closed());
    }

    #[test]
    fn test_circuit_opens_after_threshold() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        for _ in 0..3 {
            cb.record_failure();
        }
        assert!(cb.is_open());
    }

    #[test]
    fn test_circuit_allows_after_cooldown() {
        let cb = CircuitBreaker::new(3, Duration::from_millis(100));
        for _ in 0..3 { cb.record_failure(); }
        std::thread::sleep(Duration::from_millis(150));
        assert!(cb.should_allow_request());
    }

    #[test]
    fn test_success_resets_counter() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(30));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert!(cb.is_closed());
    }
}
```

- [ ] **Step 2: Implement CircuitBreaker struct**

```rust
use std::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct CircuitBreaker {
    failure_count: AtomicU32,
    threshold: u32,
    cooldown: Duration,
    is_open: AtomicBool,
    last_failure: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, cooldown: Duration) -> Self { /* ... */ }
    pub fn is_closed(&self) -> bool { !self.is_open.load(Ordering::Relaxed) }
    pub fn is_open(&self) -> bool { self.is_open.load(Ordering::Relaxed) }

    pub fn should_allow_request(&self) -> bool {
        if self.is_closed() { return true; }
        // Check if cooldown elapsed
        if let Ok(last) = self.last_failure.lock() {
            if let Some(t) = *last {
                if t.elapsed() >= self.cooldown {
                    return true; // HALF-OPEN
                }
            }
        }
        false
    }

    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.threshold {
            self.is_open.store(true, Ordering::Relaxed);
            *self.last_failure.lock().unwrap() = Some(Instant::now());
            tracing::warn!("Circuit breaker OPEN after {} failures", count);
        }
    }

    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.is_open.store(false, Ordering::Relaxed);
    }
}
```

- [ ] **Step 3: Integrate into Live Client API polling loop**

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib recording::live_client::circuit_breaker_tests`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/recording/live_client.rs
git commit -m "feat(live-client): add circuit breaker pattern for API resilience"
```

---

### Task 19: Network Failure Exponential Backoff (Spec 4.3)

**Files:**
- Modify: `src-tauri/src/youtube/commands.rs`
- Modify: `src-tauri/src/social/tiktok/upload.rs`
- Modify: `src-tauri/src/social/instagram/upload.rs`

- [ ] **Step 1: Implement retry with backoff**

```rust
async fn retry_with_backoff<F, Fut, T, E>(
    max_retries: u32,
    operation: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < max_retries => {
                let delay = Duration::from_secs(1 << attempt); // 1, 2, 4, 8, 16
                tracing::warn!("Retry {}/{} after {:?}", attempt + 1, max_retries, delay);
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

- [ ] **Step 2: Apply to YouTube, TikTok, and Instagram upload commands** — TikTok/Instagram upload commands are archived obsolete direct-upload scope, not current implementation scope.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/youtube/commands.rs \
  src-tauri/src/social/tiktok/upload.rs \
  src-tauri/src/social/instagram/upload.rs
git commit -m "feat(upload): add exponential backoff retry for network failures"
```

---

### Task 20: Background Task Health Check (Spec 4.5)

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Wrap spawned tasks in catch_unwind + restart logic**

- [ ] **Step 2: Add failure counter and max restart limit (3)**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(health): catch panics in background tasks with auto-restart"
```

---

## Chunk 5: Recording Pipeline Part 1

### Task 21: Multi-Monitor Support (Spec 3.1)

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`
- Modify: `src-tauri/src/recording/integration_backend/windows_capture.rs`
- Modify: `src-tauri/src/settings/models.rs`
- Modify: `src/components/settings/VideoSettings.tsx`

- [ ] **Step 1: Add monitor_index field to VideoSettings**

```rust
#[serde(default)]
pub monitor_index: u32, // 0 = primary
```

- [ ] **Step 2: Enumerate monitors via Win32 API**

```rust
#[cfg(windows)]
pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    use windows::Win32::Graphics::Gdi::*;
    // EnumDisplayMonitors callback
}
```

- [ ] **Step 3: Pass monitor offset to gdigrab in segment_recorder.rs**

- [ ] **Step 4: Add monitor dropdown in VideoSettings.tsx**

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(recording): add multi-monitor support with monitor selection"
```

---

### Task 22: Window State Verification (Spec 3.2)

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/windows_capture.rs`

- [ ] **Step 1: Add window visibility check before capture**

```rust
#[cfg(windows)]
fn is_game_window_visible() -> Result<bool, String> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let hwnd = unsafe { FindWindowW(None, w!("League of Legends (TM) Client")) };
    if hwnd.0 == 0 { return Ok(false); }
    Ok(unsafe { IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() })
}
```

- [ ] **Step 2: Check before recording start, warn during recording**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(recording): verify game window visible before capture start"
```

---

### Task 23: Audio Device Enumeration & Fallback (Spec 3.3)

**Files:**
- Modify: `src-tauri/src/recording/wasapi_audio.rs`
- Modify: `src-tauri/src/settings/models.rs`

- [ ] **Step 1: Add audio_device_id to AudioSettings**

```rust
#[serde(default)]
pub audio_device_id: Option<String>, // None = system default
```

- [ ] **Step 2: Enumerate WASAPI output devices**

- [ ] **Step 3: Add device change notification callback**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(audio): enumerate devices with selection and disconnect fallback"
```

---

### Task 24: Audio-Video Sync Verification (Spec 3.4)

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Record WASAPI start timestamp**

- [ ] **Step 2: Use -itsoffset for alignment in FFmpeg mux**

- [ ] **Step 3: Post-mux ffprobe duration check (tolerance 100ms)**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(recording): audio-video sync alignment with ffprobe validation"
```

---

### Task 25: Sample Rate Validation (Spec 3.5)

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Query WASAPI device sample rate before recording**

- [ ] **Step 2: Add -ar 48000 resample if mismatch**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(recording): auto-detect and resample audio sample rate"
```

---

### Task 26: Segment Integrity Verification (Spec 3.6)

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Add ffprobe validation after segment write**

```rust
async fn verify_segment(path: &Path, ffmpeg_path: &Path) -> bool {
    let output = TokioCommand::new(ffmpeg_path)
        .arg("-v").arg("error")
        .arg("-i").arg(path.to_str().unwrap())
        .arg("-f").arg("null").arg("-")
        .output().await;
    matches!(output, Ok(o) if o.status.success())
}
```

- [ ] **Step 2: Remove invalid segments from concat list**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(recording): ffprobe segment integrity check after write"
```

---

### Task 27: Hardware Encoder Failure Recovery (Spec 3.7)

**Files:**
- Modify: `src-tauri/src/video/processor/pipeline.rs`

- [ ] **Step 1: Detect hardware encoder failure and retry with libx264**

```rust
match execute_ffmpeg_command(&mut command).await {
    Ok(output) => Ok(output),
    Err(e) if is_hardware_encoder && !is_retry => {
        tracing::warn!("Hardware encoder failed: {}. Retrying with libx264", e);
        // Rebuild command with libx264
        retry_with_software_encoder(input, output, params).await
    }
    Err(e) => Err(e),
}
```

- [ ] **Step 2: Emit notification event to frontend**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(video): auto-fallback to software encoder on hardware failure"
```

---

## Chunk 6: Recording Pipeline Part 2

### Task 28: FFmpeg Crash Recovery (Spec 3.8)

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Monitor FFmpeg process exit during recording**

- [ ] **Step 2: On unexpected exit, attempt 1 restart within 5 seconds**

- [ ] **Step 3: Preserve existing segments on crash**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(recording): auto-restart FFmpeg on unexpected crash"
```

---

### Task 29: Remake/Surrender Detection (Spec 3.9)

**Files:**
- Modify: `src-tauri/src/recording/live_client.rs`
- Modify: `src-tauri/src/settings/models.rs`

- [ ] **Step 1: Add GameResult enum**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameResult {
    Victory,
    Defeat,
    EarlySurrender,
    Remake,
    Unknown,
}
```

- [ ] **Step 2: Infer result from game_time at GameEnd**

```rust
fn infer_game_result(game_time_secs: f64) -> GameResult {
    if game_time_secs < 300.0 { GameResult::Remake }
    else if game_time_secs < 1200.0 { GameResult::EarlySurrender }
    else { GameResult::Unknown }
}
```

- [ ] **Step 3: Add filter setting for minimum game duration**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(events): infer game result from duration for remake/surrender detection"
```

---

### Task 30: Steal Detection Tuning (Spec 3.10)

**Files:**
- Modify: `src-tauri/src/recording/live_client.rs`
- Modify: `src-tauri/src/settings/models.rs`

- [ ] **Step 1: Add contest_window_secs to EventFilterSettings**

Note: `AdvancedSettings` does not exist. The correct target is `EventFilterSettings` in `src-tauri/src/settings/models.rs`, which already contains `min_priority` and other event tuning fields.

```rust
// In EventFilterSettings struct:
#[serde(default = "default_contest_window")]
pub contest_window_secs: u32, // default 10, range 5-20

fn default_contest_window() -> u32 { 10 }
```

Also add to `EventFilterSettings::validate()`:
```rust
if self.contest_window_secs < 5 || self.contest_window_secs > 20 {
    return Err(format!("contest_window_secs {} out of range 5-20", self.contest_window_secs));
}
```

- [ ] **Step 2: Replace hardcoded Duration::from_secs(10) with configurable value**

- [ ] **Step 3: Adjust 15s kill retention window proportionally**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(events): configurable steal detection window with proportional kill retention"
```

---

### Task 31: Spectator Mode Detection (Spec 3.11)

**Files:**
- Modify: `src-tauri/src/recording/live_client.rs`

- [ ] **Step 1: Add spectator detection signals**

- [ ] **Step 2: Disable event clipping when spectating**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(live-client): improved spectator mode detection"
```

---

### Task 32: Video Aspect Ratio Handling (Spec 3.12)

**Files:**
- Modify: `src-tauri/src/video/processor/pipeline.rs`

- [ ] **Step 1: Detect aspect ratio via ffprobe**

- [ ] **Step 2: Add center-crop filter for 9:16 Shorts format**

```
-vf "crop=ih*9/16:ih:(iw-ih*9/16)/2:0"
```

- [ ] **Step 3: Add user preference for crop vs pad**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(video): auto-crop 16:9 to 9:16 for Shorts format"
```

---

### Task 33: FFmpeg Stderr Buffer Safety (Spec 3.13)

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Add line count limit (10000) and severity filter**

- [ ] **Step 2: Only log ERROR/WARNING lines**

- [ ] **Step 3: Commit**

```bash
git commit -m "fix(recording): limit FFmpeg stderr buffer to prevent deadlock"
```

---

### Task 34: Event Session Scoping (Spec 3.14)

**Files:**
- Modify: `src-tauri/src/recording/live_client.rs`

- [ ] **Step 1: Track session_id (game start timestamp)**

- [ ] **Step 2: Reset last_event_id on new session**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(events): scope event IDs to game session to prevent duplicates"
```

---

## Chunk 7: Frontend Quality Part 1

### Task 35: Frontend Form Validation with zod (Spec 1.4)

**Files:**
- Create: `src/lib/validation.ts`
- Modify: `package.json` (add zod dependency)

- [ ] **Step 1: Install zod**

Run: `npm install zod`

- [ ] **Step 2: Create validation schemas**

```typescript
import { z } from 'zod';

export const loginSchema = z.object({
  email: z.string().email(),
  password: z.string().min(8).max(128),
});

export const signupSchema = z.object({
  email: z.string().email(),
  password: z.string().min(8).max(128),
  confirmPassword: z.string(),
}).refine((data) => data.password === data.confirmPassword, {
  message: "Passwords don't match",
  path: ["confirmPassword"],
});

export const youtubeUploadSchema = z.object({
  title: z.string().min(1).max(100),
  description: z.string().max(5000).optional(),
  tags: z.string().max(500).optional(),
  privacyStatus: z.enum(['public', 'unlisted', 'private']),
});
```

- [ ] **Step 3: Apply to LoginForm, SignupForm, YouTubeUpload**

- [ ] **Step 4: Run tests**

Run: `npx jest --no-coverage`

- [ ] **Step 5: Commit**

```bash
git add package.json package-lock.json src/lib/validation.ts \
  src/components/auth/ src/components/youtube/
git commit -m "feat(validation): add zod schema validation for all forms"
```

---

### Task 36: Extend Error Boundary Coverage (Spec 5.1)

**Files:**
- Modify: `src/pages/Editor.tsx`
- Modify: `src/components/youtube/YouTubeUpload.tsx`
- Modify: `src/components/social/TikTokUpload.tsx` (archived obsolete direct-upload scope)
- Modify: `src/components/social/InstagramUpload.tsx` (archived obsolete direct-upload scope)

- [ ] **Step 1: Wrap each panel with appropriate ErrorBoundary**

```tsx
import { ErrorBoundary, VideoErrorBoundary, FormErrorBoundary } from '@/components/ErrorBoundary';

// Editor panel
<VideoErrorBoundary>
  <EditorContent />
</VideoErrorBoundary>

// Upload panels
<FormErrorBoundary>
  <YouTubeUpload />
</FormErrorBoundary>
```

- [ ] **Step 2: Run tests**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(ux): wrap feature panels with specialized error boundaries"
```

---

### Task 37: Modal Focus Management (Spec 5.2)

**Files:**
- Modify: `src/components/PaymentModal.tsx` (historical payment UI reference only; payment/Toss remains deferred)
- Modify: `src/components/editor/ExportModal.tsx`

- [ ] **Step 1: Ensure all dialogs use Radix Dialog with focus trap**

- [ ] **Step 2: Add aria-modal="true"**

- [ ] **Step 3: Verify Escape closes, focus returns to trigger**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(a11y): ensure focus trap and keyboard dismissal for all modals"
```

---

### Task 38: Keyboard Navigation (Spec 5.3)

**Files:**
- Modify: `src/components/video/VideoPlayer.tsx`
- Modify: `src/components/editor/Timeline.tsx`

- [ ] **Step 1: Add onKeyDown handler to VideoPlayer**

```tsx
const handleKeyDown = (e: React.KeyboardEvent) => {
  switch (e.key) {
    case ' ': e.preventDefault(); togglePlay(); break;
    case 'ArrowLeft': seek(-5); break;
    case 'ArrowRight': seek(5); break;
    case 'ArrowUp': e.preventDefault(); adjustVolume(0.1); break;
    case 'ArrowDown': e.preventDefault(); adjustVolume(-0.1); break;
    case 'm': case 'M': toggleMute(); break;
    case 'f': case 'F': toggleFullscreen(); break;
  }
};
```

- [ ] **Step 2: Add tabIndex={0} to make focusable**

- [ ] **Step 3: Add keyboard controls to Timeline**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(a11y): add keyboard navigation for VideoPlayer and Timeline"
```

---

### Task 39: Empty State Coverage (Spec 5.5)

**Files:**
- Modify: `src/pages/Editor.tsx`

- [ ] **Step 1: Add EmptyState for no clips**

```tsx
{clips.length === 0 && (
  <EmptyState
    title={t('editor.noClips')}
    description={t('editor.noClipsDescription')}
    action={{ label: t('editor.goToSettings'), onClick: () => navigate('/settings') }}
  />
)}
```

- [ ] **Step 2: Add i18n keys**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(ux): add empty state components for Editor and AutoEdit"
```

---

### Task 40: Success Feedback (Spec 5.6)

**Files:**
- Modify: `src/pages/Settings.tsx`
- Modify: `src/components/RecordingControls.tsx`

- [ ] **Step 1: Add toast notifications for settings save, recording start/stop**

Use the project's existing toast system (`@/components/ui/use-toast`), NOT `sonner`:

```tsx
import { useToast } from "@/components/ui/use-toast";
const { toast } = useToast();

const handleSave = async () => {
  await saveSettings(settings);
  toast({ title: t('settings.saved') });
};
```

- [ ] **Step 2: Add i18n keys for all success messages**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(ux): add success toast notifications for user operations"
```

---

### Task 41: Error Message Improvement (Spec 5.7)

**Files:**
- Modify: `src/lib/errorMapper.ts`
- Modify: `src/locales/en/translation.json`
- Modify: `src/locales/ko/translation.json`

- [ ] **Step 1: Add BACKEND_ERROR_MAP to errorMapper.ts**

```typescript
const BACKEND_ERROR_MAP: Record<string, string> = {
  'DISK_FULL': 'errors.diskFull',
  'FFMPEG_NOT_FOUND': 'errors.ffmpegNotFound',
  'NETWORK_ERROR': 'errors.networkError',
  'AUTH_EXPIRED': 'errors.authExpired',
  'PROCESS_TIMEOUT': 'errors.processTimeout',
  'RATE_LIMITED': 'errors.rateLimited',
  'OUT_OF_MEMORY': 'errors.outOfMemory',
  'CORRUPTED_FILE': 'errors.corruptedFile',
  'DEVICE_DISCONNECTED': 'errors.deviceDisconnected',
  'SERVICE_UNAVAILABLE': 'errors.serviceUnavailable',
};
```

- [ ] **Step 2: Add Korean translations for each**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(ux): extend errorMapper with backend error code mappings"
```

---

## Chunk 8: Frontend Quality Part 2

### Task 42: Destructive Action Confirmation (Spec 5.9)

**Files:**
- Modify: `src/components/PaymentModal.tsx` (historical payment UI reference only; payment/Toss remains deferred)
- Modify: `src/pages/Settings.tsx`

- [ ] **Step 1: Add confirmation dialog for subscription cancellation, bulk delete, settings reset**

- [ ] **Step 2: Commit**

```bash
git commit -m "feat(ux): add confirmation dialogs for destructive actions"
```

---

### Task 43: Color Contrast Fix (Spec 5.10)

**Files:**
- Modify: `src/index.css`

- [ ] **Step 1: Audit muted-foreground contrast ratios**

- [ ] **Step 2: Adjust CSS variable if below 4.5:1**

- [ ] **Step 3: Commit**

```bash
git commit -m "fix(a11y): improve muted-foreground contrast to meet WCAG AA"
```

---

### Task 44: ARIA Label Audit (Spec 5.11)

**Files:**
- Multiple component files

- [ ] **Step 1: Add aria-label to all icon-only buttons**

- [ ] **Step 2: Priority: recording controls, video player, timeline, settings**

- [ ] **Step 3: Commit**

```bash
git commit -m "fix(a11y): add aria-labels to all interactive elements"
```

---

### Task 45: Upload Metadata Validation (Spec 5.12)

**Files:**
- Modify: `src/components/youtube/YouTubeUpload.tsx`
- Modify: `src/components/social/TikTokUpload.tsx` (archived obsolete direct-upload scope)
- Modify: `src/components/social/InstagramUpload.tsx` (archived obsolete direct-upload scope)

- [ ] **Step 1: Add title/description/tags validation with character counts**

- [ ] **Step 2: Disable upload button until valid**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(upload): validate metadata with character counts before upload"
```

---

### Task 46: Long Operation Progress (Spec 5.13)

**Files:**
- Modify: `src/components/editor/ExportModal.tsx`

- [ ] **Step 1: Parse FFmpeg frame progress for percentage**

- [ ] **Step 2: Multi-stage progress bar with stage labels**

- [ ] **Step 3: Add cancel button**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(ux): multi-stage progress bar for video export"
```

---

### Task 47: Realistic Disk Space Fallback (Spec 4.8)

**Files:**
- Modify: `src-tauri/src/recording/commands.rs`

- [ ] **Step 1: Return DiskInfo { known: false } when API fails**

```rust
pub struct DiskInfo {
    pub known: bool,
    pub total: Option<u64>,
    pub free: Option<u64>,
}
```

- [ ] **Step 2: Remove hardcoded 500GB/100GB fallback**

- [ ] **Step 3: Commit**

```bash
git commit -m "fix(disk): return unknown instead of fake values when API fails"
```

---

### Task 48: Frontend Polling Cleanup (Spec 2.6)

**Files:**
- Modify: `src/pages/Dashboard.tsx`
- Modify: `src/components/youtube/YouTubeUpload.tsx`
- Modify: `src/components/RecordingControls.tsx`
- Modify: `src/components/ClipLibrary.tsx`
- Modify: `src/components/editor/ClipLibrary.tsx`

- [ ] **Step 1: Add isMounted ref pattern to all polling components**

```tsx
useEffect(() => {
  const isMounted = { current: true };
  const interval = setInterval(async () => {
    if (!isMounted.current) return;
    const data = await fetchData();
    if (isMounted.current) setState(data);
  }, 5000);
  return () => {
    isMounted.current = false;
    clearInterval(interval);
  };
}, []);
```

- [ ] **Step 2: Commit**

```bash
git commit -m "fix(frontend): add isMounted checks to all polling intervals"
```

---

## Chunk 9: Infrastructure

### Task 49: Sentry DSN from Environment (Spec 6.3)

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src/main.tsx`

- [ ] **Step 1: Read SENTRY_DSN from env, skip init if empty**

- [ ] **Step 2: Remove hardcoded placeholder DSN strings**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(infra): read Sentry DSN from environment variables"
```

---

### Task 50: Environment Variable Validation (Spec 6.5)

**Files:**
- Create: `src-tauri/src/utils/env_validation.rs`
- Modify: `src-tauri/src/utils/mod.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Create env_validation.rs**

```rust
pub struct EnvValidationResult {
    pub required_missing: Vec<String>,
    pub optional_missing: Vec<String>,
    pub malformed: Vec<(String, String)>,
}

pub fn validate_env() -> EnvValidationResult {
    let mut result = EnvValidationResult::default();
    // Check SUPABASE_URL, SUPABASE_ANON_KEY
    // Check optional: YOUTUBE_CLIENT_ID, SENTRY_DSN
    result
}
```

- [ ] **Step 2: Call at startup, show dialog if required vars missing**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(infra): validate environment variables at startup"
```

---

### Task 51: Code Signing Configuration (Spec 6.1)

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Set timestampUrl to digicert**

- [ ] **Step 2: Document certificate setup in CI vars**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(build): configure code signing timestamp URL"
```

---

### Task 52: Auto-Updater Key Generation (Spec 6.2)

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Document key generation process**

- [ ] **Step 2: Set placeholder notes for CI secret setup**

- [ ] **Step 3: Commit**

```bash
git commit -m "docs(build): document auto-updater key generation process"
```

---

### Task 53: Pre-commit Hooks (Spec 6.4)

**Files:**
- Modify: `package.json`
- Create: `.husky/pre-commit`
- Create: `.husky/pre-push`

- [ ] **Step 1: Install husky + lint-staged**

Run: `npm install -D husky lint-staged && npx husky init`

- [ ] **Step 2: Configure lint-staged in package.json**

```json
"lint-staged": {
  "*.{ts,tsx}": ["eslint --fix", "prettier --write"]
}
```

- [ ] **Step 3: Create pre-commit hook**

```bash
#!/bin/sh
npx lint-staged
```

- [ ] **Step 4: Create pre-push hook**

```bash
#!/bin/sh
cd src-tauri && cargo clippy -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(dx): add husky pre-commit and pre-push hooks"
```

---

### Task 54: SBOM Generation (Spec 6.6)

**Files:**
- Modify: `package.json`

- [ ] **Step 1: Add sbom npm script**

- [ ] **Step 2: Commit**

```bash
git commit -m "feat(compliance): add SBOM generation scripts"
```

---

### Task 55: Settings Backup/Restore (Spec 6.7)

**Files:**
- Modify: `src-tauri/src/settings/commands.rs`
- Modify: `src/pages/Settings.tsx`

- [ ] **Step 1: Add export_settings and import_settings Tauri commands**

- [ ] **Step 2: Add UI buttons in Settings page**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(settings): user-facing backup/restore with file dialog"
```

---

### Task 56: User Data Export (Spec 6.8)

**Files:**
- Create: `src-tauri/src/utils/data_export.rs`
- Modify: `src-tauri/src/utils/mod.rs`

- [ ] **Step 1: Create export_user_data command → ZIP of JSON files**

- [ ] **Step 2: Exclude video files and auth tokens**

- [ ] **Step 3: Add UI button in Settings > Account**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(gdpr): user data export as ZIP"
```

---

### Task 57: Minimum System Requirements Check (Spec 6.9)

**Files:**
- Create: `src-tauri/src/utils/system_check.rs`
- Modify: `src-tauri/src/utils/mod.rs`

- [ ] **Step 1: Check RAM, GPU, disk at startup**

- [ ] **Step 2: Warn (don't block) if below minimums**

- [ ] **Step 3: Show in Settings > About**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(infra): minimum system requirements check at startup"
```

---

## Chunk 10: Testing & Observability

### Task 58: Code Coverage in CI (Spec 6.10)

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `jest.config.js`

- [ ] **Step 1: Add --coverage to CI test step**

- [ ] **Step 2: Set initial threshold at 50%**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(ci): add code coverage tracking with 50% threshold"
```

---

### Task 59: Rust Backend Unit Tests (Spec 6.11)

**Files:**
- Modify: `src-tauri/src/settings/models.rs` (add #[cfg(test)] mod tests)
- Modify: `src-tauri/src/utils/security.rs`
- Modify: `src-tauri/src/recording/live_client.rs`
- Modify: `src-tauri/src/utils/rate_limit.rs`
- Modify: `src-tauri/src/error.rs`

- [ ] **Step 1: Write 10+ tests for settings validation**

- [ ] **Step 2: Write 10+ tests for security validation**

- [ ] **Step 3: Write 15+ tests for event parsing and steal detection**

- [ ] **Step 4: Write 5+ tests for rate limiter**

- [ ] **Step 5: Write 5+ tests for error serialization**

- [ ] **Step 6: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: 45+ tests PASS

- [ ] **Step 7: Commit**

```bash
git commit -m "test(rust): add 45+ unit tests for critical backend modules"
```

---

### Task 60: Structured Operation Logging (Spec 7.1)

**Files:**
- Modify: `src-tauri/src/video/processor/pipeline.rs`
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`
- Modify: `src-tauri/src/settings/mod.rs`

- [ ] **Step 1: Log full FFmpeg command before execution**

```rust
tracing::info!(command = %args.join(" "), "Executing FFmpeg");
```

- [ ] **Step 2: Log encoder detection, settings source, recording config**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(observability): add structured logging for operations"
```

---

### Task 61: Recording Quality Metrics (Spec 7.2)

**Files:**
- Modify: `src-tauri/src/recording/integration_backend/segment_recorder.rs`

- [ ] **Step 1: Parse FFmpeg stats output (frame, fps, bitrate, drop_frames)**

- [ ] **Step 2: Emit quality metrics to frontend**

- [ ] **Step 3: Warn if dropped frames > 5%**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(observability): parse and emit recording quality metrics"
```

---

### Task 62: Performance Metrics (Spec 7.3)

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/video/processor/pipeline.rs`

- [ ] **Step 1: Log startup time, encode time, upload throughput** — upload metrics are historical plan scope and do not imply TikTok/Instagram direct upload is current.

- [ ] **Step 2: Commit**

```bash
git commit -m "feat(observability): track app startup and processing performance"
```

---

### Task 63: Background Task Status (Spec 7.4)

**Files:**
- Create: `src-tauri/src/utils/health.rs`
- Modify: `src-tauri/src/utils/mod.rs`
- Modify: `src/pages/Settings.tsx`

- [ ] **Step 1: Create get_system_health Tauri command**

```rust
#[derive(Serialize)]
pub struct SystemHealth {
    pub game_monitor: &'static str,
    pub upload_scheduler: &'static str, // Historical plan scope; not evidence of current TikTok/Instagram direct upload.
    pub disk_monitor: &'static str,
    pub ffmpeg_processes: usize,
    pub event_queue_size: usize,
}
```

- [ ] **Step 2: Display in Settings > About or debug panel**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(observability): background task health status API"
```

---

### Task 64: Audit Logging (Spec 4.9)

**Files:**
- Create: `src-tauri/src/utils/audit.rs`
- Modify: `src-tauri/src/utils/mod.rs`

- [ ] **Step 1: Create audit logger with daily rotation**

```rust
pub fn audit_log(category: &str, action: &str, details: &str) {
    // Write to audit.log with timestamp, category, action, details
}
```

- [ ] **Step 2: Add audit calls to upload, delete, settings change, auth events** — upload/auth audit scope is historical plan context and does not revive TikTok/Instagram direct upload.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(observability): structured audit logging with daily rotation"
```

---

## Success Criteria

> **Historical success criteria only:** Passing these engineering checks is not proof of current 100% commercial readiness or production readiness. E5 Field QA remains required, and TikTok/Instagram direct upload remains out of current scope.

After all 64 tasks:

1. `cd src-tauri && cargo test` — 45+ Rust tests PASS
2. `cd src-tauri && cargo clippy -- -D warnings` — no warnings
3. `npx jest --no-coverage` — all frontend tests PASS
4. `cd src-tauri && cargo check` — compiles clean
5. No hardcoded English strings in UI
6. All Tauri commands return `AppResult<T>`
7. All user inputs validated before processing
8. FFmpeg processes managed through pool (max 3)
9. Settings backup/restore functional
10. Pre-commit hooks active
