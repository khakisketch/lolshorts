import { ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAuthStore } from "@/lib/auth";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { AuthModal } from "./AuthModal";

interface ProtectedFeatureProps {
  children: ReactNode;
  requiresPro?: boolean;
  featureName?: string;
  fallback?: ReactNode;
  onUpgrade?: () => void;
}

export function ProtectedFeature({
  children,
  requiresPro = false,
  featureName = "this feature",
  fallback,
  onUpgrade,
}: ProtectedFeatureProps) {
  const { t } = useTranslation();
  const { entitlement, isAuthenticated, user } = useAuthStore();
  const [authModalOpen, setAuthModalOpen] = useState(false);
  const hasProEntitlement =
    entitlement?.tier === "PRO" && entitlement.status === "active";

  // Not authenticated
  if (!isAuthenticated || !user) {
    if (fallback) {
      return <>{fallback}</>;
    }

    return (
      <>
        <div className="gaming-panel p-6">
          <div className="mb-4">
            <h3 className="text-lg font-semibold flex items-center gap-2">
              {t("auth.authenticationRequired")}
            </h3>
            <p className="text-sm text-muted-foreground">
              {t("auth.pleaseLoginToAccess", { feature: featureName })}
            </p>
          </div>
          <div>
            <p className="text-sm text-muted-foreground mb-4">
              {t("auth.createAccountOrLogin")}
            </p>
            <Button
              onClick={() => setAuthModalOpen(true)}
              className="w-full"
              data-testid="open-auth-modal-button"
            >
              {t("auth.loginSignup")}
            </Button>
          </div>
        </div>

        <AuthModal
          open={authModalOpen}
          onClose={() => setAuthModalOpen(false)}
          defaultMode="login"
        />
      </>
    );
  }

  // Authenticated but needs PRO
  if (requiresPro && !hasProEntitlement) {
    if (fallback) {
      return <>{fallback}</>;
    }

    return (
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center justify-between">
            <span className="flex items-center gap-2">
              {t("protectedFeature.proFeature")}
            </span>
            <Badge variant="default">PRO</Badge>
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("protectedFeature.upgradeDescription", { feature: featureName })}
          </p>
        </div>
        <div>
          <div className="space-y-4">
            <p className="text-sm text-muted-foreground">
              {t("protectedFeature.exclusiveDescription")}
            </p>
            <ul className="list-disc list-inside space-y-2 text-sm">
              <li>{t("protectedFeature.features.manualClipExtraction")}</li>
              <li>{t("protectedFeature.features.youtubeShortsComposition")}</li>
              <li>{t("protectedFeature.features.customThumbnail")}</li>
              <li>{t("protectedFeature.features.advancedVideoEditing")}</li>
              <li>{t("protectedFeature.features.noWatermarks")}</li>
            </ul>
            <Button onClick={onUpgrade} className="w-full" variant="default">
              {t("protectedFeature.upgradeToPro")}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  // All checks passed
  return <>{children}</>;
}

// Hook for imperative checks
export function useFeatureAccess() {
  const { entitlement, isAuthenticated } = useAuthStore();
  const isPro = entitlement?.tier === "PRO" && entitlement.status === "active";

  return {
    isAuthenticated,
    isPro,
    canAccess: (requiresPro: boolean = false) => {
      if (!isAuthenticated) return false;
      if (requiresPro && !isPro) return false;
      return true;
    },
  };
}
