import { invoke } from "@tauri-apps/api/core";
import { useState, useCallback, useEffect } from "react";
import { AutoEditQuotaInfo } from "@/types/autoEdit";
import { getErrorMessage } from "@/lib/utils";

/**
 * Hook for managing auto-edit quota
 *
 * Tracks monthly usage and enforces limits:
 * - FREE tier: 5 auto-edits per month
 * - PRO tier: Unlimited auto-edits
 */
export function useAutoEditQuota() {
  const [quota, setQuota] = useState<AutoEditQuotaInfo | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /**
   * Fetch current quota information
   */
  const fetchQuota = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const quotaInfo = await invoke<AutoEditQuotaInfo>("get_auto_edit_quota");
      setQuota(quotaInfo);
      return quotaInfo;
    } catch (err) {
      const errorMsg = getErrorMessage(err);
      setError(errorMsg);
      // Error is captured in state for UI display
      return null;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Check if user has quota available
   */
  const hasQuota = useCallback((): boolean => {
    if (!quota) return false;
    return quota.is_pro || quota.remaining > 0;
  }, [quota]);

  /**
   * Get quota warning level
   * - 'none': No warning (PRO or plenty of quota)
   * - 'low': Low quota (1-2 remaining)
   * - 'exhausted': No quota remaining
   */
  const getQuotaWarningLevel = useCallback((): "none" | "low" | "exhausted" => {
    if (!quota) return "none";
    if (quota.is_pro) return "none";
    if (quota.remaining === 0) return "exhausted";
    if (quota.remaining <= 2) return "low";
    return "none";
  }, [quota]);

  // Fetch quota on mount
  useEffect(() => {
    fetchQuota();
  }, [fetchQuota]);

  return {
    quota,
    isLoading,
    error,
    fetchQuota,
    hasQuota,
    getQuotaWarningLevel,
  };
}
