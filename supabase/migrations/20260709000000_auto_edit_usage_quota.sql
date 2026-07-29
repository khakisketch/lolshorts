-- Server-authoritative Auto-Edit usage quota.
--
-- Purpose: the FREE-tier monthly auto-edit limit (5 / calendar month) must not
-- be enforceable only by the desktop client's local SQLite counter. That local
-- counter can be reset or bypassed by editing/deleting the on-device DB file,
-- which lets a FREE user run auto-edit an unlimited number of times. This table
-- is the authoritative, per-user, per-month counter. The desktop app treats its
-- local SQLite counter as a cache / offline fallback only (see
-- storage::Storage auto-edit quota docs and video::commands::start_auto_edit).
--
-- Enforcement boundary (deliberate availability trade-off): the client falls
-- back to the local counter when this endpoint is UNREACHABLE (offline, server
-- outage), so legitimate offline users are never blocked. Consequently the
-- server counter is authoritative for ONLINE clients only; a determined user
-- who firewalls the quota endpoint can still fall back to (and tamper with)
-- the local counter. Hardening beyond this (fail-closed when FREE, or signed
-- offline-usage reconciliation on reconnect) is a documented future option,
-- not implemented.
--
-- Security model (mirrors public.user_licenses / public.billing_events):
--   * RLS enabled. A user may SELECT only their own row.
--   * There is intentionally NO INSERT/UPDATE/DELETE policy, so a client can
--     never write or tamper with the counter through PostgREST with its own
--     JWT. Only the service role (used by the `quota` Edge Function), which
--     bypasses RLS, may mutate the counter -- and it does so exclusively via the
--     SECURITY DEFINER `consume_auto_edit_quota` RPC defined below.

CREATE TABLE IF NOT EXISTS public.auto_edit_usage (
    -- References auth.users directly: this is auth/entitlement-adjacent state
    -- and must key off the canonical Supabase auth identity, not app metadata.
    user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    month TEXT NOT NULL, -- 'YYYY-MM' UTC calendar month
    used INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),

    PRIMARY KEY (user_id, month),
    CONSTRAINT auto_edit_usage_month_format CHECK (month ~ '^\d{4}-\d{2}$'),
    CONSTRAINT auto_edit_usage_used_nonneg CHECK (used >= 0)
);

ALTER TABLE public.auto_edit_usage ENABLE ROW LEVEL SECURITY;

-- Read-only for the owning user. The absence of any write policy means clients
-- cannot INSERT/UPDATE/DELETE; only the service role (RLS-exempt) may write.
DROP POLICY IF EXISTS "Users can view own auto-edit usage" ON public.auto_edit_usage;
CREATE POLICY "Users can view own auto-edit usage"
    ON public.auto_edit_usage FOR SELECT
    USING (auth.uid() = user_id);

CREATE INDEX IF NOT EXISTS idx_auto_edit_usage_user_id
    ON public.auto_edit_usage(user_id);

-- Atomic consume-one-unit primitive.
--
-- Inserts the month row if missing, then increments `used` only while it is
-- strictly below the limit, in a single statement so concurrent callers cannot
-- race past the ceiling. Returns whether the increment was applied plus the
-- resulting (or current, when already exhausted) count and the limit.
--
-- SECURITY DEFINER so the write runs as the table owner and is not blocked by
-- the read-only RLS policy. EXECUTE is then revoked from every client role and
-- granted only to `service_role`, so a JWT holder cannot call this RPC directly
-- (e.g. with an inflated p_limit) to forge quota -- only the trusted Edge
-- Function, which supplies the real limit, can invoke it.
CREATE OR REPLACE FUNCTION public.consume_auto_edit_quota(
    p_user_id UUID,
    p_month TEXT,
    p_limit INTEGER
)
RETURNS TABLE (allowed BOOLEAN, used INTEGER, "limit" INTEGER)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
    v_used INTEGER;
BEGIN
    INSERT INTO public.auto_edit_usage (user_id, month, used, updated_at)
    VALUES (p_user_id, p_month, 0, NOW())
    ON CONFLICT (user_id, month) DO NOTHING;

    UPDATE public.auto_edit_usage
    SET used = used + 1, updated_at = NOW()
    WHERE user_id = p_user_id
      AND month = p_month
      AND used < p_limit
    RETURNING used INTO v_used;

    IF v_used IS NULL THEN
        -- Already at/over the limit: report the current count, no increment.
        SELECT auto_edit_usage.used INTO v_used
        FROM public.auto_edit_usage
        WHERE user_id = p_user_id AND month = p_month;
        RETURN QUERY SELECT FALSE, COALESCE(v_used, 0), p_limit;
    ELSE
        RETURN QUERY SELECT TRUE, v_used, p_limit;
    END IF;
END;
$$;

REVOKE ALL ON FUNCTION public.consume_auto_edit_quota(UUID, TEXT, INTEGER) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.consume_auto_edit_quota(UUID, TEXT, INTEGER) FROM anon;
REVOKE ALL ON FUNCTION public.consume_auto_edit_quota(UUID, TEXT, INTEGER) FROM authenticated;
GRANT EXECUTE ON FUNCTION public.consume_auto_edit_quota(UUID, TEXT, INTEGER) TO service_role;
