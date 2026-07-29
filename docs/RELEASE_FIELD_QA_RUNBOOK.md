# Release Field QA Runbook

This runbook turns a candidate build into reviewable evidence. It does not make
the build production-ready by itself. Public readiness still requires the E5
field evidence in [FIELD_QA_COMMERCIAL_READINESS.md](./FIELD_QA_COMMERCIAL_READINESS.md).

## 1. Local automated preflight

Run this before asking a human tester to spend time on real League of Legends,
YouTube, installer, updater, GPU, or audio checks.

```powershell
npm run verify:release-preflight
```

The command writes a timestamped report under `qa-evidence/`. That directory is
gitignored because reports can contain local paths, machine details, and command
logs.

For a quick script smoke test that skips long build and test commands:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-field-qa-preflight.ps1 -SkipCommandChecks -OutputDir "$env:TEMP\lolshorts-qa"
```

Expected result:

- No `FAIL` rows.
- `WARN` rows are acceptable only when they describe missing field-only inputs,
  such as League Client not running or release installers not built yet.
- `SKIP` rows must be resolved before release sign-off unless the related
  product area is explicitly non-shipping for that candidate.

## 2. Installer artifact validation

After a release build creates MSI or NSIS artifacts, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tests/installer/validate-installer.ps1 -NonInteractive
```

To include installer validation in the preflight report:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-field-qa-preflight.ps1 -RunInstallerValidation
```

Use `-RunSilentInstall` with `tests/installer/validate-installer.ps1` only on a
machine where installing and uninstalling the candidate build is intended.

## 3. Supabase staging migration gate

The preflight script statically checks production-facing SQL for:

- passworded bootstrap roles
- broad exposed-role grants to `anon`, `authenticated`, or `public`
- `SECURITY DEFINER` functions without matching `SET search_path` guardrails
- missing RLS statements in schema-bearing files

Before applying to a real project:

1. Run `supabase --help` and the relevant subcommand `--help` on the installed
   Supabase CLI version.
2. Apply migrations to a staging project first, never directly to production.
3. Confirm exposed tables have both appropriate grants and matching RLS
   policies. RLS controls rows; grants still control whether the Data API can
   see the table.
4. Run Supabase advisors or the dashboard security advisor against staging.
5. Attach staging migration logs and advisor output to the field QA evidence.

Current Supabase changelog context: newer Supabase projects may require explicit
grants for API exposure even when RLS policies exist, so staging validation must
check both.

## 4. Real LoL and recording field test

Use [FIELD_QA_COMMERCIAL_READINESS.md](./FIELD_QA_COMMERCIAL_READINESS.md) as
the evidence form. Required minimum for MVP sign-off:

- League Client before app and app before League Client launch orders.
- Live game start detected.
- At least one real or controlled custom-game event creates clip metadata.
- Game end stops recording and indexes files.
- One replay download/playback/capture path.
- One long capture with system audio and, if enabled, microphone audio.
- One hardware encoder path or clear software fallback evidence.

## 5. YouTube field test

Use a real test Google account and non-public video privacy setting. Attach only
redacted evidence:

- OAuth starts and returns to the app.
- Upload succeeds for a small private or unlisted test video.
- Token refresh or expired-token path is exercised.
- Quota/API error state is visible and non-destructive.
- Sign-out clears the account state.

Do not paste OAuth tokens, refresh tokens, cookies, or client secrets into the
field QA record.

## 6. Release decision

Release owner can promote only when:

- automated preflight has no unresolved `FAIL`
- field-only rows in `FIELD_QA_COMMERCIAL_READINESS.md` have Pass evidence or
  are explicitly non-shipping
- installer/updater evidence is attached if those paths are public
- YouTube evidence is attached if upload is public
- payment and Toss live keys remain deferred until a separate payment QA gate
