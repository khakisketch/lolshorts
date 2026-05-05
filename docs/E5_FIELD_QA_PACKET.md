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
| Capture           | Hardware encoder or fallback, full-game/replay recording, playable output, no corrupt library entry |
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
- Release notes are qualified to evidence level and do not claim paid/commercial readiness.

## Fail Criteria

- Any silent crash, blank screen, corrupt output, unbounded recording/export wait, hidden recovery step, unredacted diagnostic secret, local-only PRO grant, or live payment path blocks release.
