import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { EntitlementInfo } from "@/api/auth";
import { Crown, Shield, CheckCircle2, XCircle, CreditCard } from "lucide-react";

interface LicensePanelProps {
  isAuthenticated: boolean;
  isLoadingLicense: boolean;
  license: EntitlementInfo | null;
  userEmail: string | undefined;
  onLogin: () => void;
  onUpgradeToPro: () => void;
  onManageSubscription: () => void;
  onRetry: () => void;
}

function formatExpirationDate(dateStr: string | null): string {
  if (!dateStr) return "";
  const date = new Date(dateStr);
  const locale = navigator.language || "ko-KR";
  return date.toLocaleDateString(locale, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

function getDaysRemaining(dateStr: string | null): number {
  if (!dateStr) return -1;
  const expirationDate = new Date(dateStr);
  const now = new Date();
  const diff = expirationDate.getTime() - now.getTime();
  return Math.ceil(diff / (1000 * 60 * 60 * 24));
}

export function LicensePanel({
  isAuthenticated,
  isLoadingLicense,
  license,
  userEmail,
  onLogin,
  onUpgradeToPro,
  onManageSubscription,
  onRetry,
}: LicensePanelProps) {
  const { t } = useTranslation();
  const isActive = license?.status === "active";

  return (
    <div className="gaming-panel p-6">
      <div className="mb-4">
        <h3 className="text-base font-semibold flex items-center gap-2">
          <Crown className="w-5 h-5 text-gaming-cyan" />
          {t("settings.license.title")}
        </h3>
        <p className="text-sm text-muted-foreground mt-1">
          {t("settings.license.description")}
        </p>
      </div>

      <div className="space-y-4">
        {!isAuthenticated ? (
          <div className="text-center py-8">
            <Shield className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
            <p className="text-lg font-semibold mb-2">
              {t("settings.license.loginRequired")}
            </p>
            <p className="text-sm text-muted-foreground mb-4">
              {t("settings.license.loginPrompt")}
            </p>
            <Button onClick={onLogin}>
              {t("settings.license.loginButton")}
            </Button>
          </div>
        ) : isLoadingLicense ? (
          <div className="text-center py-8">
            <p className="text-sm text-muted-foreground">
              {t("settings.license.loadingLicense")}
            </p>
          </div>
        ) : license ? (
          <>
            {/* Current Plan */}
            <div>
              <div className="flex items-center justify-between mb-4">
                <div>
                  <h3 className="text-lg font-semibold flex items-center gap-2">
                    {t("settings.license.currentPlan")}
                    <Badge
                      variant={license.tier === "PRO" ? "default" : "secondary"}
                      className="text-base"
                    >
                      {license.tier}
                    </Badge>
                  </h3>
                  <p className="text-sm text-muted-foreground mt-1">
                    {license.tier === "PRO"
                      ? t("settings.license.proPlanDescription")
                      : t("settings.license.freePlanDescription")}
                  </p>
                </div>
                {license.tier === "FREE" && license.payment_available && (
                  <Button onClick={onUpgradeToPro}>
                    <Crown className="w-4 h-4 mr-2" />
                    {t("settings.account.upgradeToPro")}
                  </Button>
                )}
              </div>

              <Separator className="my-4 bg-white/10" />

              {/* Plan Details */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <p className="text-sm text-muted-foreground">
                    {t("settings.license.status")}
                  </p>
                  <div className="flex items-center gap-2 mt-1">
                    {isActive ? (
                      <>
                        <CheckCircle2 className="w-4 h-4 text-gaming-cyan" />
                        <span className="font-medium">
                          {t("settings.license.active")}
                        </span>
                      </>
                    ) : (
                      <>
                        <XCircle className="w-4 h-4 text-gaming-magenta" />
                        <span className="font-medium">
                          {license.status === "none"
                            ? t("settings.license.inactive")
                            : license.status}
                        </span>
                      </>
                    )}
                  </div>
                </div>

                {license.tier === "PRO" && license.expires_at && (
                  <>
                    <div>
                      <p className="text-sm text-muted-foreground">
                        {t("settings.license.expiresOn")}
                      </p>
                      <p className="font-medium mt-1">
                        {formatExpirationDate(license.expires_at)}
                      </p>
                    </div>

                    {getDaysRemaining(license.expires_at) > 0 && (
                      <div>
                        <p className="text-sm text-muted-foreground">
                          {t("settings.license.daysRemaining")}
                        </p>
                        <p className="font-medium mt-1">
                          {getDaysRemaining(license.expires_at)}{" "}
                          {t("settings.license.days")}
                        </p>
                      </div>
                    )}
                  </>
                )}

                <div>
                  <p className="text-sm text-muted-foreground">
                    {t("settings.license.accountEmail")}
                  </p>
                  <p className="font-medium mt-1">{userEmail || "N/A"}</p>
                </div>
              </div>

              {license.tier === "PRO" && (
                <div className="mt-4">
                  <Button onClick={onManageSubscription} variant="outline">
                    <CreditCard className="w-4 h-4 mr-2" />
                    {t("settings.license.manageSubscription")}
                  </Button>
                </div>
              )}
            </div>

            {/* Plan Comparison (FREE only) */}
            {license.tier === "FREE" && license.payment_available && (
              <>
                <Separator className="bg-white/10" />
                <div>
                  <h3 className="text-lg font-semibold mb-3">
                    {t("settings.license.whyUpgrade")}
                  </h3>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    {[
                      { key: "unlimitedClips", descKey: "unlimitedClipsDesc" },
                      { key: "advancedEditor", descKey: "advancedEditorDesc" },
                      {
                        key: "prioritySupport",
                        descKey: "prioritySupportDesc",
                      },
                      { key: "noWatermarks", descKey: "noWatermarksDesc" },
                    ].map(({ key, descKey }) => (
                      <div key={key} className="flex items-start gap-2">
                        <CheckCircle2 className="w-4 h-4 text-gaming-cyan mt-0.5 flex-shrink-0" />
                        <div>
                          <p className="font-medium text-sm">
                            {t(`settings.license.features.${key}`)}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            {t(`settings.license.features.${descKey}`)}
                          </p>
                        </div>
                      </div>
                    ))}
                  </div>
                  <div className="mt-4 p-4 bg-gaming-cyan/5 border border-gaming-cyan/20 rounded-lg">
                    <p className="text-sm">
                      <strong>{t("settings.license.pricing")}</strong>{" "}
                      {t("settings.license.pricingDetails")}
                    </p>
                  </div>
                </div>
              </>
            )}
            {license.tier === "FREE" && !license.payment_available && (
              <div className="rounded-lg border border-gaming-cyan/20 bg-gaming-cyan/5 p-4">
                <p className="text-sm font-medium">Free public edition</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  Paid PRO sales and checkout are disabled. Editing, export, and
                  YouTube upload are available to signed-in free accounts.
                </p>
              </div>
            )}
          </>
        ) : (
          <div className="text-center py-8">
            <p className="text-sm text-muted-foreground">
              {t("settings.license.loadError")}
            </p>
            <Button onClick={onRetry} variant="outline" className="mt-4">
              {t("editor.retry")}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
