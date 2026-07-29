# Service Readiness Policy

Use this policy before describing LoLShorts as ready for public paid use, signed distribution, automatic updates, support operations, or privacy-sensitive local processing. It is a product and documentation policy only. It does not enable payment, subscription enforcement, Toss integration, or any new upload path.

This document follows the evidence levels in [Field QA Commercial Readiness](./FIELD_QA_COMMERCIAL_READINESS.md). Use [Release Field QA Runbook](./RELEASE_FIELD_QA_RUNBOOK.md) to generate local preflight evidence before field testing. Build commands, local tests, mocked integrations, and browser checks can support a candidate build, but real readiness claims need field evidence from the target Windows environment.

## Current readiness stance

| Area                         | Current status                                                                                                                                                 | Public claim limit                                                                                                               |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| Installer and signing        | Build and signing instructions exist. Clean Windows install, signature trust, upgrade, rollback, and uninstall field checks are not yet recorded here.         | Say installer validation is planned or required. Do not claim signed installer readiness until field evidence is attached.       |
| Updater                      | Tauri updater documentation exists. Real updater channel, manifest, signature, restart, rollback, and user data preservation checks are not yet recorded here. | Say updater validation is required. Do not claim automatic update readiness until the signed update path passes field QA.        |
| YouTube                      | App metadata and upload flows exist, but production account behavior still needs real credential validation.                                                   | Say YouTube behavior requires real test account validation before public readiness claims.                                       |
| TikTok and Instagram         | Export presets and manual guidance only. Direct upload is not implemented.                                                                                     | Say TikTok and Instagram are preset or export guide workflows only. Do not claim direct upload.                                  |
| Support workflow             | Draft issue intake and diagnostics handling are defined below. A full support dry run still needs field evidence.                                              | Say support workflow is prepared for validation. Do not claim commercial support readiness until dry runs pass.                  |
| Privacy and local processing | Video processing is intended to happen locally, with exceptions listed below for external services and user-submitted support evidence.                        | Say local processing applies to video analysis and editing paths that stay on device. Do not say no data ever leaves the device. |
| FREE and PRO policy          | Product-level policy is defined below. Billing, subscription enforcement, Toss, and paid access remain deferred.                                               | Say paid plan design is deferred. Do not claim live billing or subscription enforcement readiness.                               |

## Installer and updater validation

### Verified scope today

The repository contains build, bundle, signing, and updater instructions in `BUILD_GUIDE.md`, `DEPLOYMENT.md`, and related CI docs. Those files document intended artifact generation and release steps. They do not prove a signed installer or updater is safe for public users.

### Unverified scope

These checks remain unverified unless a release owner attaches field evidence:

1. Signed MSI install on a clean Windows 10 or Windows 11 machine.
2. Signed EXE or NSIS install on a clean Windows machine, if that artifact is shipped.
3. Publisher identity, certificate chain, timestamp, and artifact hash inspection.
4. First launch from Start Menu, desktop shortcut, and install directory.
5. Upgrade from the previous candidate or production build without losing clips, settings, auth state, or local database records.
6. Updater manifest retrieval from the intended channel.
7. Signed update download, apply, restart, and version confirmation.
8. Failed update, rollback, reinstall, and uninstall behavior.
9. User data left behind after uninstall, if any, matches documented behavior.

### Required evidence before public claims

Record the following with the field QA checklist before claiming installer or updater readiness:

| Evidence item           | Required detail                                                                                      |
| ----------------------- | ---------------------------------------------------------------------------------------------------- |
| Artifact identity       | Version, commit, artifact path, SHA256 hash, installer type, and signing certificate summary         |
| Machine record          | Windows version, account type, antivirus or SmartScreen observations, and clean profile status       |
| Install proof           | Screenshots or logs for install, first launch, and bundled FFmpeg availability                       |
| Update proof            | Updater manifest URL or channel, previous version, target version, restart result, and update logs   |
| Data preservation proof | Before and after checks for clips, generated results, settings, auth state, and local database files |
| Failure proof           | Rollback, failed update, uninstall, and reinstall notes with expected user recovery steps            |

### Public claim limits

Until those checks pass, use conservative wording:

Allowed:

- "Installer build instructions are available."
- "Updater validation is part of the field QA gate."
- "Signed distribution requires clean Windows installer and updater evidence."

Blocked:

- "Production ready installer."
- "Automatic updates are ready for public users."
- "Signed distribution is complete."
- "Clean install, upgrade, rollback, and uninstall are verified" unless the evidence is attached.

## Support workflow

### Intake route

Use GitHub Issues as the default engineering intake until a staffed support route is separately approved. Email or community routes can be listed only when an owner, response expectation, and privacy handling process are documented for the release.

Every support issue should include:

1. LoLShorts version and commit or build number.
2. Windows version, CPU, RAM, GPU, driver version, and audio devices.
3. League client state, replay state, or YouTube auth state relevant to the issue.
4. Exact steps to reproduce, expected result, and actual result.
5. Screenshots or short recordings when they do not expose private information.
6. Relevant app logs or diagnostics bundle after review and redaction.
7. Whether the issue blocks install, launch, recording, replay, AutoEdit, export, upload, update, or uninstall.

### Diagnostics bundle handling

Diagnostics and support bundles must be treated as private user evidence. The user or support owner should review contents before sharing.

The desktop backend exposes `get_diagnostics_status` for release/support readiness checks and `export_diagnostics_bundle(redact: true)` for a default secret-safe support artifact. The redacted bundle is intended to include version, diagnostic checks, storage health, high-level system summary, non-secret setting keys, and recent log excerpts. It must not be treated as proof of field readiness by itself.

Allowed contents:

- App version, build number, operating system version, and high-level hardware summary.
- App settings summary that excludes credentials and tokens.
- Recent app logs, error codes, command names, and failure timestamps.
- Disk space summary and selected app storage paths.
- Clip or result IDs, file names, and generated output paths when needed for reproduction.

Do not include:

- OAuth tokens, refresh tokens, cookies, session secrets, payment keys, Toss keys, Supabase keys, or signing keys.
- Full gameplay videos, voice recordings, screenshots, or personal files unless the user explicitly chooses to attach them.
- League account passwords, Google passwords, billing records, card data, addresses, or unrelated local files.
- Private chat content or unrelated browser history.

If a bundle may contain sensitive data, the support owner must ask for a redacted copy or provide steps to extract the specific safe lines. Never request whole profile directories as a default support step.

### Escalation expectations

Use this routing until a staffed support process replaces it:

| Severity        | Example                                                                                                                       | Expected action                                                                                                  |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Release blocker | Installer cannot launch, updater loop, data loss, token exposure, corrupted local database                                    | Stop public readiness claim, file blocker, assign owner, preserve logs, publish mitigation if users are affected |
| High            | Recording fails on supported hardware, YouTube upload loses retry state, export output is missing, uninstall breaks reinstall | File issue with reproduction and logs, assign owner before paid launch                                           |
| Medium          | Confusing recovery path, missing message, unsupported platform wording, non-blocking UI bug                                   | Track for readiness cleanup or release notes                                                                     |
| Low             | Copy issue, docs mismatch, optional guide improvement                                                                         | Fix in docs or backlog                                                                                           |

Commercial support readiness requires a dry run where a tester reproduces a failure, gathers a safe bundle, files an issue, follows recovery guidance, and confirms owner coverage.

## Privacy and local processing explanation

LoLShorts should describe privacy in terms of specific workflows, not broad absolutes.

### Stays local by design

Based on current architecture and docs, these items are intended to stay on the user's PC unless the user chooses to share or upload them:

- Raw gameplay recordings and generated video files.
- FFmpeg processing for recording, clipping, composition, and export.
- Local app database, clip metadata, result metadata, settings, and logs.
- Diagnostics bundle content before the user submits it to a support route.

### May leave the device

These actions may send data to external services or people:

- YouTube OAuth and upload flows send account authorization data, video metadata, thumbnails, and selected video files to Google or YouTube when the user signs in and uploads.
- Riot or League client integrations may involve local client APIs or Riot controlled data sources, depending on the flow being tested.
- Support issues, diagnostics bundles, screenshots, and sample files leave the device when the user submits them.
- Future account, payment, Toss, subscription, analytics, or hosted service work may send additional data, but that work is deferred until after non-payment readiness gates pass.

### Evidence needed for privacy claims

Before public privacy claims, attach evidence for:

1. Which commands read local files and which commands send network requests.
2. Whether YouTube tokens are stored securely and cleared on sign out.
3. Whether diagnostics summaries omit secrets, tokens, payment keys, personal files, and private media by default.
4. Whether support instructions avoid asking for full profile folders or unrelated files.
5. Whether any account, analytics, or payment paths are active in the tested build.

Use this safe wording until evidence is complete: "Video processing is designed to run locally. Uploads, account flows, external APIs, and support submissions may send selected data outside the device."

## FREE and PRO product policy

This section defines product policy only. It does not implement billing, subscriptions, enforcement, Toss, webhooks, or paid access.

### Authority boundary

LoLShorts uses local SQLite for app metadata such as games, clips, settings, and AutoEdit results. Local SQLite is user-modifiable and is never the source of truth for authentication, payment, or PRO access. Supabase Auth validates identity, and Supabase `user_licenses` is the canonical entitlement table. `subscriptions` and `payments` are reserved for future server-side billing records while payment remains deferred.

If Supabase entitlement cannot be verified, the app must fail closed to FREE/no paid access instead of trusting cached local state.

### Policy intent

| Tier | Intended role                                                                                                                               | Readiness limit                                                                                                        |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| FREE | Let users validate core local recording, replay review, AutoEdit previews or exports within defined product limits, and local export paths. | May be described as a planned access tier only until field gates and entitlement behavior are verified.                |
| PRO  | Future paid tier for higher limits, premium workflow options, or commercial features after non-payment readiness passes.                    | Must not be sold or enforced until payment QA, legal review, refund flow, support readiness, and rollback plans exist. |

### Deferred payment rule

Payment and Toss implementation stays deferred until all non-payment field gates pass, including installer, updater, YouTube, support, privacy, recovery, GPU, audio, LoL, and replay validation. Passing automated tests or producing signed artifacts does not authorize live payment keys, paid plans, production webhooks, subscription enforcement, or paid access.

### Public claim limits

Allowed:

- "FREE and PRO are product policy concepts under review."
- "Payment work is deferred until non-payment readiness and support gates pass."
- "Toss or billing work needs a separate payment QA plan."

Blocked:

- "PRO billing is ready."
- "Subscription enforcement is active for public launch."
- "Toss payments are configured for production."
- "Paid access can start after build or installer success alone."

## Pre-release service readiness checklist

Before a release owner approves public non-payment readiness, confirm:

- The field QA checklist has E5 evidence for real Windows installer and updater paths, or release notes clearly state those paths are not public-ready.
- YouTube has real test account evidence before any public upload readiness claim.
- TikTok and Instagram remain export guide and preset only, with no direct upload claim.
- Diagnostics and support bundle instructions exclude secrets, tokens, keys, personal sensitive data, and unrelated files.
- Privacy copy names what stays local and what may leave the device.
- FREE and PRO are described as policy only, with payment and Toss deferred.
- No docs or release notes claim real LoL, replay, GPU, audio, Windows field completion, installer completion, updater completion, support readiness, or payment readiness without matching field evidence.
