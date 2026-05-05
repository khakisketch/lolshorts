# LoLShorts Supabase Setup

Supabase is the authoritative backend boundary for LoLShorts identity and entitlement.

## Authority Model

- Local SQLite stores app metadata only: games, clips, settings, AutoEdit usage, and generated result metadata.
- Supabase Auth is the only source of truth for user identity.
- `public.user_licenses` is the canonical source of truth for FREE/PRO entitlement.
- `public.subscriptions` and `public.payments` are the future server-side billing record tables.
- Desktop clients must not update `user_licenses` to grant PRO. Future Toss checkout must be confirmed by a trusted server/webhook path before entitlement changes.

## Current Payment Status

Live Toss checkout, payment confirmation, subscription mutation, and paid access are deferred. The desktop app should return `payment_available: false` and a deferred reason for payment-related commands until a separate payment QA plan is approved.

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
  status TEXT, -- active | cancelled | expired
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
  next_billing_date DATE
)

public.payments (
  id UUID PRIMARY KEY,
  user_id UUID REFERENCES public.user_profiles(id),
  subscription_id UUID REFERENCES public.subscriptions(id),
  order_id TEXT UNIQUE,
  payment_key TEXT,
  amount INTEGER,
  status TEXT,
  provider TEXT DEFAULT 'toss',
  provider_data JSONB
)
```

## Applying Schema

Use the consolidated migration:

```bash
supabase init
supabase start
supabase db push
```

Or apply the SQL directly from the Supabase dashboard:

```text
supabase/migrations/001_initial_schema.sql
```

`supabase/schema.sql` mirrors the intended production schema for dashboard/manual setup.

## RLS Contract

- Users can read their own `user_profiles`, `user_licenses`, `subscriptions`, and `payments`.
- `license_tiers` is publicly readable.
- Payment and entitlement mutations should be performed only by a trusted server-side process when billing is enabled.

## Desktop Contract

The desktop app should use:

- Supabase JS for login/OAuth session acquisition.
- Tauri `set_session` to validate the access token against Supabase Auth.
- Tauri `get_current_entitlement` to read `user_licenses`.
- UI gating derived from `tier === "PRO"` and `status === "active"` only.

If entitlement cannot be verified, the desktop app must fail closed to FREE/no paid access.
