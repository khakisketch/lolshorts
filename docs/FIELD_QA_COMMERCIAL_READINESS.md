# Field QA Checklist for Non-Payment Commercial Readiness

Use this checklist before enabling payment keys or treating a build as commercially ready. It separates automated and mock verification from checks that only count when a human tester runs them on a real Windows machine with League of Legends, capture hardware, YouTube access, signed installers, and recovery scenarios.

For service-readiness policy details covering installer and updater validation, support intake, diagnostics handling, privacy boundaries, and FREE or PRO product policy, see [Service Readiness Policy](./SERVICE_READINESS_POLICY.md). That policy is documentation-only and keeps payment, Toss, billing, and subscription enforcement deferred until the non-payment field gates pass.

## Release decision rule

Payment and Toss production key work stays deferred until every field-only gate below is marked **Pass** with evidence. If any required gate is **Fail**, **Blocked**, or **Not run**, do not enable payment keys, do not publish paid access, and roll back to the last build that passed this checklist.

Commercial readiness is a field-evidence decision, not a build or test-count decision. Automated tests, mocked integrations, browser E2E runs, and local smoke checks can support a candidate build, but they cannot prove that real League of Legends, LCU, replay playback, YouTube OAuth, installer updates, GPU encoding, audio capture, or support recovery workflows are ready for paid users.

## Evidence level taxonomy

Record the highest evidence level for every claim and every release gate. Do not promote a claim to a higher level until the matching evidence exists and can be inspected.

| Level | Evidence type                                            | What it can prove                                                                                                                                                                              | What it cannot prove                                                                                                               | Commercial claim status                                   |
| ----- | -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| E0    | Design, documentation, or unchecked implementation notes | Intended behavior, planned scope, or known risk                                                                                                                                                | Runtime behavior, user impact, service behavior, or readiness                                                                      | No readiness claim allowed                                |
| E1    | Automated unit evidence                                  | Function, model, validation, and command behavior under controlled inputs                                                                                                                      | Real desktop behavior, hardware behavior, network service behavior, installer behavior, or user recovery                           | May claim tested code paths only                          |
| E2    | Local mocked or integration evidence                     | Module wiring, mocked API handling, local database behavior, serialization, and error branches                                                                                                 | Real LoL, real LCU, real YouTube, signed installer, updater, GPU, audio device, or support workflow behavior                       | Must be qualified as mocked or local integration evidence |
| E3    | Browser E2E evidence                                     | Frontend navigation, visible UI flows, browser-side state, and mocked desktop bridge flows                                                                                                     | Tauri shell behavior, Windows install behavior, capture hardware, LoL client state, OAuth return handling, or updater behavior     | May claim browser flow coverage only                      |
| E4    | Local desktop smoke evidence                             | App launch, basic Tauri shell behavior, local file access, and limited smoke behavior on a developer machine                                                                                   | Cross-machine reliability, clean Windows install, real match capture, real service quotas, hardware coverage, or support readiness | May claim local smoke only, never field readiness         |
| E5    | Real field evidence                                      | Human-run validation on real Windows hardware with real LoL, LCU, replay files, YouTube test account, signed installers, updater channel, GPU and audio devices, and support workflow dry runs | Broad market readiness beyond the tested matrix                                                                                    | Required for non-payment commercial readiness decisions   |

## Commercial-readiness claim policy

Use this policy when reading README, changelog, release notes, build guides, user docs, support docs, or marketing copy. Historical phrases such as **Production Ready**, **100% complete**, automatic recording, direct YouTube upload, local processing, signed installers, or updater support are claim-risk context unless they are backed by current E5 evidence in this checklist.

| Claim category                   | Allowed before E5 field evidence                                                          | Must be qualified before E5 field evidence                                                                                                 | Blocked until E5 field evidence exists                                                                                          |
| -------------------------------- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| Automated and unit health        | Specific test results, such as test counts, build results, and lint or diagnostics status | State that results cover automated, mocked, or local code paths only                                                                       | Calling automated results commercial readiness                                                                                  |
| Local processing and privacy     | Architecture intent and code paths designed for local processing                          | Note any external service actions, OAuth, upload, telemetry, support logs, or account/payment systems separately                           | Claiming no data leaves the device for all workflows unless YouTube, auth, support, and payment paths are proven and documented |
| Real LoL and replay behavior     | Planned support for LoL, LCU, match events, and replay capture                            | State that real client and replay behavior remains field-gated until tested                                                                | Claiming live match or replay capture works for paid users without real client evidence                                         |
| YouTube OAuth and upload         | Mocked upload logic, UI coverage, quota calculations, and setup instructions              | State that production API behavior, OAuth return handling, token refresh, quota errors, and upload results need real test-account evidence | Claiming production YouTube upload readiness without real API evidence                                                          |
| Installer, signing, and updater  | Build commands, signing instructions, artifact locations, and installer test plans        | State that building or signing artifacts is not the same as clean-machine install, update, rollback, or uninstall field evidence           | Claiming signed production distribution or updater readiness without clean Windows install and update evidence                  |
| GPU and audio capture            | Encoder detection logic, settings UI, and FFmpeg command coverage                         | State which machines, GPUs, drivers, and devices have or have not been checked                                                             | Claiming hardware-accelerated or synced audio recording readiness without real GPU and audio device evidence                    |
| Support and recovery             | Draft troubleshooting steps, issue templates, and expected support paths                  | State that support workflow is unproven until a tester can reproduce, collect logs, and recover from failures                              | Claiming commercial support readiness without a dry-run support workflow and owner coverage                                     |
| Payment, Toss, and subscriptions | Deferred design notes, schema notes, local test data, and separate future QA plans        | State that payment work is intentionally blocked until non-payment field gates pass                                                        | Enabling live Toss keys, paid plans, subscription enforcement, production webhooks, or paid access                              |
| TikTok and Instagram             | Export-first compatibility notes                                                          | State that direct upload is not in scope                                                                                                   | Claiming TikTok or Instagram direct upload readiness                                                                            |

## Release and payment blockers

The following blockers must remain closed before payment or Toss production work can start. If any item is open, the release owner must keep payment keys deferred and must not publish paid access.

| Blocker                            | Closed only when                                                                                                                       | Evidence required                                                                                    |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Field-only gates incomplete        | Every field-only checklist row is **Pass** or explicitly marked non-shipping for the candidate build                                   | Completed rows with tester, date, machine, logs, screenshots, file paths, or issue links             |
| Real LoL or LCU instability        | LoL launch order, reconnect, live game detection, event capture, and end-game indexing pass                                            | Real client logs, app logs, screenshots, and sample clip metadata                                    |
| Replay capture uncertainty         | Replay list, download, playback, target selection, and replay clip capture pass                                                        | Replay file path, LoL playback evidence, selected target, and output clip sample                     |
| YouTube production API uncertainty | OAuth sign-in, upload, token refresh, quota or API error, and sign-out pass with a real test account                                   | OAuth flow notes, upload URL or video ID, privacy setting, quota response summary, and redacted logs |
| Installer or updater uncertainty   | Signed MSI or EXE installs, launches, upgrades, updates, restarts, rolls back, and uninstalls on clean Windows                         | Installer hash, signature screenshot, updater manifest, version screenshots, and install logs        |
| GPU or audio uncertainty           | At least one supported encoder path, software fallback, system audio, microphone behavior, device changes, and long capture pass       | Machine specs, driver version, audio device list, encoder logs, and sample media                     |
| Support workflow uncertainty       | A tester can find logs, file an issue, follow recovery guidance, preserve user data, and route a support case to an owner              | Support dry-run notes, issue link, log bundle path, owner name, and recovery result                  |
| Overbroad documentation claims     | Claim-risk docs are either updated in a later documentation phase or explicitly covered by release notes that qualify evidence level   | Follow-up issue or doc task link, plus release-owner approval                                        |
| FFmpeg sidecar integrity           | Release build includes usable `ffmpeg` and `ffprobe` sidecars, not broken shims, and the build gate verifies `-version`                | Build log showing sidecar copy and verification, artifact path, and runtime diagnostics screenshot   |
| Payment readiness gap              | Legal, account setup, refund/support paths, webhook QA, sandbox-to-live checklist, and rollback plan are ready for separate payment QA | Separate payment QA plan and owner sign-off after all non-payment field gates pass                   |

## Current automated evidence

These gates show code and automation health only. They do not prove real client, hardware, service, updater, or recovery behavior.

Refresh this table for each candidate build. Do not copy older counts into a release note unless the command has been rerun for that exact candidate.

| Gate                         | Latest known result                                                                                                                                                                                                                                              | Evidence source              | Commercial meaning                                                                                                                                                  |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Release preflight automation | `npm run verify:release-preflight` passed on 2026-05-09 with 0 fail, 2 warn, and 2 skipped; report written to `qa-evidence/release-preflight-20260509-151210/release-preflight.md`                                                                               | Local automated verification | Produces reviewable local evidence only; it does not replace E5 field evidence                                                                                      |
| Frontend aggregate gate      | Component commands were rerun separately on 2026-05-09: lint, typecheck, build, and `test:unit` all passed                                                                                                                                                       | Local automated verification | Confirms frontend automated health only                                                                                                                             |
| Frontend lint                | `npm run lint` passed with no warnings on 2026-05-09                                                                                                                                                                                                             | Local automated verification | Supports code hygiene only                                                                                                                                          |
| TypeScript typecheck         | `npm run typecheck` passed on 2026-05-09                                                                                                                                                                                                                         | Local automated verification | Confirms frontend types only                                                                                                                                        |
| Frontend build               | `npm run build` passed on 2026-05-09                                                                                                                                                                                                                             | Local automated verification | Confirms packaged frontend can compile                                                                                                                              |
| Jest unit/integration tests  | `npm run test:unit` passed 23 suites / 244 tests on 2026-05-09; existing React `act(...)` console warnings and expected error logs remain test-noise to track separately                                                                                         | Local automated verification | Confirms mocked and unit behavior only                                                                                                                              |
| Rust check/tests             | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passed on 2026-05-09; `cargo test` covered 389 passed / 4 ignored unit tests, 2 main tests, 1 ffmpeg integration test, 42 passed / 2 ignored integration tests, and doc tests | Local automated verification | Confirms backend compile and unit behavior only                                                                                                                     |
| Runtime dependency audit     | `npm run audit:runtime` passed with 0 runtime vulnerabilities on 2026-05-05                                                                                                                                                                                      | Local automated verification | Confirms dependency audit state only                                                                                                                                |
| Moderate-or-higher audit     | `npm run audit:moderate` exited 0 on 2026-05-05; 4 low-severity dev-only Jest/jsdom advisories remain and require a breaking upgrade to eliminate                                                                                                                | Local automated verification | Does not prove runtime safety, service readiness, or field readiness                                                                                                |
| Playwright browser E2E       | `npm run test:e2e` passed on 2026-05-05 across Desktop Chrome, Firefox, and Edge: 342 passed / 27 skipped / 0 failed.                                                                                                                                            | Local browser automation     | Supports mocked browser-flow coverage only; it does not prove Tauri shell, real LoL/LCU, YouTube API, installer, updater, GPU, audio, or support workflow readiness |
| Paid billing scaffold        | Supabase migration and `billing` Edge Function now define a server-authoritative Toss path, but live enablement remains blocked by E5 and paid QA evidence.                                                                                                      | Local code review            | Does not authorize live Toss keys, paid access, production webhooks, or public PRO sales                                                                            |

## Field QA environment record

Complete this section for each candidate build.

| Field                                       | Value            |
| ------------------------------------------- | ---------------- |
| Build version and commit                    |                  |
| Tester                                      |                  |
| Date and timezone                           |                  |
| Windows version                             |                  |
| CPU, RAM, GPU, driver version               |                  |
| Audio devices tested                        |                  |
| League of Legends region and client version |                  |
| Network type                                |                  |
| Installer type tested                       | MSI / EXE / both |
| Previous build used for rollback            |                  |

## Field-only checklist

Use **Pass**, **Fail**, **Blocked**, or **Not run**. Evidence should include logs, screenshots, file paths, video samples, API response summaries, or issue links. Do not paste secrets, OAuth tokens, payment keys, or private user data.

### 1. Real LoL client and LCU

| Check                                                                           | Expected result                                                 | Status | Evidence | Blocker or rollback action                                |
| ------------------------------------------------------------------------------- | --------------------------------------------------------------- | ------ | -------- | --------------------------------------------------------- |
| Launch LoL client first, then LoLShorts                                         | App detects the running client and shows connected LCU state    |        |          | Block release if LCU connection is unstable               |
| Launch LoLShorts first, then LoL client                                         | App reconnects without restart or gives clear recovery guidance |        |          | File blocker if reconnect needs manual hidden steps       |
| Start a live game                                                               | App detects game start and recording readiness                  |        |          | Roll back if live game state is missed                    |
| Trigger at least one detectable event in a real match or controlled custom game | Event appears in app state and saved clip metadata              |        |          | Block release if events are missing or mapped incorrectly |
| End the game                                                                    | Recording stops cleanly and files are indexed                   |        |          | Roll back if files are corrupt, missing, or duplicated    |

### 2. Replay download and playback

| Check                                   | Expected result                                          | Status | Evidence | Blocker or rollback action                             |
| --------------------------------------- | -------------------------------------------------------- | ------ | -------- | ------------------------------------------------------ |
| Open replay list from a real account    | Recent matches load or unsupported state is explained    |        |          | Block release if the UI silently fails                 |
| Download one available replay           | Download completes and app records the replay file state |        |          | Block release if progress or failure state is unclear  |
| Launch replay playback                  | LoL client opens playback from the app path              |        |          | Roll back if playback cannot start on a clean machine  |
| Select target player for replay capture | Target choice is applied and visible to the tester       |        |          | Block if wrong player is captured without warning      |
| Capture a replay clip                   | Clip contains the intended replay video and usable audio |        |          | Roll back if replay clips are blank, frozen, or silent |

### 3. Windows capture hardware, audio, and GPU

| Check                                                            | Expected result                                                      | Status | Evidence | Blocker or rollback action                                       |
| ---------------------------------------------------------------- | -------------------------------------------------------------------- | ------ | -------- | ---------------------------------------------------------------- |
| Record on NVIDIA, AMD, or Intel hardware available to the tester | Hardware encoder detection matches the machine or falls back cleanly |        |          | Block only if no usable encoder path exists                      |
| Record with software fallback                                    | Recording still works when hardware encoding is unavailable          |        |          | Roll back if fallback output is corrupt                          |
| Capture system audio                                             | Game audio is present and synced in the output clip                  |        |          | Block if default audio path is silent                            |
| Capture microphone audio, if enabled                             | Microphone is present only when enabled and levels are usable        |        |          | Block if privacy setting is ignored                              |
| Change audio device during or between recordings                 | App handles the change or gives clear retry guidance                 |        |          | File blocker if device loss causes crash or corrupt state        |
| Record at target quality for at least one full game or replay    | Output remains playable, indexed, and within expected disk usage     |        |          | Roll back if long capture fails or exhausts disk without warning |

### 4. YouTube OAuth and API

| Check                                         | Expected result                                                          | Status | Evidence | Blocker or rollback action                                |
| --------------------------------------------- | ------------------------------------------------------------------------ | ------ | -------- | --------------------------------------------------------- |
| Sign in with a real test Google account       | OAuth browser flow returns to the app and stores auth state securely     |        |          | Block release if sign-in cannot complete                  |
| Upload a small private or unlisted test video | Upload completes and YouTube shows the expected title, privacy, and file |        |          | Disable YouTube feature if upload fails in production API |
| Refresh or expire token path                  | App refreshes token or prompts for sign-in without data loss             |        |          | Block if stale token causes repeated failed calls         |
| Quota or API error path                       | App shows actionable error without losing local exports                  |        |          | Block if errors are hidden or destructive                 |
| Sign out                                      | Stored YouTube auth state is cleared from the app                        |        |          | Block if account remains connected after sign-out         |

### 5. Updater signing and distribution

| Check                                                     | Expected result                                                              | Status | Evidence | Blocker or rollback action                                 |
| --------------------------------------------------------- | ---------------------------------------------------------------------------- | ------ | -------- | ---------------------------------------------------------- |
| Install signed MSI on a clean Windows machine             | Installer identity, publisher, and install path are correct                  |        |          | Block release if unsigned build is presented as production |
| Install signed EXE on a clean Windows machine, if shipped | Installer completes without unsafe warnings beyond expected platform prompts |        |          | Block if installer cannot complete                         |
| Launch after install                                      | App starts from Start Menu or desktop shortcut                               |        |          | Roll back if fresh install cannot launch                   |
| Upgrade from previous production build                    | User data remains available and new version starts                           |        |          | Roll back if upgrade loses clips, settings, or auth state  |
| Updater manifest check                                    | App detects available update from the expected distribution channel          |        |          | Block if updater points at a placeholder or wrong channel  |
| Update apply and restart                                  | Updated app restarts into the expected version                               |        |          | Roll back if update loop or signature failure occurs       |
| Uninstall                                                 | App uninstalls cleanly and leaves only documented user data                  |        |          | File blocker if uninstall breaks future installs           |

### 6. Failure recovery

| Check                                                 | Expected result                                                   | Status | Evidence | Blocker or rollback action                          |
| ----------------------------------------------------- | ----------------------------------------------------------------- | ------ | -------- | --------------------------------------------------- |
| Kill FFmpeg or recording process during capture       | App stops safely, reports failure, and can start a new recording  |        |          | Roll back if recovery requires manual file deletion |
| Lose network during YouTube upload                    | App reports upload failure or retry state without deleting export |        |          | Block if exported video is lost                     |
| Fill or nearly fill target disk                       | App warns, stops safely, or preserves previous clips              |        |          | Block if disk pressure corrupts library state       |
| Remove or disable audio device                        | App reports device issue and continues without crash              |        |          | Block if crash or silent bad output occurs          |
| Corrupt or lock local database copy in a test profile | App provides recovery path or safe error state                    |        |          | Roll back if startup becomes unrecoverable          |
| Force app close during processing                     | Relaunch does not leave stuck jobs or broken library entries      |        |          | Block if user data needs unsupported repair         |

### 7. Support workflow readiness

| Check                                                                                          | Expected result                                                                                                     | Status | Evidence | Blocker or rollback action                                |
| ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ------ | -------- | --------------------------------------------------------- |
| Locate app logs and diagnostic files on a tester machine                                       | Tester can find the documented logs without developer-only knowledge                                                |        |          | Block if support cannot collect evidence from users       |
| File a field issue from a failed or simulated failed run                                       | Issue contains version, machine, LoL state, reproduction steps, logs, screenshots, and sample paths without secrets |        |          | Block if support reports are incomplete or expose secrets |
| Follow documented recovery guidance for one recording, YouTube, installer, or database failure | Tester can recover or reach a clear hold state without unsupported manual repair                                    |        |          | Roll back if recovery requires hidden developer steps     |
| Verify user-facing support route and owner coverage                                            | GitHub Issues, email, or other support route has an owner and expected response path                                |        |          | Block if paid users would have no support path            |
| Confirm privacy-safe evidence handling                                                         | Logs and attachments avoid OAuth tokens, payment keys, private user data, and unrelated local files                 |        |          | Block if support workflow risks secret or privacy leakage |

## Payment and Toss keys remain deferred

Do not configure live Toss keys, paid plans, subscription enforcement, or production payment webhooks during this checklist. The codebase may contain disabled server-authoritative billing scaffolding for review, but `LOLSHORTS_PAYMENT_ENABLED` must remain unset or false until the release owner confirms every field-only row has passing evidence, every blocker has an issue or fix, rollback has been tested, and legal or account setup requirements are complete.

Payment work is a separate later gate. Passing automated tests, producing a build, signing an installer, or completing local smoke checks does not authorize live Toss keys or paid access. The release owner must preserve this deferral until the E5 field evidence above exists for the non-payment product and the support workflow is ready for real users.

## Final sign-off

| Role              | Name | Decision                                                       | Notes |
| ----------------- | ---- | -------------------------------------------------------------- | ----- |
| Field tester      |      | Pass / Fail / Blocked                                          |       |
| Engineering owner |      | Pass / Fail / Blocked                                          |       |
| Release owner     |      | Enable non-payment release / Roll back / Hold                  |       |
| Payment owner     |      | Payment keys still deferred / Approved for separate payment QA |       |

If the decision is **Roll back** or **Hold**, record the build to use, the reason, and the user-facing mitigation. Do not describe the build as field QA complete until this file has real evidence filled in by a human tester.
