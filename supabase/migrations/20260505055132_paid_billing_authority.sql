-- Paid billing authority hardening.
-- Local SQLite must never be used as the source of truth for auth, payment, or
-- PRO entitlement. This migration keeps Supabase Postgres as the authoritative
-- ledger for Toss payment attempts, provider subscription state, and user
-- licenses.

-- Entitlement states exposed to clients. Only active, unexpired PRO licenses
-- unlock paid features; every other status must fail closed to FREE.
ALTER TABLE public.user_licenses
    DROP CONSTRAINT IF EXISTS valid_status;
ALTER TABLE public.user_licenses
    DROP CONSTRAINT IF EXISTS user_licenses_status_check;
ALTER TABLE public.user_licenses
    ADD CONSTRAINT user_licenses_status_check
    CHECK (status IN ('active', 'inactive', 'cancelled', 'expired', 'past_due'));

-- Provider subscription state. `past_due` is explicit so renewal failures do
-- not look like an active paid account.
ALTER TABLE public.subscriptions
    DROP CONSTRAINT IF EXISTS valid_status;
ALTER TABLE public.subscriptions
    DROP CONSTRAINT IF EXISTS subscriptions_status_check;
ALTER TABLE public.subscriptions
    ADD CONSTRAINT subscriptions_status_check
    CHECK (status IN ('pending', 'active', 'cancelled', 'failed', 'expired', 'past_due'));

ALTER TABLE public.subscriptions
    ADD COLUMN IF NOT EXISTS provider TEXT NOT NULL DEFAULT 'toss',
    ADD COLUMN IF NOT EXISTS provider_subscription_id TEXT,
    ADD COLUMN IF NOT EXISTS provider_customer_id TEXT,
    ADD COLUMN IF NOT EXISTS provider_order_id TEXT,
    ADD COLUMN IF NOT EXISTS current_period_start TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS current_period_end TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS cancel_at_period_end BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS cancelled_at TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS last_payment_id UUID,
    ADD COLUMN IF NOT EXISTS failure_reason TEXT,
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Payment ledger. Provider confirmation/webhook code writes here; clients only
-- read their own rows through RLS policies.
ALTER TABLE public.payments
    DROP CONSTRAINT IF EXISTS valid_status;
ALTER TABLE public.payments
    DROP CONSTRAINT IF EXISTS payments_status_check;
ALTER TABLE public.payments
    ADD CONSTRAINT payments_status_check
    CHECK (status IN ('pending', 'completed', 'failed', 'cancelled', 'refunded', 'partially_refunded'));

ALTER TABLE public.payments
    ADD COLUMN IF NOT EXISTS currency TEXT NOT NULL DEFAULT 'KRW',
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT,
    ADD COLUMN IF NOT EXISTS provider_event_id TEXT,
    ADD COLUMN IF NOT EXISTS refunded_at TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS cancelled_at TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE public.subscriptions
    DROP CONSTRAINT IF EXISTS subscriptions_last_payment_id_fkey;
ALTER TABLE public.subscriptions
    ADD CONSTRAINT subscriptions_last_payment_id_fkey
    FOREIGN KEY (last_payment_id) REFERENCES public.payments(id) ON DELETE SET NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_payments_provider_order
    ON public.payments(provider, order_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_payments_idempotency_key
    ON public.payments(idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_payments_provider_event_id
    ON public.payments(provider_event_id)
    WHERE provider_event_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_subscriptions_provider_subscription_id
    ON public.subscriptions(provider_subscription_id)
    WHERE provider_subscription_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_subscriptions_current_period_end
    ON public.subscriptions(current_period_end)
    WHERE current_period_end IS NOT NULL;

-- Raw webhook/audit event log. No user-facing write policy is added; service
-- role code is the only trusted writer. Events are idempotent per provider.
CREATE TABLE IF NOT EXISTS public.billing_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    provider TEXT NOT NULL DEFAULT 'toss',
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    user_id UUID REFERENCES public.user_profiles(id) ON DELETE SET NULL,
    payment_id UUID REFERENCES public.payments(id) ON DELETE SET NULL,
    order_id TEXT,
    payment_key TEXT,
    status TEXT NOT NULL DEFAULT 'received',
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    error_message TEXT,
    received_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),

    CONSTRAINT billing_events_status_check
        CHECK (status IN ('received', 'processed', 'ignored', 'failed')),
    CONSTRAINT billing_events_provider_event_unique
        UNIQUE (provider, event_id)
);

ALTER TABLE public.billing_events ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Users can view own billing events" ON public.billing_events;
CREATE POLICY "Users can view own billing events"
    ON public.billing_events FOR SELECT
    USING (auth.uid() = user_id);

CREATE INDEX IF NOT EXISTS idx_billing_events_user_id
    ON public.billing_events(user_id);
CREATE INDEX IF NOT EXISTS idx_billing_events_order_id
    ON public.billing_events(order_id)
    WHERE order_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_billing_events_payment_key
    ON public.billing_events(payment_key)
    WHERE payment_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_billing_events_received_at
    ON public.billing_events(received_at DESC);
