# Paid Production Readiness

This checklist must pass before LoLShorts sells paid PRO access. It is separate from the non-payment Windows RC checklist.

## Authority Rules

- SQLite is local app-data storage only.
- Supabase Auth is the login authority.
- Supabase `public.user_licenses` is the PRO entitlement authority.
- Toss payment redirects, browser routes, Zustand, localStorage, Tauri command responses, and local SQLite data cannot grant PRO.
- Server-side payment confirmation or webhook reconciliation is the only path that may update `payments`, `subscriptions`, and `user_licenses`.

## Server Configuration

Keep these values server-side in Supabase Edge Function secrets or an equivalent central server:

- `SUPABASE_SERVICE_ROLE_KEY`
- `TOSS_SECRET_KEY`
- `TOSS_WEBHOOK_SECRET`
- signing/updater private keys
- OAuth client secrets

The desktop/frontend bundle may use only public Supabase URL/anon key and user access tokens.

## Required Paid QA

| Gate | Required evidence |
| ---- | ----------------- |
| Toss sandbox checkout | Checkout URL creation, redirect success/fail, and server confirmation logs |
| Client-only bypass | Payment success route without server confirmation does not unlock PRO |
| Webhook idempotency | Duplicate webhook replay does not double-grant, double-charge, or corrupt rows |
| Out-of-order events | Failed/cancelled/refunded/past_due events fail closed and update canonical tables consistently |
| Supabase outage | Entitlement refresh failure leaves UI FREE/no paid access |
| Refund/cancel | `payments`, `subscriptions`, and `user_licenses` reflect refund or cancellation policy |
| Diagnostics redaction | Support bundle contains no Toss, Supabase service-role, OAuth, signing, or payment secrets |
| Live small-amount test | Controlled live payment plus refund/cancel evidence and rollback plan |

## Release Blockers

Paid production release is blocked until all are true:

- Non-payment E5 Field QA is complete.
- Paid QA table above is complete.
- Terms, privacy, refund, cancellation, support SLA, and Google/YouTube OAuth disclosure are approved.
- Signed installer/updater/rollback evidence exists for the exact paid release candidate.
- Release owner approves `LOLSHORTS_PAYMENT_ENABLED=true` for production.
