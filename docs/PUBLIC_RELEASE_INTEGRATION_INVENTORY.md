# Public Release Integration Inventory

Snapshot date: 2026-08-21 (Asia/Seoul)

## Safety boundary

- Working branch: `codex/public-release-readiness`
- Clean-validated source snapshot branch:
  `codex/safety-pre-game-final-20260821-v7`
- Clean-validated snapshot commit:
  `115d23642e566fa130db3e47e0fb5028c5802b97`
- The safety lineage was created with separate Git indexes and an owned
  temporary worktree. The developer's existing staged and unstaged state was
  not reset, checked out, or discarded.
- Generated `target-*`, `node_modules`, local `.env*`, evidence, and large
  sidecar binaries were excluded. They are reproducible or private local input,
  not authoritative source.
- The local branch remains ten commits ahead of the public `origin/master`
  baseline. Nothing was pushed and no GitHub release was published by this work.

## Integration groups

1. Recording and lifecycle: Windows capture diagnostics, overlay fail-closed
   exclusion, five-second scoped shutdown, media validation, replay/clip paths.
2. Media workflow: durable auto-edit/export jobs, output validation, external
   media staging, thumbnail and library presentation.
3. User experience: readiness onboarding, storage forecasting, desktop-fallback
   privacy warning, Windows startup synchronization, free-account feature gates.
4. Public services and distribution: compile-time public service configuration,
   secret-safe status reporting, least-privilege overlay capability, signed
   beta/RC/stable workflow and updater manifests.
5. Verification: scoped lint, pinned Node/npm/Rust runtimes, browser contract
   alignment, media regression, field evidence collection and quality gates.
6. Auth and dependency security: email-confirmation handoff, renderer-to-Rust
   session refresh/sign-out synchronization, RLS ownership checks and
   least-privilege column grants, OAuth2 5 PKCE/redirect hardening, and pinned
   npm/RustSec audits covering the release lockfiles.

## Current game-independent baseline

- `npm run verify:non-game-readiness` passed in a clean checkout without any
  `.env` file on 2026-08-21; Git status remained clean after the gate.
- Frontend: lint, typecheck, production build, and 57 Jest suites / 473 tests
  passed. The added configuration contract prevents missing development
  Supabase values from blanking the app while production still fails closed.
- Backend: Rust formatting and warning-denying Clippy passed; 777 tests passed
  and 6 field/performance-oriented tests were intentionally ignored.
- Media and browser contracts: the real FFmpeg regression passed; Playwright
  passed 120 tests with 9 intentional skips across Chrome plus the Firefox and
  Edge core-navigation checks.
- Desktop shell: `npm run tauri:build:debug` builds the complete application
  executable with validated, generated FFmpeg/ffprobe sidecars, without
  producing an unsigned installer that could be mistaken for a release
  artifact. The clean no-bundle executable was 54,684,160 bytes.
- Sidecar bootstrap: development `Auto` mode validates a matching local
  FFmpeg/ffprobe build pair; release workflows use an immutable BtbN archive
  and verify its pinned SHA-256 before extraction. Caller working directory and
  package-manager shims no longer affect the result.
- Service and dependency contracts: 8 Supabase quota tests passed; full npm and
  RustSec audits reported 0 vulnerabilities; the release configuration and
  least-privilege Tauri capability contract passed.
- Field evidence tooling: synthetic fixtures prove that missing app/FFmpeg
  samples remain `NOT_RUN`, concurrent samples are evaluated, fewer than 30
  labels are rejected, and the exact 30-clip quality threshold is enforced.
- Dirty-checkout browser validation passed 120 tests with 9 intentional skips
  after Vite's Windows watcher was restricted from Rust targets, generated
  QA evidence, and Playwright output. This prevents large local build trees
  from exhausting file handles and stalling the dev/E2E HTTP server.
- Release preflight finished with 0 failures, 2 expected warnings (League was
  closed and no installer artifact exists yet), and 2 expected skips (real
  YouTube redirect and installer validation). Its reviewable report is
  `qa-evidence/release-preflight-20260821-173751/release-preflight.md`.

These results certify only the current game-independent candidate baseline.
They do not convert any unrun E5 field row into a pass.

## Latest dirty-checkout verification

Validation rerun on 2026-08-27 (Asia/Seoul) after the final integration fixes:

- Frontend lint, typecheck, production build, formatting, and Jest passed: 61
  suites / 477 tests. The generated Rust/build trees remain outside the lint
  scope.
- Rust formatting, warning-denying Clippy, and the full test matrix passed:
  738 library tests passed with 3 intentional ignores; all auxiliary binaries
  and integration targets passed with only their documented ignored cases.
- The real FFmpeg media regression, release-contract check, field-evidence
  tool tests, npm runtime audit, and RustSec audit passed. RustSec still reports
  only the documented 17 allowed unmaintained warnings and no vulnerabilities.
- CI installer jobs now use the pinned project-local Tauri CLI and the current
  `TAURI_SIGNING_PRIVATE_KEY*` variables. The normal CI build creates an
  ephemeral updater key so it does not depend on production secrets; only the
  production release workflow consumes the protected signing values.
- The development server was intentionally left stopped. Playwright/Chromium,
  Firefox, and Edge E2E was not rerun in this turn because that suite starts a
  local server; the existing browser results above remain historical evidence,
  not a new current-checkout pass.

This rerun is still game-independent. Live League/LCU, Desktop Duplication/GDI
capture, GPU/audio measurements, YouTube OAuth/upload, signed installer update
and rollback, and the two-machine E5 packet remain external field gates.

## Deliberately external release blockers

- Production Supabase, YouTube desktop OAuth, Authenticode, and Tauri updater
  signing values must be installed in the protected GitHub environment.
- A signed `1.2.0-beta.1 -> 1.2.0-rc.1 -> 1.2.0` artifact sequence requires
  version changes and real GitHub Actions runs; the workflow fails closed when
  configuration is absent.
- Desktop Duplication and GDI fallback each require a fresh 90-minute RTX 4060
  run. Missing League FPS, VMAF, clip latency, or visual evidence is `NOT_RUN`,
  never an implicit pass.
- At least 30 clips from two games must be labelled. Ranking/timing changes are
  allowed only for an issue repeated in at least three clips.
- Stable publication requires complete E5 rows on the current RTX 4060 machine
  and a second clean Windows 11 + NVIDIA machine. Until then the channel is
  `public preview` only.

## Known non-blocking maintenance debt

- `npm ci` reports deprecated packages only through the Jest 29/jsdom 20 and
  Tailwind 3/Sucrase development toolchains. The full npm audit reports zero
  vulnerabilities. Removing those warnings requires coordinated major test or
  CSS-toolchain migrations and is intentionally separated from the gameplay RC.
- RustSec reports zero vulnerabilities and 17 allowed warnings, primarily the
  unsupported Linux GTK3 dependency graph plus unmaintained transitive parsing
  crates. The one GTK soundness advisory is not compiled into the supported
  Windows target; it remains a blocker to claiming Linux support.
- Semver-compatible and major npm upgrades remain a post-E5 maintenance batch.
  Updating router, Radix, Playwright, React, Vite, Jest, or Tailwind immediately
  before field capture would create unrelated UI and tooling churn despite a
  zero-vulnerability lockfile.

## Commit boundary recommendation

Do not mechanically split the current mixed index with reset. After automated
and field validation, commit by the five integration groups above using explicit
path lists or a fresh worktree based on the safety snapshot. Preserve the
original staged paths until their ownership is reviewed.
