import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";

interface AccountUser {
  email: string;
  id: string;
  tier: "FREE" | "PRO";
}

interface AccountInfoPanelProps {
  user: AccountUser;
}

export function AccountInfoPanel({ user }: AccountInfoPanelProps) {
  const { t } = useTranslation();

  return (
    <div className="gaming-panel p-6">
      <div className="mb-4">
        <h3 className="text-base font-semibold">
          {t("settings.accountInfo.title")}
        </h3>
        <p className="text-sm text-muted-foreground mt-1">
          {t("settings.accountInfo.description")}
        </p>
      </div>
      <div className="space-y-3">
        <div className="flex justify-between py-2 border-b border-white/5">
          <span className="text-sm text-muted-foreground">
            {t("settings.accountInfo.email")}
          </span>
          <span className="text-sm font-medium">{user.email}</span>
        </div>
        <div className="flex justify-between py-2 border-b border-white/5">
          <span className="text-sm text-muted-foreground">
            {t("settings.accountInfo.userId")}
          </span>
          <span className="text-sm font-mono text-gaming-cyan">
            {user.id.substring(0, 8)}...
          </span>
        </div>
        <div className="flex justify-between py-2">
          <span className="text-sm text-muted-foreground">
            {t("settings.accountInfo.licenseTier")}
          </span>
          <Badge variant={user.tier === "PRO" ? "default" : "secondary"}>
            {user.tier}
          </Badge>
        </div>
      </div>
    </div>
  );
}
