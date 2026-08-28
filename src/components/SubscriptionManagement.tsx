import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { paymentApi, SubscriptionDetails } from "@/api/payment";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import {
  CreditCard,
  AlertCircle,
  CheckCircle2,
  XCircle,
  Calendar,
  Loader2,
} from "lucide-react";
import { getErrorMessage } from "@/lib/utils";
import { logger } from "@/lib/logger";

interface SubscriptionManagementProps {
  isOpen: boolean;
  onClose: () => void;
  currentTier: "FREE" | "PRO";
  expiresAt: string | null;
}

// SubscriptionDetails interface removed (imported from api)

export function SubscriptionManagement({
  isOpen,
  onClose,
  currentTier,
  expiresAt,
}: SubscriptionManagementProps) {
  const { t } = useTranslation();
  const [subscription, setSubscription] = useState<SubscriptionDetails | null>(
    null,
  );
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCancelConfirm, setShowCancelConfirm] = useState(false);
  const [isCancelling, setIsCancelling] = useState(false);

  // Load subscription details when dialog opens
  useEffect(() => {
    if (isOpen && currentTier === "PRO") {
      loadSubscriptionDetails();
    }
  }, [isOpen, currentTier]);

  const loadSubscriptionDetails = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const details = await paymentApi.getSubscriptionDetails();
      setSubscription(details);
    } catch (err) {
      logger.error("Failed to load subscription:", err);
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  };

  const handleCancelSubscription = async () => {
    setIsCancelling(true);
    setError(null);

    try {
      await paymentApi.cancelSubscription();

      // Refresh subscription details
      await loadSubscriptionDetails();

      setShowCancelConfirm(false);

      // Show success message
      alert(t("settings.account.cancelSuccess"));
    } catch (err) {
      logger.error("Failed to cancel subscription:", err);
      setError(getErrorMessage(err));
    } finally {
      setIsCancelling(false);
    }
  };

  const formatDate = (dateStr: string): string => {
    const date = new Date(dateStr);
    const locale = navigator.language || "ko-KR";
    return date.toLocaleDateString(locale, {
      year: "numeric",
      month: "long",
      day: "numeric",
    });
  };

  if (currentTier === "FREE") {
    return (
      <Dialog open={isOpen} onOpenChange={onClose}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("subscription.noActive")}</DialogTitle>
            <DialogDescription>{t("subscription.freePlan")}</DialogDescription>
          </DialogHeader>

          <div className="text-center py-6">
            <XCircle className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
            <p className="text-sm text-muted-foreground">
              {t("subscription.upgradePrompt")}
            </p>
          </div>

          <DialogFooter>
            <Button onClick={onClose}>{t("common.close")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }

  return (
    <>
      <Dialog open={isOpen && !showCancelConfirm} onOpenChange={onClose}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <CreditCard className="w-5 h-5" />
              {t("subscription.management")}
            </DialogTitle>
            <DialogDescription>
              {t("subscription.managementDesc")}
            </DialogDescription>
          </DialogHeader>

          {error && (
            <Alert variant="destructive">
              <AlertCircle className="w-4 h-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          {isLoading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="w-8 h-8 animate-spin text-primary" />
              <span className="ml-2 text-sm text-muted-foreground">
                {t("subscription.loadingDetails")}
              </span>
            </div>
          ) : subscription ? (
            <div className="space-y-4">
              {!subscription.payment_available && (
                <Alert>
                  <AlertCircle className="w-4 h-4" />
                  <AlertDescription>
                    {subscription.reason || subscription.payment_message}
                  </AlertDescription>
                </Alert>
              )}

              {/* Subscription Status */}
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">
                  {t("subscription.status")}
                </span>
                <Badge
                  variant={subscription.is_active ? "default" : "destructive"}
                >
                  {subscription.is_active ? (
                    <CheckCircle2 className="w-3 h-3 mr-1" />
                  ) : (
                    <XCircle className="w-3 h-3 mr-1" />
                  )}
                  {subscription.is_active
                    ? t("subscription.active")
                    : t("subscription.inactive")}
                </Badge>
              </div>

              <Separator />

              {/* Plan Details */}
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <span className="text-sm text-muted-foreground">
                    {t("subscription.plan")}
                  </span>
                  <span className="font-medium">{subscription.tier}</span>
                </div>

                {subscription.expires_at && !subscription.auto_renew && (
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-muted-foreground flex items-center gap-1">
                      <Calendar className="w-4 h-4" />
                      {t("subscription.accessUntil")}
                    </span>
                    <span className="font-medium">
                      {formatDate(subscription.expires_at)}
                    </span>
                  </div>
                )}
              </div>

              <Separator />

              {/* Cancellation Info */}
              {!subscription.auto_renew ? (
                <Alert>
                  <AlertCircle className="w-4 h-4" />
                  <AlertDescription>
                    {expiresAt
                      ? t("subscription.cancelledNotice", {
                          date: formatDate(expiresAt),
                        })
                      : t("subscription.cancelledNoticeNoDate")}
                  </AlertDescription>
                </Alert>
              ) : (
                <div className="text-sm text-muted-foreground space-y-2">
                  <p>• {t("subscription.autoRenewInfo1")}</p>
                  <p>• {t("subscription.autoRenewInfo2")}</p>
                </div>
              )}
            </div>
          ) : (
            <div className="text-center py-6">
              <AlertCircle className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
              <p className="text-sm text-muted-foreground">
                {t("subscription.loadFailed")}
              </p>
            </div>
          )}

          <DialogFooter className="flex gap-2">
            <Button variant="outline" onClick={onClose}>
              {t("common.close")}
            </Button>

            {subscription && subscription.auto_renew && (
              <Button
                variant="destructive"
                onClick={() => setShowCancelConfirm(true)}
              >
                {t("subscription.cancelSubscription")}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Cancel Confirmation Dialog */}
      <Dialog open={showCancelConfirm} onOpenChange={setShowCancelConfirm}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="text-destructive">
              {t("subscription.cancelConfirmTitle")}
            </DialogTitle>
            <DialogDescription>
              {t("subscription.cancelConfirmDesc")}
            </DialogDescription>
          </DialogHeader>

          <Alert>
            <AlertCircle className="w-4 h-4" />
            <AlertDescription>
              {t("subscription.cancelNotice")}
            </AlertDescription>
          </Alert>

          <div className="space-y-2 text-sm">
            <p className="font-semibold">{t("subscription.loseAccess")}</p>
            <ul className="list-disc list-inside text-muted-foreground space-y-1">
              <li>{t("subscription.loseUnlimitedClips")}</li>
              <li>{t("subscription.loseAdvancedEditor")}</li>
              <li>{t("subscription.lose1080p60")}</li>
              <li>{t("subscription.losePrioritySupport")}</li>
              <li>{t("subscription.loseNoWatermarks")}</li>
            </ul>
          </div>

          <DialogFooter className="flex gap-2">
            <Button
              variant="outline"
              onClick={() => setShowCancelConfirm(false)}
              disabled={isCancelling}
            >
              {t("subscription.keepSubscription")}
            </Button>

            <Button
              variant="destructive"
              onClick={handleCancelSubscription}
              disabled={isCancelling}
            >
              {isCancelling ? (
                <>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  {t("subscription.cancelling")}
                </>
              ) : (
                t("subscription.confirmCancel")
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
