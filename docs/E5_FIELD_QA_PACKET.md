# E5 Field QA Packet

Use this packet for the non-payment Windows release candidate. E5 must be run by a human tester on real Windows hardware with League of Legends, LCU, replay files, audio devices, GPU encoder or fallback, YouTube test account, and signed installer/updater artifacts.

## Required Evidence Folder

Create one folder per candidate build:

```text
field-qa/<version>-<commit>/
  00_environment.md
  01_lol_lcu/
  02_replay/
  03_capture_audio_gpu/
  04_youtube/
  05_installer_updater/
  06_recovery/
  07_support/
```

Do not place OAuth tokens, payment keys, signing keys, Supabase keys, or private user data in this folder.

## First Gameplay Smoke Run

Use this short run immediately after the automated non-game gate is green. It
is intended to find real capture/LCU/audio regressions quickly; it does not
replace either 90-minute performance run or the complete E5 matrix.

1. Keep the current League launch order. If League is already open, this run
   covers **League-first**; use **app-first** on the next run.
2. In one PowerShell terminal, start the current source candidate:

   ```powershell
   npm run tauri:dev
   ```

3. Complete the readiness wizard. Do not dismiss a failed FFmpeg, NVENC,
   League/LCU, audio, disk, or overlay-exclusion check as a pass. A desktop
   fallback is allowed only when the persistent privacy warning is visible in
   the main window.
4. In a second PowerShell 7 terminal, start the five-minute privacy-safe
   evidence collector before entering Practice Tool or a custom match:

   ```powershell
   pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/collect-gameplay-field-evidence.ps1 -DurationSeconds 300 -SampleIntervalSeconds 5 -CaptureMode desktop_duplication
   ```

5. During the match, confirm recording starts, trigger at least one detectable
   event, save at least one clip, and verify that the overlay is visible to the
   player. Afterward, play the saved clip and confirm the overlay is absent,
   all frame edges are preserved, and game/system audio is usable.
6. End the match normally. Confirm recording stops and the new files appear in
   the library without duplicates or corrupt entries. Quit LoLShorts and check
   that LoLShorts-owned FFmpeg/audio processes disappear within five seconds.
7. Keep the folder printed by the collector and report: launch order, capture
   mode/warning, recording start/stop, saved clip count, overlay exclusion,
   video/audio result, exit result, and the evidence-folder basename. Do not
   paste tokens, usernames, raw LCU payloads, or private filesystem paths.

If the five-minute run passes, proceed to the opposite launch order, reconnect,
replay, recovery, two 90-minute capture runs, and 30-clip labeling below.

## Optional Gameplay Evidence Collector

While a tester runs a real capture session, collect supporting evidence with PowerShell 7:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/collect-gameplay-field-evidence.ps1 -DurationSeconds 300 -SampleIntervalSeconds 5
```

For the release performance gate, collect two 90-minute runs on the RTX 4060
using the same League scenario and settings:

```powershell
pwsh -NoProfile -File scripts/collect-gameplay-field-evidence.ps1 -DurationSeconds 5400 -SampleIntervalSeconds 5 -CaptureMode desktop_duplication
pwsh -NoProfile -File scripts/collect-gameplay-field-evidence.ps1 -DurationSeconds 5400 -SampleIntervalSeconds 5 -CaptureMode gdi_fallback
```

Evaluate each run with `scripts/evaluate-gameplay-field-evidence.ps1`. League
FPS baseline/capture medians, VMAF, clip-save latency, and visual inspection are
explicit inputs because Windows does not expose them reliably through the
privacy-safe process sampler. Missing inputs remain `NOT_RUN`; they never become
an implicit pass.

The collector writes `field-qa/<version>-<shortcommit>-<timestamp>/` with a redacted environment record, limited Live Client gamestats samples, process/GPU metrics, and MP4 metadata validation. It intentionally excludes LCU payloads, player names, usernames, paths, audio-device names, command lines, media contents, logs, and secrets. It is supporting evidence only, not E5 signoff: a human must still perform and record the required runs below. Its standard League-install locations and recent-MP4 scan may miss nonstandard installs or recording folders; absence is a WARN to investigate, not proof of failure.

`03_capture_audio_gpu/media-validation.csv` includes configured and measured bitrate, one-second p95/maximum bitrate, MiB per minute, configured/measured FPS, estimated dropped frames, audio start offset, and a full `ffmpeg -xerror` decode result. `highlight-labels.csv` is a blank review sheet for keep value, duplicate group, lead/tail trim, event classification, video, and audio issues. Do not tune highlight scores or timing until at least 30 rows are labeled; then run:

```powershell
pwsh -NoProfile -File scripts/analyze-highlight-labels.ps1 -InputCsv <highlight-labels.csv>
```

The analyzer only reports issue categories repeated in at least three clips and tracks the targets: keep-worthy >= 70%, duplicates <= 10%, and lead/tail trim problems <= 5%.

## Aspect-ratio Playback Fixtures

Generate the 16:9 and 43:18 five-second marker clips with:

```powershell
pwsh -NoProfile -File scripts/generate-aspect-ratio-test-videos.ps1
```

For both clips, resize the window, enter and leave fullscreen, press ESC, click the backdrop, and reveal playback controls. In the 43:18 clip, confirm the red, green, blue, and yellow corner markers remain visible in the external player, editor, home modal, and captured screenshots; black letterboxing is expected and cropping is a failure.

## NVENC 20 Mbps Candidate Gate

The candidate is `vbr`, target 20 Mbps, max 25 Mbps, buffer 40 Mbps, preset `p4`. Do not change the shipping command solely from this document. Apply it only after the same field runs show median capture FPS >= 59 at 3440x1440/60, VMAF >= 95 against the source, p95 bitrate <= 27.5 Mbps, and zero black frames, freezes, or encoder restarts. QSV and AMF use the same measurements without adding unverified vendor-specific options; H.265 and AV1 remain advanced choices.

## Tester Environment Record

Capture:

- Build version and commit.
- Tester name and date/timezone.
- Windows version.
- CPU, RAM, GPU, driver version.
- Audio devices tested.
- LoL region/client version.
- Network type.
- Installer type and artifact hash.
- Rollback build.

## Required Runs

| Area              | Required checks                                                                                     |
| ----------------- | --------------------------------------------------------------------------------------------------- |
| LoL/LCU           | LoL-first launch, app-first launch, reconnect, live game start, detectable event, game end indexing |
| Replay            | Match list, replay download, playback launch, target selection, captured replay clip                |
| Capture           | Windows 11 x64 + NVIDIA NVENC, Desktop Duplication and GDI fallback, playable output, no corrupt library entry |
| Audio             | System audio, optional microphone, device removal/change handling, sync check                       |
| Recovery          | FFmpeg kill/crash, low disk, forced app close, locked/corrupt output, relaunch cleanup              |
| YouTube           | OAuth, private/unlisted upload, token refresh/expiry, quota/API error, sign-out/token clear         |
| Installer/updater | Signed install, launch, upgrade, updater manifest, apply/restart, rollback, uninstall               |
| Support           | Export redacted diagnostics, file support issue, owner triage, privacy review                       |

## Pass Criteria

- Every shipping row in `docs/FIELD_QA_COMMERCIAL_READINESS.md` is Pass.
- Every failure has a blocker issue or documented non-shipping decision.
- Rollback path is known and tested.
- Payment remains deferred for the non-payment RC.
- Overlay is visible to the player but absent from every recorded frame.
- Game-window discovery failure continues with desktop capture and leaves a warning only in the settings status dashboard.
- LoLShorts, capture FFmpeg, and audio capture are gone within five seconds of exit.
- Every produced MP4 passes the collector's full `ffmpeg -xerror` decode.
- Formal release evidence is from Windows 11 x64 + NVIDIA NVENC. AMD, Intel,
  and CPU encoding remain explicitly experimental and do not substitute for
  the NVIDIA acceptance runs.
- Release notes are qualified to evidence level and do not claim paid/commercial readiness.

## Fail Criteria

- Any silent crash, blank screen, corrupt output, unbounded recording/export wait, hidden recovery step, unredacted diagnostic secret, local-only PRO grant, or live payment path blocks release.
