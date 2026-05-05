-- LoLShorts Database Schema
-- Supabase PostgreSQL Schema for LoLShorts Application

-- =============================================================================
-- Enable Required Extensions
-- =============================================================================
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- =============================================================================
-- Users Table (extends Supabase auth.users)
-- =============================================================================
CREATE TABLE IF NOT EXISTS public.user_profiles (
    id UUID REFERENCES auth.users(id) ON DELETE CASCADE PRIMARY KEY,
    email TEXT NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Enable Row Level Security
ALTER TABLE public.user_profiles ENABLE ROW LEVEL SECURITY;

-- Users can only read their own profile
CREATE POLICY "Users can view own profile"
    ON public.user_profiles FOR SELECT
    USING (auth.uid() = id);

-- Users can update their own profile
CREATE POLICY "Users can update own profile"
    ON public.user_profiles FOR UPDATE
    USING (auth.uid() = id);

-- =============================================================================
-- License Tiers Table
-- =============================================================================
CREATE TABLE IF NOT EXISTS public.license_tiers (
    tier TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    price_monthly INTEGER NOT NULL, -- in KRW (₩)
    price_yearly INTEGER NOT NULL,  -- in KRW (₩)
    max_clips_per_game INTEGER,     -- NULL = unlimited
    max_storage_gb INTEGER,          -- NULL = unlimited
    features JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Insert default tiers
INSERT INTO public.license_tiers (tier, name, price_monthly, price_yearly, max_clips_per_game, max_storage_gb, features)
VALUES
    ('FREE', 'Free', 0, 0, 10, 5, '["Basic recording", "10 clips per game", "5GB storage", "720p export"]'),
    ('PRO', 'PRO', 9900, 99000, NULL, NULL, '["Unlimited clips", "Unlimited storage", "1080p export", "Advanced editor", "Priority support", "No watermark", "Cloud backup"]')
ON CONFLICT (tier) DO NOTHING;

-- Public read access for license tiers
ALTER TABLE public.license_tiers ENABLE ROW LEVEL SECURITY;
CREATE POLICY "License tiers are publicly readable"
    ON public.license_tiers FOR SELECT
    TO public
    USING (true);

-- =============================================================================
-- User Licenses Table
-- =============================================================================
CREATE TABLE IF NOT EXISTS public.user_licenses (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES public.user_profiles(id) ON DELETE CASCADE NOT NULL,
    tier TEXT REFERENCES public.license_tiers(tier) NOT NULL DEFAULT 'FREE',
    status TEXT NOT NULL DEFAULT 'active', -- active | cancelled | expired
    started_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE, -- NULL = lifetime/free tier
    cancelled_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT valid_status CHECK (status IN ('active', 'cancelled', 'expired'))
);

-- Ensure one active license per user
CREATE UNIQUE INDEX idx_user_active_license ON public.user_licenses(user_id) WHERE status = 'active';

-- Enable Row Level Security
ALTER TABLE public.user_licenses ENABLE ROW LEVEL SECURITY;

-- Users can view their own licenses
CREATE POLICY "Users can view own licenses"
    ON public.user_licenses FOR SELECT
    USING (auth.uid() = user_id);

-- =============================================================================
-- Subscriptions Table
-- =============================================================================
CREATE TABLE IF NOT EXISTS public.subscriptions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES public.user_profiles(id) ON DELETE CASCADE NOT NULL,
    license_id UUID REFERENCES public.user_licenses(id) ON DELETE CASCADE NOT NULL,
    period TEXT NOT NULL, -- MONTHLY | YEARLY
    amount INTEGER NOT NULL, -- in KRW (₩)
    status TEXT NOT NULL DEFAULT 'pending', -- pending | active | cancelled | failed
    billing_key TEXT, -- Toss Payments billing key for auto-renewal
    next_billing_date DATE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT valid_period CHECK (period IN ('MONTHLY', 'YEARLY')),
    CONSTRAINT valid_status CHECK (status IN ('pending', 'active', 'cancelled', 'failed'))
);

-- Enable Row Level Security
ALTER TABLE public.subscriptions ENABLE ROW LEVEL SECURITY;

-- Users can view their own subscriptions
CREATE POLICY "Users can view own subscriptions"
    ON public.subscriptions FOR SELECT
    USING (auth.uid() = user_id);

-- =============================================================================
-- Payments Table
-- =============================================================================
CREATE TABLE IF NOT EXISTS public.payments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES public.user_profiles(id) ON DELETE CASCADE NOT NULL,
    subscription_id UUID REFERENCES public.subscriptions(id) ON DELETE SET NULL,
    order_id TEXT UNIQUE NOT NULL,
    payment_key TEXT,
    amount INTEGER NOT NULL, -- in KRW (₩)
    status TEXT NOT NULL DEFAULT 'pending', -- pending | completed | failed | cancelled | refunded
    method TEXT, -- card | bank_transfer | mobile | etc
    provider TEXT DEFAULT 'toss', -- toss | other
    provider_data JSONB, -- Raw payment provider response
    requested_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE,
    failed_at TIMESTAMP WITH TIME ZONE,
    failure_reason TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT valid_status CHECK (status IN ('pending', 'completed', 'failed', 'cancelled', 'refunded'))
);

-- Create index for faster order_id lookups
CREATE INDEX idx_payments_order_id ON public.payments(order_id);
CREATE INDEX idx_payments_user_id ON public.payments(user_id);
CREATE INDEX idx_payments_status ON public.payments(status);

-- Enable Row Level Security
ALTER TABLE public.payments ENABLE ROW LEVEL SECURITY;

-- Users can view their own payments
CREATE POLICY "Users can view own payments"
    ON public.payments FOR SELECT
    USING (auth.uid() = user_id);

-- =============================================================================
-- Game Statistics Table (Optional - for analytics)
-- =============================================================================
CREATE TABLE IF NOT EXISTS public.game_statistics (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES public.user_profiles(id) ON DELETE CASCADE NOT NULL,
    game_id TEXT NOT NULL,
    champion TEXT,
    game_mode TEXT,
    duration INTEGER, -- seconds
    result TEXT, -- win | loss | remake
    kills INTEGER DEFAULT 0,
    deaths INTEGER DEFAULT 0,
    assists INTEGER DEFAULT 0,
    clips_created INTEGER DEFAULT 0,
    played_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes for analytics queries
CREATE INDEX idx_game_stats_user_id ON public.game_statistics(user_id);
CREATE INDEX idx_game_stats_played_at ON public.game_statistics(played_at DESC);

-- Enable Row Level Security
ALTER TABLE public.game_statistics ENABLE ROW LEVEL SECURITY;

-- Users can view their own stats
CREATE POLICY "Users can view own game statistics"
    ON public.game_statistics FOR SELECT
    USING (auth.uid() = user_id);

-- Users can insert their own stats
CREATE POLICY "Users can insert own game statistics"
    ON public.game_statistics FOR INSERT
    WITH CHECK (auth.uid() = user_id);

-- =============================================================================
-- Functions
-- =============================================================================

-- Function to get user's current license tier
CREATE OR REPLACE FUNCTION public.get_user_license_tier(user_uuid UUID)
RETURNS TEXT
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    current_tier TEXT;
BEGIN
    SELECT tier INTO current_tier
    FROM public.user_licenses
    WHERE user_id = user_uuid
    AND status = 'active'
    AND (expires_at IS NULL OR expires_at > NOW())
    LIMIT 1;

    RETURN COALESCE(current_tier, 'FREE');
END;
$$;

-- Function to update license expiration status
CREATE OR REPLACE FUNCTION public.update_expired_licenses()
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
BEGIN
    UPDATE public.user_licenses
    SET status = 'expired',
        updated_at = NOW()
    WHERE status = 'active'
    AND expires_at IS NOT NULL
    AND expires_at < NOW();
END;
$$;

-- Function to handle new user registration
CREATE OR REPLACE FUNCTION public.handle_new_user()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
BEGIN
    INSERT INTO public.user_profiles (id, email, created_at)
    VALUES (NEW.id, NEW.email, NOW());

    INSERT INTO public.user_licenses (user_id, tier, status)
    VALUES (NEW.id, 'FREE', 'active');

    RETURN NEW;
END;
$$;

-- Trigger for new user registration
CREATE TRIGGER on_auth_user_created
    AFTER INSERT ON auth.users
    FOR EACH ROW
    EXECUTE FUNCTION public.handle_new_user();

-- =============================================================================
-- Scheduled Jobs (using pg_cron extension if available)
-- =============================================================================
-- To enable: Run this in Supabase SQL editor
-- SELECT cron.schedule(
--     'expire-licenses',
--     '0 0 * * *', -- Daily at midnight
--     $$SELECT public.update_expired_licenses()$$
-- );

-- =============================================================================
-- Indexes for Performance
-- =============================================================================
CREATE INDEX IF NOT EXISTS idx_user_licenses_user_id ON public.user_licenses(user_id);
CREATE INDEX IF NOT EXISTS idx_user_licenses_status ON public.user_licenses(status);
CREATE INDEX IF NOT EXISTS idx_user_licenses_expires_at ON public.user_licenses(expires_at) WHERE expires_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_subscriptions_user_id ON public.subscriptions(user_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_status ON public.subscriptions(status);
CREATE INDEX IF NOT EXISTS idx_subscriptions_next_billing ON public.subscriptions(next_billing_date) WHERE status = 'active';

-- =============================================================================
-- Initial Setup Complete
-- =============================================================================
-- To apply this schema:
-- 1. Go to Supabase Dashboard > SQL Editor
-- 2. Create new query
-- 3. Paste this entire file
-- 4. Click "Run"
-- 5. Verify all tables created successfully
