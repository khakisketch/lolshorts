# LoLShorts Production Hardening Plan

This is the current implementation plan for moving LoLShorts from a strong local candidate toward a complete production product. It is intentionally evidence-gated: automated tests can support a candidate, but they do not replace field validation on real Windows machines with League of Legends, LCU, replay files, YouTube, signed installers, updater channels, GPU and audio devices, and support dry runs.

For the end-to-end execution sequence, phase exits, and production definition of done, use [Production Completion Execution Plan](./PRODUCTION_COMPLETION_EXECUTION_PLAN.md) as the primary roadmap.

## Product authority rules

- SQLite is the local app-data store for games, clips, settings, AutoEdit output, local metadata, and one-time migration from legacy local files.
- Authentication, payment state, billing records, and PRO entitlement are never authoritative in local SQLite.
- Supabase Auth validates identity, and Supabase `user_licenses` is the canonical entitlement table.
- Toss/live billing remains deferred until all non-payment field gates pass and a separate payment QA plan is approved.
- If entitlement cannot be verified from Supabase, the app must fail closed to FREE/no paid access.

## Current implemented baseline

- Local app storage has moved toward SQLite with guarded legacy JSON migration.
- Auth/session and entitlement APIs use Supabase session validation and `user_licenses`.
- Payment commands return explicit deferred responses instead of granting PRO locally.
- Frontend PRO UI uses authoritative entitlement state, not persisted `tier` or local-only state.
- Clip export skips zero-byte, corrupt, or half-written recording segments instead of letting one bad segment poison the whole concat job.
- FFmpeg, ffprobe, encoder detection, GIF export, audio device listing, LCU WMIC fallback, and process cleanup paths now have hard timeout boundaries on the Windows RC path.
- Recording and auto-capture start/stop paths now attach and cancel the disk pressure monitor used by the segment recorder.
- Diagnostics now include runtime readiness, storage health, FFmpeg version, recording readiness, YouTube configuration, payment-deferred state, Field QA blocker state, and a redacted support bundle export command.
- Field QA and service-readiness policy documents define the release and payment blockers.
- Frontend lint, typecheck, build, Jest, runtime audit, and moderate-or-higher audit commands are available from `package.json`.

## Remaining workstreams

| Priority | Workstream                       | Required improvements                                                                                                                                                                                | Exit evidence                                                                         |
| -------- | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| P0       | Release claim control            | Keep package metadata, README, release notes, build docs, and product copy from claiming production or commercial readiness without E5 evidence.                                                     | Claim review plus updated `docs/FIELD_QA_COMMERCIAL_READINESS.md` for each candidate. |
| P0       | Auth and entitlement reliability | Add stronger tests for missing license, expired license, active PRO, Supabase outage, logout, stale cache, and cross-session recovery.                                                               | Rust and frontend tests proving fail-closed entitlement behavior.                     |
| P0       | Recording reliability            | Integrate segment verification, disk pressure monitoring, FFmpeg failure recovery, and corruption handling into the main recording lifecycle.                                                        | Automated tests for failure paths plus E5 Windows capture evidence.                   |
| P0       | Local data resilience            | Continue SQLite backup/restore or repair UX, migration idempotency tests, corrupt DB startup handling, and user-data preservation checks.                                                            | Migration tests, recovery smoke checks, and field recovery evidence.                  |
| P1       | Replay and LCU field behavior    | Validate launch order, reconnect, replay download, replay playback, target selection, and clip capture with real LoL.                                                                                | Field QA rows with logs, screenshots, and sample output files.                        |
| P1       | YouTube service readiness        | Validate real OAuth redirect, token refresh, quota/API errors, upload retry, sign-out, and privacy settings with a test account.                                                                     | Redacted OAuth/upload evidence and supportable failure messages.                      |
| P1       | Installer, signing, and updater  | Validate signed MSI/EXE, clean install, upgrade, updater manifest, update apply/restart, rollback, and uninstall.                                                                                    | Signed artifact hash, signature evidence, install/update logs, rollback notes.        |
| P1       | Support and privacy              | Make diagnostics easy to collect, keep summaries secret-safe, document log locations, and dry-run support issue routing.                                                                             | Support dry-run issue, owner assignment, and privacy-safe log bundle evidence.        |
| P2       | UX polish and accessibility      | Check loading, empty, error, offline, entitlement-refresh, payment-deferred, and text-overflow states across core screens.                                                                           | Browser screenshots or Playwright evidence plus targeted fixes.                       |
| P2       | Observability                    | Standardize structured logs, error categories, diagnostics redaction, and crash-report opt-in or policy.                                                                                             | Redaction tests and support workflow validation.                                      |
| P3       | Payment activation               | Only after non-payment field gates pass: implement server-side checkout approval, webhooks, `payments`, `subscriptions`, `user_licenses` mutation, refund/support flows, legal review, and rollback. | Separate payment QA plan, sandbox evidence, live-key approval, webhook replay tests.  |

## Candidate verification commands

Run these for every code candidate before field testing:

```bash
npm run lint
npm run typecheck
npm run build
npm run test:unit
npm run audit:runtime
npm run audit:moderate
```

Run these when backend or Tauri code changes:

```bash
cd src-tauri
cargo check
cargo test
```

Run browser E2E for UI-flow candidates:

```bash
npm run test:e2e
```

## Release rule

A build can be treated as a non-payment release candidate only when automated gates pass and the field-only checklist is completed for the target build. Payment, Toss, paid access, production webhooks, and subscription enforcement remain blocked until the non-payment field gates and a separate payment QA plan are both approved.
