import { useAutoEditQuota } from "@/hooks/useAutoEditQuota";
import { Badge } from "@/components/ui/badge";
import { Sparkles, Crown, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

export function AutoEditQuotaBadge() {
  const { t } = useTranslation();
  const { quota, isLoading, getQuotaWarningLevel } = useAutoEditQuota();

  if (isLoading || !quota) {
    return (
      <Badge variant="secondary" className="flex items-center gap-1">
        <Loader2 className="w-3 h-3 animate-spin" />
        {t("autoEdit.loadingQuota")}
      </Badge>
    );
  }

  const warningLevel = getQuotaWarningLevel();

  // PRO user badge
  if (quota.is_pro) {
    return (
      <Badge
        variant="default"
        className="flex items-center gap-1 bg-gradient-to-r from-yellow-400 to-yellow-600"
      >
        <Crown className="w-3 h-3" />
        PRO • {t("autoEdit.unlimitedEdits")}
      </Badge>
    );
  }

  const badgeVariant =
    warningLevel === "exhausted"
      ? "destructive"
      : warningLevel === "low"
        ? "default"
        : "secondary";
  const badgeText = `${quota.remaining}/${quota.limit} ${t("autoEdit.remaining")}`;

  return (
    <div className="flex items-center gap-2">
      <Badge variant={badgeVariant} className="flex items-center gap-1">
        <Sparkles className="w-3 h-3" />
        {badgeText}
      </Badge>
      {warningLevel === "exhausted" && (
        <span className="text-xs text-muted-foreground">
          Free public edition quota reached
        </span>
      )}
    </div>
  );
}
