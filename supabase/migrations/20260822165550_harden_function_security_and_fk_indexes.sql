-- Harden the hosted schema after the initial public release migration.
--
-- Keep SECURITY DEFINER functions explicit and inaccessible through the Data
-- API, make trigger helpers deterministic, and add covering indexes for the
-- billing foreign keys that are queried during reconciliation.

ALTER FUNCTION public.update_updated_at_column()
    SET search_path = public, pg_temp;

REVOKE ALL ON FUNCTION public.handle_new_user() FROM PUBLIC;
REVOKE ALL ON FUNCTION public.handle_new_user() FROM anon;
REVOKE ALL ON FUNCTION public.handle_new_user() FROM authenticated;

ALTER POLICY "Users can insert own profile" ON public.user_profiles
    TO authenticated
    WITH CHECK (
        (SELECT auth.uid()) = id
        AND email = COALESCE(((SELECT auth.jwt()) ->> 'email'), '')
    );

CREATE INDEX IF NOT EXISTS idx_billing_events_payment_id
    ON public.billing_events(payment_id)
    WHERE payment_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_payments_subscription_id
    ON public.payments(subscription_id)
    WHERE subscription_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_subscriptions_last_payment_id
    ON public.subscriptions(last_payment_id)
    WHERE last_payment_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_subscriptions_license_id
    ON public.subscriptions(license_id);

CREATE INDEX IF NOT EXISTS idx_user_licenses_tier
    ON public.user_licenses(tier);
