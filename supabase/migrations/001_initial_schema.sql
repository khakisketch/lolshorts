-- Enable required extensions.
-- Production Supabase projects already provide the auth schema and API roles.
-- Any local bootstrap in this file must stay passwordless and least-privilege.
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Create auth schema if needed
CREATE SCHEMA IF NOT EXISTS auth;

-- Create auth.users table (simplified version for local dev)
CREATE TABLE IF NOT EXISTS auth.users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    encrypted_password TEXT,
    email_confirmed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create custom user profiles table
CREATE TABLE IF NOT EXISTS public.user_profiles (
    id UUID PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT,
    avatar_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- License tiers are catalog data, not per-user authority.
CREATE TABLE IF NOT EXISTS public.license_tiers (
    tier TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    price_monthly INTEGER NOT NULL,
    price_yearly INTEGER NOT NULL,
    max_clips_per_game INTEGER,
    max_storage_gb INTEGER,
    features JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO public.license_tiers (
    tier,
    name,
    price_monthly,
    price_yearly,
    max_clips_per_game,
    max_storage_gb,
    features
) VALUES
    ('FREE', 'Free', 0, 0, 10, 5, '["Basic recording", "10 clips per game", "5GB storage", "720p export"]'),
    ('PRO', 'PRO', 9900, 99000, NULL, NULL, '["Unlimited clips", "Unlimited storage", "1080p export", "Advanced editor", "Priority support", "No watermark", "Cloud backup"]')
ON CONFLICT (tier) DO NOTHING;

-- Canonical entitlement table. Desktop SQLite must not be treated as
-- authoritative for this data.
CREATE TABLE IF NOT EXISTS public.user_licenses (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES public.user_profiles(id) ON DELETE CASCADE,
    tier TEXT NOT NULL REFERENCES public.license_tiers(tier) DEFAULT 'FREE',
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'cancelled', 'expired')),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_user_active_license
    ON public.user_licenses(user_id)
    WHERE status = 'active';

CREATE TABLE IF NOT EXISTS public.subscriptions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES public.user_profiles(id) ON DELETE CASCADE,
    license_id UUID NOT NULL REFERENCES public.user_licenses(id) ON DELETE CASCADE,
    period TEXT NOT NULL CHECK (period IN ('MONTHLY', 'YEARLY')),
    amount INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'active', 'cancelled', 'failed')),
    billing_key TEXT,
    next_billing_date DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.payments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES public.user_profiles(id) ON DELETE CASCADE,
    subscription_id UUID REFERENCES public.subscriptions(id) ON DELETE SET NULL,
    order_id TEXT UNIQUE NOT NULL,
    payment_key TEXT,
    amount INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'completed', 'failed', 'cancelled', 'refunded')),
    method TEXT,
    provider TEXT NOT NULL DEFAULT 'toss',
    provider_data JSONB,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    failed_at TIMESTAMPTZ,
    failure_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create games table
CREATE TABLE IF NOT EXISTS public.games (
    game_id BIGINT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES public.user_profiles(id) ON DELETE CASCADE,
    game_start_time TIMESTAMPTZ NOT NULL,
    game_end_time TIMESTAMPTZ,
    champion_name TEXT,
    game_mode TEXT,
    game_result TEXT CHECK (game_result IN ('Victory', 'Defeat', 'Remake')),
    kills INTEGER DEFAULT 0,
    deaths INTEGER DEFAULT 0,
    assists INTEGER DEFAULT 0,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create clips table
CREATE TABLE IF NOT EXISTS public.clips (
    id BIGSERIAL PRIMARY KEY,
    game_id BIGINT NOT NULL REFERENCES public.games(game_id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES public.user_profiles(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_time DOUBLE PRECISION NOT NULL,
    priority INTEGER NOT NULL CHECK (priority >= 1 AND priority <= 5),
    duration_secs DOUBLE PRECISION NOT NULL DEFAULT 30.0,
    thumbnail_path TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create auto_edit_results table
CREATE TABLE IF NOT EXISTS public.auto_edit_results (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES public.user_profiles(id) ON DELETE CASCADE,
    job_id UUID NOT NULL DEFAULT uuid_generate_v4(),
    output_file_path TEXT NOT NULL,
    duration_secs DOUBLE PRECISION NOT NULL,
    clips_used INTEGER NOT NULL DEFAULT 0,
    file_size_bytes BIGINT,
    template_name TEXT,
    with_music BOOLEAN DEFAULT FALSE,
    status TEXT NOT NULL DEFAULT 'completed' CHECK (status IN ('completed', 'failed')),
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create youtube_uploads table
CREATE TABLE IF NOT EXISTS public.youtube_uploads (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES public.user_profiles(id) ON DELETE CASCADE,
    video_id TEXT UNIQUE,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'uploading', 'processing', 'completed', 'failed')),
    upload_progress INTEGER DEFAULT 0,
    file_path TEXT NOT NULL,
    thumbnail_path TEXT,
    privacy TEXT NOT NULL DEFAULT 'private' CHECK (privacy IN ('public', 'unlisted', 'private')),
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    uploaded_at TIMESTAMPTZ
);

-- Create quota_usage table (for FREE tier limits)
CREATE TABLE IF NOT EXISTS public.quota_usage (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES public.user_profiles(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL CHECK (resource_type IN ('auto_edit', 'upload', 'storage_gb')),
    usage_count INTEGER NOT NULL DEFAULT 0,
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, resource_type, period_start)
);

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_games_user_id ON public.games(user_id);
CREATE INDEX IF NOT EXISTS idx_games_game_start_time ON public.games(game_start_time DESC);
CREATE INDEX IF NOT EXISTS idx_clips_game_id ON public.clips(game_id);
CREATE INDEX IF NOT EXISTS idx_clips_user_id ON public.clips(user_id);
CREATE INDEX IF NOT EXISTS idx_clips_priority ON public.clips(priority DESC);
CREATE INDEX IF NOT EXISTS idx_user_licenses_user_id ON public.user_licenses(user_id);
CREATE INDEX IF NOT EXISTS idx_user_licenses_status ON public.user_licenses(status);
CREATE INDEX IF NOT EXISTS idx_user_licenses_expires_at ON public.user_licenses(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_subscriptions_user_id ON public.subscriptions(user_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_status ON public.subscriptions(status);
CREATE INDEX IF NOT EXISTS idx_payments_order_id ON public.payments(order_id);
CREATE INDEX IF NOT EXISTS idx_payments_user_id ON public.payments(user_id);
CREATE INDEX IF NOT EXISTS idx_payments_status ON public.payments(status);
CREATE INDEX IF NOT EXISTS idx_auto_edit_results_user_id ON public.auto_edit_results(user_id);
CREATE INDEX IF NOT EXISTS idx_youtube_uploads_user_id ON public.youtube_uploads(user_id);
CREATE INDEX IF NOT EXISTS idx_quota_usage_user_id ON public.quota_usage(user_id, resource_type);

-- Create updated_at trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION public.handle_new_user()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO public.user_profiles (id, email)
    VALUES (NEW.id, NEW.email)
    ON CONFLICT (id) DO NOTHING;

    INSERT INTO public.user_licenses (user_id, tier, status)
    VALUES (NEW.id, 'FREE', 'active')
    ON CONFLICT DO NOTHING;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER
SET search_path = public, auth, pg_temp;

-- Add triggers for updated_at
CREATE TRIGGER update_user_profiles_updated_at BEFORE UPDATE ON public.user_profiles
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_user_licenses_updated_at BEFORE UPDATE ON public.user_licenses
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_subscriptions_updated_at BEFORE UPDATE ON public.subscriptions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_games_updated_at BEFORE UPDATE ON public.games
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

DROP TRIGGER IF EXISTS on_auth_user_created ON auth.users;
CREATE TRIGGER on_auth_user_created
    AFTER INSERT ON auth.users
    FOR EACH ROW EXECUTE FUNCTION public.handle_new_user();

-- Enable Row Level Security (RLS)
ALTER TABLE public.user_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.license_tiers ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.user_licenses ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.payments ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.games ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.clips ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.auto_edit_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.youtube_uploads ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.quota_usage ENABLE ROW LEVEL SECURITY;

-- RLS Policies for user_profiles
CREATE POLICY "Users can view own profile" ON public.user_profiles
    FOR SELECT USING (auth.uid() = id);

CREATE POLICY "Users can update own profile" ON public.user_profiles
    FOR UPDATE USING (auth.uid() = id);

-- RLS Policies for license and billing tables
CREATE POLICY "License tiers are publicly readable" ON public.license_tiers
    FOR SELECT TO public USING (true);

CREATE POLICY "Users can view own licenses" ON public.user_licenses
    FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can view own subscriptions" ON public.subscriptions
    FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can view own payments" ON public.payments
    FOR SELECT USING (auth.uid() = user_id);

-- RLS Policies for games
CREATE POLICY "Users can view own games" ON public.games
    FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can insert own games" ON public.games
    FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can update own games" ON public.games
    FOR UPDATE USING (auth.uid() = user_id);

CREATE POLICY "Users can delete own games" ON public.games
    FOR DELETE USING (auth.uid() = user_id);

-- RLS Policies for clips
CREATE POLICY "Users can view own clips" ON public.clips
    FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can insert own clips" ON public.clips
    FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can delete own clips" ON public.clips
    FOR DELETE USING (auth.uid() = user_id);

-- RLS Policies for auto_edit_results
CREATE POLICY "Users can view own results" ON public.auto_edit_results
    FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can insert own results" ON public.auto_edit_results
    FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can delete own results" ON public.auto_edit_results
    FOR DELETE USING (auth.uid() = user_id);

-- RLS Policies for youtube_uploads
CREATE POLICY "Users can view own uploads" ON public.youtube_uploads
    FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can insert own uploads" ON public.youtube_uploads
    FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can update own uploads" ON public.youtube_uploads
    FOR UPDATE USING (auth.uid() = user_id);

CREATE POLICY "Users can delete own uploads" ON public.youtube_uploads
    FOR DELETE USING (auth.uid() = user_id);

-- RLS Policies for quota_usage
CREATE POLICY "Users can view own quota" ON public.quota_usage
    FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can insert own quota" ON public.quota_usage
    FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can update own quota" ON public.quota_usage
    FOR UPDATE USING (auth.uid() = user_id);

-- Create only the passwordless API roles needed by local Postgres bootstrap.
-- Supabase-hosted production projects already own these roles; do not create
-- LOGIN roles or default passwords in production-facing migrations.
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'anon') THEN
        CREATE ROLE anon NOLOGIN;
    END IF;

    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'authenticated') THEN
        CREATE ROLE authenticated NOLOGIN;
    END IF;

    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'service_role') THEN
        CREATE ROLE service_role NOLOGIN;
    END IF;
END
$$;

-- Grant permissions. RLS remains the data access boundary for authenticated
-- users; anon only receives explicit public catalog reads.
GRANT USAGE ON SCHEMA public TO anon, authenticated, service_role;
GRANT SELECT ON public.license_tiers TO anon, authenticated;

GRANT SELECT, INSERT, UPDATE, DELETE ON public.user_profiles TO authenticated;
GRANT SELECT ON public.user_licenses TO authenticated;
GRANT SELECT ON public.subscriptions TO authenticated;
GRANT SELECT ON public.payments TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.games TO authenticated;
GRANT SELECT, INSERT, DELETE ON public.clips TO authenticated;
GRANT SELECT, INSERT, DELETE ON public.auto_edit_results TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.youtube_uploads TO authenticated;
GRANT SELECT, INSERT, UPDATE ON public.quota_usage TO authenticated;

GRANT ALL ON ALL TABLES IN SCHEMA public TO service_role;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO authenticated, service_role;
GRANT EXECUTE ON FUNCTION public.update_updated_at_column() TO authenticated, service_role;
GRANT EXECUTE ON FUNCTION public.handle_new_user() TO service_role;


-- Comment on tables
COMMENT ON TABLE public.user_profiles IS 'User profile information only; entitlement is in user_licenses';
COMMENT ON TABLE public.user_licenses IS 'Canonical user entitlement source for FREE/PRO access';
COMMENT ON TABLE public.subscriptions IS 'Server-side subscription records; inactive while payment is deferred';
COMMENT ON TABLE public.payments IS 'Server-side payment records; inactive while payment is deferred';
COMMENT ON TABLE public.games IS 'League of Legends game sessions';
COMMENT ON TABLE public.clips IS 'Individual gameplay clips extracted from recordings';
COMMENT ON TABLE public.auto_edit_results IS 'Auto-generated highlight videos';
COMMENT ON TABLE public.youtube_uploads IS 'YouTube upload queue and status';
COMMENT ON TABLE public.quota_usage IS 'Tracks resource usage for FREE tier limits';
