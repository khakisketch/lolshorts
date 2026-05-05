import { useEffect, useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useYouTube } from "@/hooks/useYouTube";
import { Progress } from "@/components/ui/progress";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { BarChart3, AlertTriangle, CheckCircle } from "lucide-react";
import { QuotaInfo } from "@/types/youtube";
import { logger } from "@/lib/logger";

export function QuotaDisplay() {
  const { t } = useTranslation();
  const { authStatus, getQuotaInfo } = useYouTube();
  const [quota, setQuota] = useState<QuotaInfo | null>(null);
  const [quotaError, setQuotaError] = useState<string | null>(null);

  const loadQuota = useCallback(async () => {
    try {
      setQuotaError(null);
      const data = await getQuotaInfo();
      setQuota(data);
    } catch (err) {
      logger.error("Failed to load quota:", err);
      setQuota(null);
      setQuotaError(
        err instanceof Error
          ? err.message
          : "Failed to load quota information.",
      );
    }
  }, [getQuotaInfo]);

  useEffect(() => {
    if (authStatus.authenticated) {
      loadQuota();
    }
  }, [authStatus.authenticated, loadQuota]);

  const getQuotaPercentage = () => {
    if (!quota) return 0;
    return Math.round((quota.used / quota.daily_limit) * 100);
  };

  const getQuotaStatus = (): "low" | "medium" | "high" => {
    const percentage = getQuotaPercentage();
    if (percentage >= 80) return "high";
    if (percentage >= 50) return "medium";
    return "low";
  };

  const formatResetDate = (timestamp: number) => {
    const date = new Date(timestamp * 1000);
    return date.toLocaleDateString() + " " + t("quota.atMidnightPT");
  };

  const getUploadsRemaining = () => {
    if (!quota) return 0;
    return Math.floor(quota.remaining / 1600);
  };

  if (!authStatus.authenticated) {
    return null;
  }

  if (quotaError) {
    return (
      <Alert variant="destructive">
        <AlertTriangle className="h-4 w-4" />
        <AlertDescription>{quotaError}</AlertDescription>
      </Alert>
    );
  }

  if (!quota) {
    return (
      <div className="gaming-panel p-6">
        <div className="flex items-center gap-3">
          <BarChart3 className="h-6 w-6" />
          <div>
            <h3 className="text-lg font-semibold">{t("quota.title")}</h3>
            <p className="text-sm text-muted-foreground">
              {t("autoEdit.loadingQuota")}
            </p>
          </div>
        </div>
      </div>
    );
  }

  const quotaStatus = getQuotaStatus();

  return (
    <div className="gaming-panel p-6">
      <div className="mb-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <BarChart3 className="h-6 w-6" />
            <div>
              <h3 className="text-lg font-semibold">{t("quota.title")}</h3>
              <p className="text-sm text-muted-foreground">
                {t("quota.description")}
              </p>
            </div>
          </div>
          {quotaStatus === "high" ? (
            <Badge variant="destructive" className="gap-1">
              <AlertTriangle className="h-3 w-3" />
              {t("quota.highUsage")}
            </Badge>
          ) : quotaStatus === "medium" ? (
            <Badge variant="secondary" className="gap-1">
              <AlertTriangle className="h-3 w-3" />
              {t("quota.mediumUsage")}
            </Badge>
          ) : (
            <Badge variant="default" className="gap-1">
              <CheckCircle className="h-3 w-3" />
              {t("quota.lowUsage")}
            </Badge>
          )}
        </div>
      </div>

      <div className="space-y-4">
        <div className="space-y-2">
          <div className="flex justify-between text-sm">
            <span className="font-medium">{t("quota.usage")}</span>
            <span className="text-muted-foreground">
              {t("quota.units", {
                used: quota.used.toLocaleString(),
                limit: quota.daily_limit.toLocaleString(),
              })}
            </span>
          </div>
          <Progress
            value={getQuotaPercentage()}
            className={
              quotaStatus === "high"
                ? "[&>div]:bg-destructive"
                : quotaStatus === "medium"
                  ? "[&>div]:bg-yellow-500"
                  : ""
            }
          />
          <p className="text-xs text-muted-foreground">
            {t("quota.percentUsed", { percent: getQuotaPercentage() })}
          </p>
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-1">
            <p className="text-sm font-medium">{t("quota.remaining")}</p>
            <p className="text-2xl font-bold">
              {quota.remaining.toLocaleString()}
            </p>
            <p className="text-xs text-muted-foreground">
              {t("quota.unitsLabel")}
            </p>
          </div>

          <div className="space-y-1">
            <p className="text-sm font-medium">{t("quota.uploadsLeft")}</p>
            <p className="text-2xl font-bold">{getUploadsRemaining()}</p>
            <p className="text-xs text-muted-foreground">
              {t("quota.unitsPerUpload")}
            </p>
          </div>
        </div>

        {quotaStatus === "high" && (
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertDescription>{t("quota.highUsageWarning")}</AlertDescription>
          </Alert>
        )}

        <div className="pt-4 border-t">
          <p className="text-xs text-muted-foreground">
            {t("quota.resetDate")} {formatResetDate(quota.reset_at)}
          </p>
        </div>
      </div>
    </div>
  );
}
