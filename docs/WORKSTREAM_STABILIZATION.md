# Workstream Stabilization Record

This record keeps the current broad dirty tree reviewable without reverting unrelated user changes. It is a release-candidate coordination artifact, not a production-readiness claim.

## Workstream Split

| Workstream              | Primary intent                                                                                        | Representative paths                                                                                                 |
| ----------------------- | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `auth-entitlement`      | Keep Supabase Auth and `user_licenses` authoritative; fail closed to FREE on verification uncertainty | `src/lib/auth.ts`, `src/api/auth.ts`, `src-tauri/src/auth`, `src-tauri/src/supabase`, `supabase/`                    |
| `recording-reliability` | Bound FFmpeg/ffprobe/device processes, disk pressure, capture recovery, replay and segment robustness | `src-tauri/src/recording`, `src-tauri/src/video`, `src-tauri/src/lcu`, `src-tauri/src/utils/ffmpeg*`                 |
| `diagnostics-support`   | Expose readiness state and secret-safe support bundles                                                | `src-tauri/src/utils/commands.rs`, `src-tauri/src/storage`, `src/api/utils.ts`, `src/components/StatusDashboard.tsx` |
| `ui-e2e`                | Stabilize browser E2E and verify degraded UX states                                                   | `playwright.config.ts`, `tests/e2e`, `src/pages`, `src/components`                                                   |
| `release-docs`          | Keep release claims evidence-gated and payment deferred                                               | `.github/workflows/release.yml`, `README.md`, `docs/*READINESS*`, `docs/*PRODUCTION*`, release notes                 |

## Unrelated or Review-Separate Buckets

- `.omc/`, `.serena/`, and `.claude/` deletions are treated as pre-existing local/tooling cleanup unless separately approved.
- `output/`, `.playwright-mcp/`, `test-results/`, and `playwright-report/` are local verification artifacts and must not be included in release commits.
- Social direct-upload removals for TikTok/Instagram are aligned with export-only readiness, but should be reviewed separately from YouTube readiness.

## Stabilization Rules

- Do not revert files just because they are dirty; preserve user or prior-agent changes unless they block the requested work.
- Stage or review by workstream, not by broad `git add .`.
- Every workstream must list the last command set that passed before it is considered release-candidate material.
- Any claim touching production/commercial readiness must point to `docs/FIELD_QA_COMMERCIAL_READINESS.md` evidence.

## Current Local Evidence Snapshot

- `npm run lint`: passed on 2026-05-05.
- `npm run typecheck`: passed on 2026-05-05.
- `npm run build`: passed on 2026-05-05.
- `npm run test:unit`: passed 23 suites / 243 tests on 2026-05-05.
- `npm run test:e2e`: passed 333 / skipped 27 / failed 0 across Chrome, Firefox, and Edge on 2026-05-05.
- `npm run audit:runtime`: 0 vulnerabilities on 2026-05-05.
- `npm run audit:moderate`: exit 0 with 4 low-severity dev-only Jest/jsdom advisories on 2026-05-05.
- `cargo check` and `cargo test`: passed on 2026-05-05.

## Next Review Order

1. Review `release-docs` first to ensure no overclaiming or payment activation.
2. Review `auth-entitlement` before any PRO-gated UI or YouTube changes.
3. Review `diagnostics-support` before E4/E5 testing so testers can export redacted evidence.
4. Review `recording-reliability` before real LoL/LCU/capture Field QA.
5. Review `ui-e2e` last as the user-visible regression net.
