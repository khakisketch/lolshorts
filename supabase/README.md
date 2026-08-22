# LoLShorts Supabase Setup

Supabase is the authoritative backend boundary for LoLShorts identity and entitlement.

## Authority Model

- Local SQLite stores app metadata only: games, clips, settings, AutoEdit usage, and generated result metadata.
- Supabase Auth is the only source of truth for user identity.
- `public.user_licenses` is the canonical source of truth for FREE/PRO entitlement.
- `public.auto_edit_usage` is the canonical source of truth for the FREE monthly auto-edit quota. The desktop app's local SQLite counter is only a cache / offline fallback and can be reset by editing the local DB, so it must not be trusted as authoritative.
- `public.subscriptions` and `public.payments` are the future server-side billing record tables.
- Desktop clients must not update `user_licenses` to grant PRO. Future Toss checkout must be confirmed by a trusted server/webhook path before entitlement changes.

## Current Payment Status

Live Toss checkout, payment confirmation, subscription mutation, and paid access are disabled by default. The desktop app returns `payment_available: false` until `LOLSHORTS_PAYMENT_ENABLED=true` is configured for the Supabase `billing` Edge Function after non-payment E5 Field QA and separate paid QA pass.

When enabled, the desktop app can only request checkout, confirmation, cancellation, and subscription reads from the billing function. It must still refresh entitlement from `public.user_licenses`; a payment redirect or client response is never enough to unlock PRO.

## Canonical Tables

```sql
public.user_profiles (
  id UUID PRIMARY KEY REFERENCES auth.users(id),
  email TEXT NOT NULL,
  display_name TEXT,
  avatar_url TEXT,
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ
)

public.user_licenses (
  id UUID PRIMARY KEY,
  user_id UUID REFERENCES public.user_profiles(id),
  tier TEXT REFERENCES public.license_tiers(tier),
  status TEXT, -- active | inactive | cancelled | expired | past_due
  started_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ,
  cancelled_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ
)

public.subscriptions (
  id UUID PRIMARY KEY,
  user_id UUID REFERENCES public.user_profiles(id),
  license_id UUID REFERENCES public.user_licenses(id),
  period TEXT, -- MONTHLY | YEARLY
  amount INTEGER,
  status TEXT,
  billing_key TEXT,
  provider TEXT DEFAULT 'toss',
  provider_subscription_id TEXT,
  provider_customer_id TEXT,
  provider_order_id TEXT,
  next_billing_date DATE,
  current_period_start TIMESTAMPTZ,
  current_period_end TIMESTAMPTZ,
  cancel_at_period_end BOOLEAN,
  cancelled_at TIMESTAMPTZ,
  last_payment_id UUID REFERENCES public.payments(id),
  failure_reason TEXT,
  metadata JSONB
)

public.payments (
  id UUID PRIMARY KEY,
  user_id UUID REFERENCES public.user_profiles(id),
  subscription_id UUID REFERENCES public.subscriptions(id),
  order_id TEXT UNIQUE,
  payment_key TEXT,
  amount INTEGER,
  currency TEXT DEFAULT 'KRW',
  status TEXT,
  method TEXT,
  provider TEXT DEFAULT 'toss',
  idempotency_key TEXT,
  provider_event_id TEXT,
  provider_data JSONB,
  refunded_at TIMESTAMPTZ,
  cancelled_at TIMESTAMPTZ,
  metadata JSONB
)

public.billing_events (
  id UUID PRIMARY KEY,
  provider TEXT DEFAULT 'toss',
  event_id TEXT,
  event_type TEXT,
  user_id UUID REFERENCES public.user_profiles(id),
  payment_id UUID REFERENCES public.payments(id),
  order_id TEXT,
  payment_key TEXT,
  status TEXT,
  payload JSONB,
  processed_at TIMESTAMPTZ
)

public.auto_edit_usage (
  user_id UUID REFERENCES auth.users(id), -- part of PK
  month TEXT,                             -- 'YYYY-MM', part of PK
  used INTEGER DEFAULT 0,
  updated_at TIMESTAMPTZ,
  PRIMARY KEY (user_id, month)
)
-- RLS: owner SELECT only; no client write policy. Mutated exclusively by the
-- service role via the SECURITY DEFINER consume_auto_edit_quota(user_id, month,
-- limit) RPC (EXECUTE granted to service_role only).
```

## Applying Schema

Use the complete ordered migration chain (the migrations directory is the
authoritative production schema):

```bash
supabase init
supabase start
supabase db push
```

Do not apply only `001_initial_schema.sql`: later migrations contain billing
authority, idempotent quota enforcement, and RLS hardening. `supabase/schema.sql`
is a legacy dashboard reference snapshot and must not be used in place of the
ordered migrations.

## Billing Edge Function

The `supabase/functions/billing` function exposes the server-only payment boundary:

- `POST /billing/checkout`: authenticated user checkout creation.
- `POST /billing/confirm`: Toss payment confirmation and canonical DB mutation.
- `POST /billing/webhook/toss`: idempotent webhook reconciliation.
- `POST /billing/cancel`: subscription cancellation request.
- `GET /billing/subscription`: authoritative billing summary.

Required server-side secrets and settings:

- `SUPABASE_URL`
- `SUPABASE_ANON_KEY`
- `SUPABASE_SERVICE_ROLE_KEY`
- `TOSS_SECRET_KEY`
- `TOSS_WEBHOOK_SECRET`
- `LOLSHORTS_PAYMENT_SUCCESS_URL`
- `LOLSHORTS_PAYMENT_FAIL_URL`
- `LOLSHORTS_PAYMENT_ENABLED=true` only after release approval

Never put service role keys, Toss secret keys, webhook secrets, signing keys, or OAuth client secrets in the desktop bundle, frontend environment, local SQLite, support bundles, or logs.

## Auto-Edit Quota Edge Function

The `supabase/functions/quota` function is the server-authoritative counter for
the FREE-tier monthly auto-edit limit (5 / calendar month; PRO is unlimited). It
verifies the caller's Supabase JWT, reads the tier from `user_licenses` with the
service role, and:

- `POST /quota` with body `{ "action": "check" }`: returns `{ allowed, used, limit }`
  (PRO returns `{ allowed: true, limit: null, unlimited: true }`).
- `POST /quota` with body `{ "action": "consume" }`: atomically increments the
  month counter only while under the limit (via `consume_auto_edit_quota` RPC)
  and returns the same shape. Over-limit responds with `allowed: false` and does
  not increment.

Required server-side secrets (same as billing, no Toss keys needed):

- `SUPABASE_URL`
- `SUPABASE_ANON_KEY`
- `SUPABASE_SERVICE_ROLE_KEY`

The desktop app (`video::commands::start_auto_edit`) calls `check` before
composing and `consume` after a successful compose, using a short 5s timeout. On
any server failure (offline/timeout/non-2xx) it falls back to the local SQLite
counter so offline users are not blocked; the local counter is always advanced
as a cache. This means a determined offline user can still exceed the limit
locally, but an online user cannot bypass it by editing the local DB.

### Deploying the quota function

> Do NOT deploy from an automated agent. These are the manual operator steps.

```bash
# 1. Apply the migration that creates public.auto_edit_usage + the RPC.
supabase db push

# 2. Deploy the edge function.
supabase functions deploy quota

# 3. Confirm the required secrets are set (service role must be present).
supabase secrets list
# If missing:
# supabase secrets set SUPABASE_SERVICE_ROLE_KEY=... (etc.)
```

### Verifying the quota function

```bash
# With a valid user access token in $TOKEN and project ref in $REF:
# check (FREE user, fresh month) -> {"allowed":true,"used":0,"limit":5,"tier":"FREE"}
curl -s -X POST "https://$REF.supabase.co/functions/v1/quota" \
  -H "Authorization: Bearer $TOKEN" \
  -H "apikey: $SUPABASE_ANON_KEY" \
  -H "Content-Type: application/json" \
  -d '{"action":"check"}'

# consume 5x -> used climbs 1..5; the 6th consume returns "allowed":false and
# does not increment "used" beyond 5.
curl -s -X POST "https://$REF.supabase.co/functions/v1/quota" \
  -H "Authorization: Bearer $TOKEN" -H "apikey: $SUPABASE_ANON_KEY" \
  -H "Content-Type: application/json" -d '{"action":"consume"}'
```

Verify tamper-resistance: authenticate as a FREE user and confirm a direct
PostgREST write to `auto_edit_usage` (INSERT/UPDATE) is rejected by RLS, and that
calling `rpc/consume_auto_edit_quota` directly with a user JWT is denied
(EXECUTE granted to `service_role` only). Pure request-parsing/tier logic is unit
tested in `supabase/functions/quota/logic.test.ts` (`deno test`).

## RLS Contract

- Users can read their own `user_profiles`, `user_licenses`, `subscriptions`, and `payments`.
- `license_tiers` is publicly readable.
- Payment and entitlement mutations should be performed only by a trusted server-side process when billing is enabled.
- Webhook events are stored in `billing_events` and must be idempotent by provider event id.
- Users can read their own `auto_edit_usage`, but cannot write it. The counter is mutated only by the service role through the `consume_auto_edit_quota` RPC (SECURITY DEFINER, EXECUTE granted to `service_role` only).

## Desktop Contract

The desktop app should use:

- Supabase JS for login/OAuth session acquisition.
- Tauri `set_session` to validate the access token against Supabase Auth.
- Tauri `get_current_entitlement` to read `user_licenses`.
- UI gating derived from `tier === "PRO"` and `status === "active"` only.

If entitlement cannot be verified, the desktop app must fail closed to FREE/no paid access.
