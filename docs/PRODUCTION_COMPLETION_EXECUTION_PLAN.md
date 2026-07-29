# LoLShorts Production Completion Execution Plan

This plan defines the remaining work required to move LoLShorts from a validated local/browser candidate to a production-grade Windows release. It intentionally does not treat automated test success as production completion. Production completion requires E5 field evidence on real Windows machines with League of Legends, LCU, replay files, FFmpeg/GPU/audio devices, YouTube test account, signed installer/updater, and a support workflow dry run.

## Current State, 2026-05-05

### Confirmed Local Evidence

- Frontend lint, typecheck, production build, Jest, browser E2E, runtime audit, moderate-or-higher audit, `cargo check`, and `cargo test` have passed locally.
- Browser E2E now runs against `127.0.0.1:5181` with two workers and the latest full run passed: 342 passed / 27 skipped / 0 failed across Desktop Chrome, Firefox, and Edge.
- Browser UI smoke on Dashboard found and fixed translation-key exposure, status badge overflow, title focus outline, and LCU header wrapping.
- Payment and Toss remain disabled by default. A server-authoritative Supabase billing scaffold exists for review, but no live payment key should be configured before E5 and paid QA pass.
- Supabase Auth and `user_licenses` remain the entitlement authority; local SQLite is app-data storage only.

### Not Yet Proven

- Real League of Legends and LCU behavior.
- Replay download, playback, target selection, and replay capture on a real account.
- Long-running Windows capture with real GPU/audio devices and device-change failures.
- Real YouTube OAuth redirect, token refresh, upload, quota errors, sign-out, and privacy setting behavior.
- Signed installer, updater, upgrade, rollback, and uninstall behavior on a clean Windows machine.
- Support workflow with redacted diagnostics bundle collected by a non-developer tester.
- Cross-machine stability and commercial support readiness.

## Production Definition of Done

A build is production-complete only when all of the following are true:

1. All automated gates pass for the exact release candidate commit.
2. The Field QA checklist has Pass evidence for every shipping E5 row.
3. Release claims are reviewed and do not exceed available evidence.
4. A signed installer and updater path are validated on clean Windows.
5. A rollback build is identified and tested.
6. Payment remains deferred, or a separate payment QA plan has passed after non-payment Field QA.
7. Support can collect a redacted diagnostics bundle and route a real issue to an owner.

## Execution Phases

### Phase 0: Worktree Stabilization

Goal: make the current broad dirty tree reviewable and prevent accidental regressions.

| Task                                              | Output                                                                                             | Verification                                   |
| ------------------------------------------------- | -------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| Split current changes into reviewable workstreams | Branch or PR set for auth/billing, recording reliability, diagnostics, UI/E2E, docs/release policy | `git diff --name-only` reviewed per workstream |
| Preserve unrelated user or generated changes      | No unrelated revert or overwrite                                                                   | Manual status review                           |
| Record latest automated evidence                  | Updated `docs/FIELD_QA_COMMERCIAL_READINESS.md`                                                    | Evidence table matches latest commands         |
| Decide release-candidate commit boundary          | Candidate commit hash and rollback baseline                                                        | Release owner sign-off                         |

Exit condition: the team can review and ship changes without relying on one unbounded diff.

### Phase 1: Local Production Hardening

Goal: finish failure-safe behavior that can be developed and tested locally before field testing.

| Workstream                     | Required Work                                                                                                                                               | Evidence                                   |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| Entitlement fail-closed        | Expand frontend and Rust regression tests for missing license, expired license, malformed expiry, Supabase outage, logout, stale cache, and session restore | Unit tests and mocked browser E2E          |
| SQLite resilience              | Add visible recovery states for corrupt DB, backup/restore guidance, migration idempotency, and user-data preservation checks                               | Rust storage tests and UI smoke            |
| Process reliability            | Keep all FFmpeg/ffprobe/device/process calls under timeout and cancellation; add tests for timeout/crash paths where practical                              | Rust tests and diagnostic state checks     |
| Recording recovery             | Verify stuck job cleanup, locked output handling, corrupt segment skip, low disk handling, and forced-close relaunch behavior                               | Rust tests plus desktop smoke              |
| Diagnostics and support bundle | Validate redaction, include app version/storage health/FFmpeg/readiness/payment-deferred state, and expose user-facing export path                          | Redaction tests and browser UI             |
| Browser UX states              | Cover loading, empty, error, offline, entitlement-refresh, payment-deferred, YouTube-disabled, diagnostics-unavailable, and text-overflow states            | Playwright screenshots and component tests |

Exit condition: local automated and browser evidence is green with no known fail-open auth/payment paths and no indefinite external process waits.

### Phase 2: E4 Desktop Smoke

Goal: prove the app shell works on a developer Windows desktop before real field QA.

| Task                               | Output                                                                  | Verification                  |
| ---------------------------------- | ----------------------------------------------------------------------- | ----------------------------- |
| Launch Tauri desktop app           | App opens without browser-only mocks                                    | Manual smoke notes and logs   |
| Open every primary route           | Dashboard, Games, Replays, Editor, AutoEdit, Results, YouTube, Settings | Screenshot set                |
| Export diagnostics bundle          | Redacted bundle path and sample JSON                                    | Manual inspection for secrets |
| Validate local storage lifecycle   | Create local settings/game/clip metadata, restart, verify persistence   | Desktop smoke notes           |
| Validate graceful degraded startup | Simulate missing FFmpeg, missing Supabase env, disabled YouTube config  | UI error/degraded screenshots |

Exit condition: app works as a desktop app, not only as a mocked browser SPA.

### Phase 3: E5 Field QA For Non-Payment Windows RC

Goal: prove real product readiness for a non-payment Windows release candidate.

| Area              | Required Evidence                                                                                             |
| ----------------- | ------------------------------------------------------------------------------------------------------------- |
| LoL and LCU       | Launch-order tests, reconnect, live game detection, event mapping, end-game indexing, logs/screenshots        |
| Replay            | Real match list, replay download, playback launch, target selection, captured replay clip sample              |
| Capture hardware  | GPU encoder path or software fallback, full-game recording, output clip playback, encoder logs                |
| Audio             | System audio, microphone optional behavior, device removal/change handling, sync verification                 |
| Disk and recovery | Low-disk scenario, FFmpeg kill/crash, forced app close, restart recovery, no broken library entries           |
| YouTube           | Real test account OAuth, private/unlisted upload, token refresh/expiry, quota/API error, sign-out/token clear |
| Installer/updater | Signed MSI/EXE install, launch, upgrade, update apply/restart, rollback, uninstall on clean Windows           |
| Support workflow  | Tester exports diagnostics, files an issue without secrets, follows recovery guidance, owner triages          |

Exit condition: every required row in `docs/FIELD_QA_COMMERCIAL_READINESS.md` is Pass or explicitly non-shipping for the candidate.

### Phase 4: Release Claim and Distribution Gate

Goal: prevent overclaiming and publish only what is proven.

| Task                        | Output                                                             | Verification                                             |
| --------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------- |
| Claim review                | README, release notes, website copy, updater/release body reviewed | No "production-ready" or paid-readiness claim without E5 |
| Draft/prerelease release    | Release artifacts are draft/prerelease until approval              | GitHub release settings                                  |
| Updater manifest validation | Manifest points to the correct signed artifact and signature       | Updater smoke                                            |
| Rollback procedure          | Known-good build and rollback instructions                         | Release owner dry run                                    |

Exit condition: release notes state exactly what is proven and what is deferred.

### Phase 5: Payment Activation, Separate Future Track

Payment is blocked until the non-payment Windows RC passes. When it starts, it needs a separate plan:

| Required Work             | Evidence                                                                                    |
| ------------------------- | ------------------------------------------------------------------------------------------- |
| Server-side Toss approval | Sandbox and live approval logs, no client-only grant path through Tauri, browser routes, Zustand, localStorage, or SQLite |
| Webhook idempotency       | Replay tests for duplicate, out-of-order, failed, refund, cancel events against the Supabase `billing` Edge Function       |
| Canonical DB mutation     | `payments`, `subscriptions`, and `user_licenses` updated only by the server-authoritative billing function or equivalent central server |
| Legal/support readiness   | Terms, refund flow, support routing, tax/account owner, rollback plan                       |
| Paid launch rollback      | Disable payment, revoke/restore licenses safely, communicate user impact                    |

Exit condition: paid access is proven separately from non-payment product readiness.

## Immediate Next Work Order

1. Stabilize the current dirty tree into reviewable workstreams using [Workstream Stabilization Record](./WORKSTREAM_STABILIZATION.md).
2. Run a desktop Tauri smoke pass and capture E4 evidence using [E4 Desktop Smoke Packet](./E4_DESKTOP_SMOKE_PACKET.md).
3. Add or tighten local tests for startup recovery, SQLite corruption guidance, diagnostics redaction, and recording recovery.
4. Expand Playwright screenshots beyond Dashboard to Settings, YouTube, AutoEdit, Replays, and degraded/payment-deferred states.
5. Prepare and execute [E5 Field QA Packet](./E5_FIELD_QA_PACKET.md): tester instructions, evidence folders, log bundle procedure, sample issue template, and pass/fail criteria.
6. Execute E5 on real Windows hardware and record evidence in the checklist.
7. Only after E5 passes, promote a non-payment Windows RC. Keep payment disabled unless the separate paid QA plan also passes.
8. For paid launch, deploy the Supabase billing function with service-role/Toss secrets server-side only, run Toss sandbox QA, webhook replay QA, refund/cancel QA, and one controlled live small-amount test with rollback evidence.

## Standard Verification Set

Run for every release-candidate commit:

```bash
npm run lint
npm run typecheck
npm run build
npm run test:unit
npm run test:e2e
npm run audit:runtime
npm run audit:moderate
cd src-tauri
cargo check
cargo test
```

## Non-Negotiable Release Rules

- Do not call the product production-ready until E5 field evidence exists.
- Do not enable `LOLSHORTS_PAYMENT_ENABLED` or live Toss/payment keys before non-payment Field QA and separate paid QA pass.
- Do not trust SQLite, Zustand, localStorage, or UI state for auth, payment, or PRO entitlement authority.
- Do not ship direct TikTok or Instagram upload claims; keep export/manual guidance only.
- Do not publish updater or signing readiness claims without clean-machine install/update evidence.
- Do not accept hidden recovery steps as support-ready behavior.
