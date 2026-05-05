# E4 Desktop Smoke Packet

Use this packet before E5 Field QA. E4 proves the app shell and local desktop paths work on a developer Windows machine, but it does not prove production or commercial readiness.

## Preconditions

- Use a clean test profile or explicitly record the existing profile path.
- Keep Toss/live payment keys disabled.
- Use non-production YouTube credentials if YouTube is configured.
- Preserve any generated logs and diagnostics bundles for review.

## Smoke Steps

| Step                                                                         | Expected result                                                                   | Evidence to collect                             |
| ---------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | ----------------------------------------------- |
| Launch Tauri desktop app with `npm run tauri:dev`                            | App window opens without relying on browser mocks                                 | Screenshot of Dashboard and startup log excerpt |
| Open Dashboard, Games, Replays, Editor, AutoEdit, Results, YouTube, Settings | Each route renders a loading, empty, or actionable degraded state                 | Screenshot per route                            |
| Export diagnostics bundle with redaction enabled                             | Bundle file is created and contains no token/key/payment/signing/Supabase secrets | Bundle path and redaction inspection note       |
| Restart app after changing one local setting                                 | Setting persists through restart                                                  | Before/after screenshot or log                  |
| Run with missing or unavailable FFmpeg                                       | Recording readiness blocks with clear recovery action                             | Screenshot of readiness/diagnostics             |
| Run with missing YouTube credentials                                         | YouTube feature is disabled or prompts setup without crash                        | Screenshot and diagnostic check                 |
| Confirm payment state                                                        | Payment UI is disabled/deferred and does not open checkout                        | Screenshot of payment-deferred modal            |

## Pass Criteria

- No app crash.
- No blank primary screen.
- No unredacted secrets in diagnostics bundle.
- No local-only PRO grant.
- No live payment or checkout path.
- All failures shown to the user have an action or support path.

## Output

Record the result in `docs/FIELD_QA_COMMERCIAL_READINESS.md` as E4 supporting evidence only. E4 evidence cannot close E5 field-only rows.
