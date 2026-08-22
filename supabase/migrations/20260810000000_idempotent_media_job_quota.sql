-- Idempotent quota consumption keyed by the durable desktop media job.
CREATE TABLE IF NOT EXISTS public.auto_edit_quota_consumptions (
    user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    job_id TEXT NOT NULL,
    month TEXT NOT NULL,
    consumed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, job_id),
    CONSTRAINT auto_edit_quota_job_id_length CHECK (char_length(job_id) BETWEEN 1 AND 160),
    CONSTRAINT auto_edit_quota_month_format CHECK (month ~ '^\d{4}-\d{2}$')
);

ALTER TABLE public.auto_edit_quota_consumptions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Users can view own auto-edit quota consumptions"
    ON public.auto_edit_quota_consumptions;
CREATE POLICY "Users can view own auto-edit quota consumptions"
    ON public.auto_edit_quota_consumptions FOR SELECT
    USING (auth.uid() = user_id);

CREATE OR REPLACE FUNCTION public.consume_auto_edit_quota(
    p_user_id UUID,
    p_month TEXT,
    p_limit INTEGER,
    p_job_id TEXT
)
RETURNS TABLE (allowed BOOLEAN, used INTEGER, "limit" INTEGER)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
    v_used INTEGER;
    v_inserted INTEGER;
BEGIN
    IF p_job_id IS NULL OR char_length(p_job_id) NOT BETWEEN 1 AND 160 THEN
        RAISE EXCEPTION 'invalid job id';
    END IF;

    INSERT INTO public.auto_edit_usage (user_id, month, used, updated_at)
    VALUES (p_user_id, p_month, 0, NOW())
    ON CONFLICT (user_id, month) DO NOTHING;

    INSERT INTO public.auto_edit_quota_consumptions(user_id, job_id, month)
    VALUES(p_user_id, p_job_id, p_month)
    ON CONFLICT (user_id, job_id) DO NOTHING;
    GET DIAGNOSTICS v_inserted = ROW_COUNT;

    IF v_inserted = 0 THEN
        SELECT auto_edit_usage.used INTO v_used
        FROM public.auto_edit_usage
        WHERE user_id = p_user_id AND month = p_month;
        RETURN QUERY SELECT TRUE, COALESCE(v_used, 0), p_limit;
        RETURN;
    END IF;

    UPDATE public.auto_edit_usage
    SET used = used + 1, updated_at = NOW()
    WHERE user_id = p_user_id
      AND month = p_month
      AND used < p_limit
    RETURNING auto_edit_usage.used INTO v_used;

    IF v_used IS NULL THEN
        DELETE FROM public.auto_edit_quota_consumptions
        WHERE user_id = p_user_id AND job_id = p_job_id;
        SELECT auto_edit_usage.used INTO v_used
        FROM public.auto_edit_usage
        WHERE user_id = p_user_id AND month = p_month;
        RETURN QUERY SELECT FALSE, COALESCE(v_used, 0), p_limit;
    ELSE
        RETURN QUERY SELECT TRUE, v_used, p_limit;
    END IF;
END;
$$;

REVOKE ALL ON FUNCTION public.consume_auto_edit_quota(UUID, TEXT, INTEGER, TEXT) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.consume_auto_edit_quota(UUID, TEXT, INTEGER, TEXT) FROM anon;
REVOKE ALL ON FUNCTION public.consume_auto_edit_quota(UUID, TEXT, INTEGER, TEXT) FROM authenticated;
GRANT EXECUTE ON FUNCTION public.consume_auto_edit_quota(UUID, TEXT, INTEGER, TEXT) TO service_role;
