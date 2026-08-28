# LoLShorts System Architecture

Comprehensive overview of LoLShorts architecture, data flow, and key components.

## Table of Contents

- [System Overview](#system-overview)
- [Architecture Diagram](#architecture-diagram)
- [Core Modules](#core-modules)
- [Data Flow](#data-flow)
- [Frontend Architecture](#frontend-architecture)
- [Backend Architecture](#backend-architecture)
- [Integration Points](#integration-points)

---

## System Overview

LoLShorts is a desktop application that:

1. **Captures** League of Legends gameplay in real-time via FFmpeg screen recording
2. **Detects** important gameplay events (kills, multikills, objectives) via League Client Update API
3. **Extracts** relevant clips based on event detection
4. **Edits** clips with auto-composition (Shorts format: 9:16, or Montage format: 16:9)
5. **Uploads** to YouTube with metadata and thumbnails

### Key Design Principles

- **Local Processing**: All video analysis and editing happens on user's PC (no cloud upload)
- **Segment-Based Recording**: Circular buffer of 60-second replay window for instant clip extraction
- **Official runtime target**: Windows 11 x64 with NVIDIA NVENC. AMD, Intel, CPU,
  macOS, and Linux code paths remain experimental fallbacks and are not covered by
  the public-release support or performance guarantee.
- **Modular Architecture**: Independent modules for recording, video, auth, YouTube integration

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    LoLShorts Desktop App                        │
│                      (Tauri + React)                            │
└─────────────────────────────────────────────────────────────────┘
                              │
                ┌─────────────┼─────────────┐
                │             │             │
        ┌───────▼──────┐  ┌───▼───────┐  ┌─▼──────────────┐
        │   Frontend   │  │  Backend  │  │  System APIs   │
        │  (React 18)  │  │  (Rust)   │  │                │
        └──────────────┘  └───────────┘  └────────────────┘
              │                 │              │
              │         ┌───────┴──────────┐   │
              │         │                  │   │
         ┌────▼────┐  ┌─▴──────────┐   ┌──┴───▼──┐
         │Zustand  │  │FFmpeg CLI  │   │LCU/Live │
         │Stores   │  │(gdigrab)   │   │Client   │
         └─────────┘  └────────────┘   └─────────┘
                │           │               │
         ┌──────┴───────────┴──────┬────────┘
         │                        │
    ┌────▼──────────┐      ┌──────▼───────┐
    │   SQLite DB   │      │  LoL Client  │
    │  (Local)      │      │  (Port 2999) │
    └───────────────┘      └──────────────┘

┌──────────────────────────────────────┐
│   External Services                  │
├──────────────────────────────────────┤
│ • Supabase (Auth + Entitlements)     │
│ • YouTube API v3 (Upload)            │
│ • Toss Payments (Deferred/Future)    │
└──────────────────────────────────────┘
```

SQLite is the local app-data database only. Supabase Auth and Supabase
Postgres are the authoritative source for authentication, billing records,
and PRO entitlement. Local SQLite values must never grant paid access.

---

## Core Modules

### 1. Recording Module (`src-tauri/src/recording/`)

**Responsibility**: Capture gameplay, detect events, create clips

#### Components

- **`integration_backend.rs`** - Main recording engine
  - Manages FFmpeg process lifecycle
  - Implements circular buffer (6 segments × 10 seconds = 60s replay window)
  - Extracts clips when events occur
  - Handles multiple hardware encoders (NVENC/AMF/QSV)

- **`game_monitor.rs`** - Game detection
  - Monitors LoL process via EnumWindows (Windows API)
  - Detects game start/stop
  - Detects window focus changes

- **`live_client.rs`** - Live Client API integration
  - Connects to LoL client on port 2999
  - Polls for player stats (kills, deaths, CS)
  - Parses game events (champion selected, game started, etc.)

- **`audio.rs`** - Audio capture
  - Captures system audio (game sounds)
  - Supports platform-specific backends:
    - Windows: WASAPI (Windows Audio Session API)
    - macOS: AVFoundation
    - Linux: ALSA/PulseAudio

- **`auto_clip_manager.rs`** - Event-based clip creation
  - Listens for game events from Live Client API
  - Calculates event importance (priority 1-5)
  - Triggers clip extraction at event timestamp

#### Data Flow

```
LoL Game Start
    ↓
Game Monitor detects window
    ↓
FFmpeg starts recording (gdigrab + audio capture)
    ↓
Recording Engine writes segments to circular buffer
    ↓
Live Client API polls for events (every 500ms)
    ↓
Auto Clip Manager detects kill/multikill/objective
    ↓
Clip Manager extracts segment window around event
    ↓
Clip saved to storage (SQLite + file system)
```

### 2. Video Module (`src-tauri/src/video/`)

**Responsibility**: Process clips, apply effects, export videos

#### Components

- **`processor.rs`** - FFmpeg video operations
  - Re-encode videos (convert codec/format)
  - Trim/crop clips
  - Merge clips into montages
  - Apply filters (scaling, color correction)
  - Burn subtitles (captions, timestamps)

- **`auto_composer.rs`** - Auto-edit algorithm
  - Implements priority-based clip selection
  - Fills target duration (60s, 120s, 180s)
  - Applies canvas overlays (branding, text)
  - Mixes audio (game + background music)
  - Exports to YouTube-optimized format (9:16 vertical or 16:9 montage)

- **`thumbnail.rs`** - Thumbnail generation
  - Extracts frames from clips
  - Applies text overlays (video title)
  - Generates YouTube-friendly thumbnails

- **`statistics.rs`** - Clip analysis
  - Calculates clip duration and quality metrics
  - Tracks which clips were used (prevents duplicates)
  - Provides stats for auto-edit algorithm

#### Data Flow

```
Raw Clip Files
    ↓
[Optional] User selects clips in Editor
    ↓
Auto-Composer selects clips by priority
    ↓
FFmpeg processes (re-encodes, trims, merges)
    ↓
[Optional] Apply canvas overlay + background music
    ↓
Export to .mp4 (YouTube format)
    ↓
Generate thumbnail
    ↓
Ready for upload
```

### 3. Authentication Module (`src-tauri/src/auth/`)

**Responsibility**: User authentication and session management

#### Components

- **`mod.rs`** - Main auth logic
  - Handles user login/signup via Supabase
  - Manages JWT tokens (access + refresh)
  - Keeps the active desktop session in memory

- **`middleware.rs`** - Auth middleware
  - Validates tokens before API calls
  - Refreshes expired tokens
  - Gates authenticated operations

- **`commands.rs`** - Session and entitlement bridge
  - `set_session` validates the Supabase Auth token with `/auth/v1/user`
  - Fetches entitlement only from Supabase `user_licenses`
  - Returns the standard entitlement response used by the UI

#### Integration with Supabase

```
User Login
    ↓
Supabase Authentication
    ↓
JWT token received by supabase-js
    ↓
Tauri set_session validates token subject
    ↓
Supabase user_licenses checked for FREE/PRO entitlement
    ↓
UI gates PRO features from authoritative entitlement only
```

### 4. League Client Integration (`src-tauri/src/lcu/`)

**Responsibility**: Communicate with League Client Update API

#### API Endpoints Used

| Endpoint | Purpose |
|----------|---------|
| `/lol-summoner/v1/current-summoner` | Get current player info |
| `/lol-match-history/v1/products/lol/me/matches` | Get match history |
| `/lol-game-queues/v1/queues` | Get queue info |
| `/lol-gameflow/v1/gameflow-phase` | Monitor game state |

#### Event Detection

Live Client API polling (every 500ms) detects:

- Game start/stop
- Champion selection
- Kills, deaths, assists
- Objective captures (dragon, baron, towers)
- Game end state

### 5. YouTube Integration (`src-tauri/src/youtube/`)

**Responsibility**: OAuth2 authentication and video upload

#### Components

- **`oauth.rs`** - OAuth2 flow
  - Implements authorization code flow
  - Stores refresh tokens for automated uploads

- **`callback_server.rs`** - Local OAuth callback listener
  - Binds to the host/port/path parsed from `YOUTUBE_REDIRECT_URI`
    (`CallbackServer::from_redirect_uri` — single source of truth, so the
    redirect URI handed to Google and the listener that receives it can
    never drift apart)
  - Default (no `YOUTUBE_REDIRECT_URI` set): `http://localhost:9090/oauth/callback`

- **`upload.rs`** - Video upload
  - Implements resumable upload (handles network interruptions)
  - Sets video metadata (title, description, tags, thumbnail)
  - Manages upload progress

#### Upload Flow

```
User authorizes YouTube in app
    ↓
Receives OAuth authorization code
    ↓
Exchange code for access token + refresh token
    ↓
User selects video to upload
    ↓
Upload to YouTube with metadata
    ↓
Set custom thumbnail
    ↓
Video published (public/unlisted/private per user choice)
```

### 6. Settings & Configuration (`src-tauri/src/settings/`)

**Responsibility**: User preferences and hardware detection

#### Components

- **`platform_config.rs`** - Hardware detection
  - Detects GPU model (NVIDIA/AMD/Intel)
  - Chooses optimal encoder (NVENC/AMF/QSV)
  - Detects monitor resolution and refresh rate

- **`storage.rs`** - Persistent settings
  - Stores recording quality (720p/1080p)
  - Stores audio preferences
  - Stores hotkey bindings
  - Stores language preference

- **`models.rs`** - Settings data structures
  - `RecordingSettings`: Video/audio quality
  - `HotkeySettings`: Key bindings
  - `EventFilterSettings`: Which events to record
  - `LanguageSettings`: UI language

#### Settings Persistence

Recording settings are stored locally. General app metadata is stored in the
local SQLite database; authentication, billing, and PRO entitlement are not
stored there as authoritative state.
```
Database: ~/.local/share/lolshorts/lolshorts.db (macOS/Linux)
          %APPDATA%/lolshorts/lolshorts.db (Windows)
```

### 7. Storage Module (`src-tauri/src/storage/`)

**Responsibility**: Local data persistence

#### Data Models

- **`Clip`**: Metadata for recorded clips
  - Clip ID, file path, duration
  - Event type (kill, multikill, baron, etc.)
  - Timestamp, game ID
  - Priority score

- **`ComposedVideo`**: Metadata for edited videos
  - Video ID, file path, duration
  - Associated clips (array)
  - Format (shorts/montage)
  - Thumbnail path
  - Upload status

- **`Settings`**: Local key/value settings used by app integrations
  - YouTube queue and local integration state
  - Non-authoritative local preferences

#### Database Schema

```sql
CREATE TABLE games (
  game_id TEXT PRIMARY KEY,
  metadata_json TEXT NOT NULL,
  champion TEXT NOT NULL,
  game_mode TEXT NOT NULL,
  start_time TEXT NOT NULL,
  end_time TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE clips (
  game_id TEXT NOT NULL,
  file_path TEXT NOT NULL,
  metadata_json TEXT NOT NULL,
  event_time REAL NOT NULL,
  priority INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (game_id, file_path)
);

CREATE TABLE auto_edit_results (
  result_id TEXT PRIMARY KEY,
  metadata_json TEXT NOT NULL,
  output_path TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

---

## Data Flow

### Complete End-to-End Flow

```
1. GAME START
   └─→ Game Monitor detects LoL process
       └─→ Live Client API connects (port 2999)

2. RECORDING
   └─→ FFmpeg starts (gdigrab screen capture + audio)
       └─→ Writes to circular buffer (60s rolling window)
       └─→ Segments: 6 × 10 seconds

3. EVENT DETECTION
   └─→ Live Client API polls every 500ms
       └─→ Parses: gold, kills, objectives, game events
       └─→ Auto Clip Manager evaluates event priority

4. CLIP EXTRACTION
   └─→ Priority ≥ threshold triggers clip creation
       └─→ Extract window: [event_time - 5s to event_time + 5s]
       └─→ Re-encode with hardware acceleration
       └─→ Save to local storage

5. CLIP STORAGE
   └─→ Metadata → SQLite (timestamp, event type, priority)
       └─→ File → File system (~50-100MB per clip)

6. EDITING (User Initiated)
   └─→ User selects clips in Editor UI
       └─→ Auto-Composer ranks clips by priority
       └─→ Fills target duration (60s/120s/180s)

7. COMPOSITION
   └─→ [Optional] Apply canvas overlay (branding)
       └─→ [Optional] Mix audio (game + background)
       └─→ Merge selected clips
       └─→ Export to .mp4 (9:16 shorts or 16:9 montage)

8. UPLOAD PREP
   └─→ Generate thumbnail
       └─→ Generate title/description from game stats
       └─→ Prepare metadata

9. YOUTUBE UPLOAD
   └─→ User clicks "Upload to YouTube"
       └─→ OAuth2 flow (if not authorized)
       └─→ Resumable upload to YouTube
       └─→ Set thumbnail + metadata
       └─→ Publish (visibility per user choice)

10. POST-UPLOAD
    └─→ Store YouTube video ID in database
        └─→ Update clip status (used/uploaded)
        └─→ Show upload link to user
```

### Alternative Flow: Replay Mode

```
User selects Replays tab
    ↓
Fetches match history from LCU API
    ↓
User clicks "Download" → Downloads replay file
    ↓
User clicks "Watch" → Opens in LoL client
    ↓
Target Modal asks "Who to record?"
    ↓
User selects player (e.g., "Faker")
    ↓
Game starts, camera follows selected player
    ↓
[Same as GAME START → EVENT DETECTION → CLIP EXTRACTION flow]
```

---

## Frontend Architecture

### State Management (Zustand)

Multiple independent stores for separation of concerns:

#### Recording Store
```typescript
{
  isRecording: boolean
  recordingStatus: 'idle' | 'recording' | 'processing'
  currentClips: Clip[]
  recordingSettings: RecordingSettings
}
```

#### Editor Store
```typescript
{
  selectedClips: Clip[]
  compositionFormat: 'shorts' | 'montage'
  targetDuration: 60 | 120 | 180
  canvasOverlay: CanvasOverlay | null
  backgroundMusic: AudioFile | null
}
```

#### Auth Store
```typescript
{
  user: User | null
  entitlement: Entitlement | null
  isAuthenticated: boolean
  loading: boolean
}
```

`isPro` is derived at render/use time from `entitlement.tier === "PRO"` and
`entitlement.status === "active"`. It is not trusted from localStorage or
local SQLite.

### Component Hierarchy

```
AppShell (Layout)
├── Sidebar (Navigation)
│   ├── Dashboard link
│   ├── Recording link
│   ├── Editor link
│   ├── Settings link
│   └── YouTube link
└── Main Content Area
    ├── Dashboard Page
    │   ├── StatusDashboard
    │   ├── RecordingControls
    │   └── SubscriptionManagement
    ├── Recording Page
    │   ├── GameMonitor
    │   ├── ClipLibrary
    │   └── RecordingSettings
    ├── Editor Page
    │   ├── CanvasEditor
    │   ├── ClipCard (multiple)
    │   ├── VideoPreview
    │   ├── AudioMixer
    │   ├── ExportModal
    │   └── TemplateLibrary
    ├── Settings Page
    │   ├── VideoSettings
    │   ├── AudioSettings
    │   ├── HotkeySettings
    │   ├── EventFilterSettings
    │   └── GeneralSettings
    └── YouTube Page
        ├── YouTubeAuth
        ├── YouTubeHistory
        └── YouTubeUpload
```

### API Layer (`src/api/`)

Each module has corresponding API client:

```typescript
// src/api/auth.ts
export async function login(email, password): Promise<AuthResponse>
export async function signup(email, password): Promise<AuthResponse>
export async function logout(): Promise<void>

// src/api/recording.ts
export async function startRecording(): Promise<void>
export async function stopRecording(): Promise<void>
export async function getClips(): Promise<Clip[]>

// src/api/video.ts
export async function composeVideo(clips, format, duration): Promise<ComposedVideo>
export async function exportVideo(videoId): Promise<string> // Returns file path

// src/api/youtube.ts
export async function authorizeYouTube(): Promise<void>
export async function uploadVideo(videoId): Promise<YoutubeResponse>
```

All API calls invoke Tauri commands (IPC to backend).

### Internationalization (i18n)

- Translation files: `src/locales/{lang}/translation.json`
- Supported languages: English, Korean
- Runtime language detection via browser settings
- User can override in Settings

---

## Backend Architecture

### Tauri Command Pattern

All frontend-backend communication via Tauri commands (IPC):

```rust
// Defined in src-tauri/src/recording/commands.rs
#[tauri::command]
pub async fn start_recording(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.recording_backend.start().await
}
```

Invoked from frontend:
```typescript
import { invoke } from '@tauri-apps/api/core'

await invoke('start_recording')
```

### Error Handling

Custom error types via `thiserror`:

```rust
#[derive(thiserror::Error, Debug)]
pub enum RecordingError {
    #[error("FFmpeg process failed: {0}")]
    FFmpegError(String),

    #[error("Game not detected")]
    GameNotDetected,

    #[error("Insufficient disk space")]
    InsufficientDiskSpace,
}
```

Errors serialized to JSON and returned to frontend.

### Async Runtime (Tokio)

All I/O operations (file, network, process) are async:

```rust
#[tokio::main]
async fn main() {
    // Async initialization
    let recording = RecordingBackend::new().await?;
    // Handle background tasks
}
```

Benefits:
- Non-blocking UI (no frozen windows)
- Efficient resource usage
- Easy task cancellation

---

## Integration Points

### External Services

#### Supabase
- **Purpose**: User authentication + authoritative entitlements
- **Integration**: OAuth via supabase-js library
- **Data**: `user_profiles` for display data; `user_licenses` for FREE/PRO entitlement; `subscriptions` and `payments` for future server-side billing records
- **Fallback**: Fail closed to FREE/no paid access if entitlement cannot be verified

#### YouTube API v3
- **Purpose**: Video upload + metadata
- **Integration**: OAuth2 authorization code flow
- **Endpoints**: `youtube.googleapis.com/youtube/v3/videos`
- **Rate Limits**: 10,000 units/day (1 video upload ≈ 1,600 units)

#### Toss Payments
- **Purpose**: Future Korean payment processing for PRO subscription
- **Integration**: Deferred. Live checkout, client-side payment confirmation, and subscription mutation are disabled.
- **Data**: Future server-side payment/webhook path must update Supabase `payments`, `subscriptions`, and `user_licenses`.

### Platform-Specific Integrations

#### Windows
- **Screen Capture**: GDI (gdigrab via FFmpeg)
- **Audio**: WASAPI (Windows Audio Session API)
- **Process Monitoring**: Windows API (EnumWindows)
- **GPU Encoding**: NVIDIA NVENC (official); AMD AMF, Intel QSV, and software
  encoding are retained as experimental fallbacks

#### macOS (experimental, not release-supported)
- **Screen Capture**: AVFoundation (AVCaptureSession)
- **Audio**: CoreAudio / AVFoundation
- **Process Monitoring**: macOS API (GetProcesses)
- **GPU Encoding**: Apple Video Toolbox (H.264/H.265)

#### Linux (experimental, not release-supported)
- **Screen Capture**: X11 with xdamage or Wayland
- **Audio**: ALSA or PulseAudio
- **Process Monitoring**: /proc filesystem
- **GPU Encoding**: VAAPI (when available)

---

## Performance Considerations

The values in this section are design targets and historical observations, not a
current release guarantee. A release must carry fresh Windows 11 + NVIDIA field
evidence from the E5 packet (two 90-minute runs, capture/drop/FPS/bitrate/VMAF,
memory growth, clip-save latency, and clean shutdown) before these claims can be
used as acceptance criteria.

### Memory Management

- **Circular Buffer**: Limited to 60 seconds (prevents unbounded growth)
- **Segment Size**: 10 seconds per segment (~200-400MB per segment at 1080p/60fps)
- **Total Memory**: ~1.2-2.4GB for 60s rolling window
- **Cleanup**: Automatic deletion of old segments

### CPU Usage

- **FFmpeg Recording**: 10-15% CPU (hardware accelerated)
- **Event Polling**: <1% CPU (500ms interval)
- **Video Composition**: 30-50% CPU (depends on duration and effects)

### Disk I/O

- **Recording Rate**: 300-500 Mbps (1080p/60fps)
- **Disk Space Required**: ~50-100 GB for 8-hour gaming session
- **SSD Recommended**: For sustained write performance

### Network

- **Upload Speed**: YouTube resumable upload at user's bandwidth
- **Bandwidth**: ~500 Mbps per concurrent upload
- **Offline Support**: Videos can be composed offline, uploaded when ready

---

## Security & Privacy

### Authentication
- JWT tokens (Supabase)
- Refresh token rotation
- Secure token storage

### Data Privacy
- Local processing (no gameplay uploads to servers)
- Clips stored locally until user exports
- YouTube upload only on explicit user action
- Session data encrypted in transit

### Error Logging
- Logs contain no sensitive data (stripped of personal info)
- Logs stored locally in app data directory
- User can opt out of analytics

---

## Future Extensibility

### Plugin Architecture
Current design supports future plugin system for:
- Custom overlays/effects
- Additional game support (VALORANT, CS2)
- Export-first workflows for other target platforms, such as TikTok-ready formats

### Feature Gates
- PRO features controlled via feature_gate module
- Easy to add new subscription tiers

---

## Deployment & Distribution

### Version Control

- Semantic versioning (MAJOR.MINOR.PATCH)
- Version in: `package.json`, `Cargo.toml`, `tauri.conf.json`
- Tagged releases on GitHub

### Automated Updates

- Tauri Updater with signature verification
- Release assets: MSI + NSIS installers
- Auto-check on app startup

---

## References

- FFmpeg Documentation: https://ffmpeg.org/documentation.html
- Tauri Docs: https://tauri.app/
- React Hooks: https://react.dev/reference/react
- Zustand: https://github.com/pmndrs/zustand
- Supabase: https://supabase.com/docs
