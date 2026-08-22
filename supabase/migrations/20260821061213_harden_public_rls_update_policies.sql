-- Harden client-visible policies without rewriting already-applied migrations.
--
-- 1. Evaluate auth.uid() once per statement rather than once per row.
-- 2. Bind policies to authenticated clients explicitly.
-- 3. Prevent UPDATE from changing ownership columns.
-- 4. Retire the legacy client-writable quota table. Current quota enforcement
--    uses auto_edit_usage through a service-role-only RPC.

ALTER POLICY "Users can view own profile" ON public.user_profiles
    TO authenticated
    USING ((SELECT auth.uid()) = id);

ALTER POLICY "Users can update own profile" ON public.user_profiles
    TO authenticated
    USING ((SELECT auth.uid()) = id)
    WITH CHECK ((SELECT auth.uid()) = id);

DROP POLICY IF EXISTS "Users can insert own profile" ON public.user_profiles;
CREATE POLICY "Users can insert own profile" ON public.user_profiles
    FOR INSERT
    TO authenticated
    WITH CHECK (
        (SELECT auth.uid()) = id
        AND email = COALESCE((SELECT auth.jwt() ->> 'email'), '')
    );

-- Profile identity and timestamps are controlled by Supabase Auth/defaults.
-- Clients may only edit presentation fields.
REVOKE INSERT, UPDATE ON public.user_profiles FROM authenticated;
GRANT INSERT (id, email, display_name, avatar_url)
    ON public.user_profiles TO authenticated;
GRANT UPDATE (display_name, avatar_url)
    ON public.user_profiles TO authenticated;

ALTER POLICY "Users can view own licenses" ON public.user_licenses
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can view own subscriptions" ON public.subscriptions
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can view own payments" ON public.payments
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can view own games" ON public.games
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can insert own games" ON public.games
    TO authenticated
    WITH CHECK ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can update own games" ON public.games
    TO authenticated
    USING ((SELECT auth.uid()) = user_id)
    WITH CHECK ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can delete own games" ON public.games
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can view own clips" ON public.clips
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can insert own clips" ON public.clips
    TO authenticated
    WITH CHECK ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can delete own clips" ON public.clips
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can view own results" ON public.auto_edit_results
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can insert own results" ON public.auto_edit_results
    TO authenticated
    WITH CHECK ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can delete own results" ON public.auto_edit_results
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can view own uploads" ON public.youtube_uploads
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can insert own uploads" ON public.youtube_uploads
    TO authenticated
    WITH CHECK ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can update own uploads" ON public.youtube_uploads
    TO authenticated
    USING ((SELECT auth.uid()) = user_id)
    WITH CHECK ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can delete own uploads" ON public.youtube_uploads
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can view own quota" ON public.quota_usage
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

DROP POLICY IF EXISTS "Users can insert own quota" ON public.quota_usage;
DROP POLICY IF EXISTS "Users can update own quota" ON public.quota_usage;
REVOKE INSERT, UPDATE ON public.quota_usage FROM authenticated;

ALTER POLICY "Users can view own billing events" ON public.billing_events
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can view own auto-edit usage" ON public.auto_edit_usage
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);

ALTER POLICY "Users can view own auto-edit quota consumptions"
    ON public.auto_edit_quota_consumptions
    TO authenticated
    USING ((SELECT auth.uid()) = user_id);
