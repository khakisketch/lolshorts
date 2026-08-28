import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Label } from "@/components/ui/label";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Crown, Check, AlertCircle, Loader2 } from "lucide-react";
import { paymentApi, type SubscriptionDetails } from "@/api/payment";

interface PaymentModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function PaymentModal({ isOpen, onClose }: PaymentModalProps) {
  const { t } = useTranslation();
  const [period, setPeriod] = useState<"MONTHLY" | "YEARLY">("MONTHLY");
  const [subscription, setSubscription] = useState<SubscriptionDetails | null>(
    null,
  );
  const [isLoading, setIsLoading] = useState(false);
  const [checkoutError, setCheckoutError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;

    let cancelled = false;
    setIsLoading(true);
    setCheckoutError(null);

    paymentApi
      .getSubscriptionDetails()
      .then((details) => {
        if (!cancelled) {
          setSubscription(details);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setCheckoutError(
            error instanceof Error
              ? error.message
              : "Unable to load billing status.",
          );
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [isOpen]);

  const paymentAvailable = subscription?.payment_available === true;
  const statusMessage =
    checkoutError ??
    subscription?.payment_message ??
    subscription?.reason ??
    "Payment and PRO upgrades are deferred until non-payment field readiness is verified. Local export and current app workflows remain available without starting checkout.";

  const handleCheckout = async () => {
    if (!paymentAvailable) return;

    setIsLoading(true);
    setCheckoutError(null);
    try {
      const checkoutUrl = await paymentApi.openPaymentPage(period);
      await open(checkoutUrl);
    } catch (error) {
      setCheckoutError(
        error instanceof Error
          ? error.message
          : "Unable to open payment checkout.",
      );
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Crown className="w-6 h-6 text-yellow-500" />
            {t("payment.upgradeToPro")}
          </DialogTitle>
          <DialogDescription>
            {t("payment.chooseSubscription")}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          <Alert>
            <AlertCircle className="w-4 h-4" />
            <AlertDescription>{statusMessage}</AlertDescription>
          </Alert>

          {/* Plan Selection (policy preview only) */}
          <RadioGroup
            value={period}
            onValueChange={(val) => setPeriod(val as "MONTHLY" | "YEARLY")}
          >
            <div className="space-y-3">
              {/* Monthly Plan */}
              <div
                role="button"
                tabIndex={0}
                className={`relative flex items-start p-4 border-2 rounded-lg cursor-pointer transition-colors ${
                  period === "MONTHLY"
                    ? "border-primary bg-primary/5"
                    : "border-border hover:border-primary/50"
                }`}
                onClick={() => setPeriod("MONTHLY")}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setPeriod("MONTHLY");
                  }
                }}
                aria-label={t("payment.monthlyPrice", { price: "₩9,900" })}
              >
                <RadioGroupItem value="MONTHLY" id="monthly" className="mt-1" />
                <Label htmlFor="monthly" className="flex-1 ml-3 cursor-pointer">
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="font-semibold">
                        {t("payment.monthlyPlan")}
                      </p>
                      <p className="text-sm text-muted-foreground">
                        {t("payment.monthlyPrice", { price: "₩9,900" })}
                      </p>
                    </div>
                    {period === "MONTHLY" && (
                      <Check className="w-5 h-5 text-primary" />
                    )}
                  </div>
                </Label>
              </div>

              {/* Yearly Plan */}
              <div
                role="button"
                tabIndex={0}
                className={`relative flex items-start p-4 border-2 rounded-lg cursor-pointer transition-colors ${
                  period === "YEARLY"
                    ? "border-primary bg-primary/5"
                    : "border-border hover:border-primary/50"
                }`}
                onClick={() => setPeriod("YEARLY")}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setPeriod("YEARLY");
                  }
                }}
                aria-label={`${t("payment.yearlyPrice", { price: "₩99,000" })}, ${t("payment.savePercent", { percent: 17 })}`}
              >
                <RadioGroupItem value="YEARLY" id="yearly" className="mt-1" />
                <Label htmlFor="yearly" className="flex-1 ml-3 cursor-pointer">
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="font-semibold flex items-center gap-2">
                        {t("payment.yearlyPlan")}
                        <span className="px-2 py-0.5 text-xs font-medium bg-green-500/10 text-green-600 dark:text-green-400 rounded">
                          {t("payment.savePercent", { percent: 17 })}
                        </span>
                      </p>
                      <p className="text-sm text-muted-foreground">
                        {t("payment.yearlyPrice", { price: "₩99,000" })}
                        <span className="ml-2 text-xs line-through opacity-60">
                          ₩118,800
                        </span>
                      </p>
                    </div>
                    {period === "YEARLY" && (
                      <Check className="w-5 h-5 text-primary" />
                    )}
                  </div>
                </Label>
              </div>
            </div>
          </RadioGroup>

          {/* Features List */}
          <div className="p-4 bg-muted/50 rounded-lg">
            <p className="font-semibold mb-3">{t("payment.features.title")}</p>
            <div className="space-y-2 text-sm">
              <div className="flex items-center gap-2">
                <Check className="w-4 h-4 text-green-500" />
                <span>{t("payment.features.unlimitedClips")}</span>
              </div>
              <div className="flex items-center gap-2">
                <Check className="w-4 h-4 text-green-500" />
                <span>{t("payment.features.advancedEditor")}</span>
              </div>
              <div className="flex items-center gap-2">
                <Check className="w-4 h-4 text-green-500" />
                <span>{t("payment.features.noWatermarks")}</span>
              </div>
              <div className="flex items-center gap-2">
                <Check className="w-4 h-4 text-green-500" />
                <span>{t("payment.features.prioritySupport")}</span>
              </div>
              <div className="flex items-center gap-2">
                <Check className="w-4 h-4 text-green-500" />
                <span>{t("payment.features.youtubeUpload")}</span>
              </div>
            </div>
          </div>

          {/* Action Buttons */}
          <div className="flex gap-3">
            <Button variant="outline" onClick={onClose} className="flex-1">
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              disabled={!paymentAvailable || isLoading}
              onClick={handleCheckout}
              className="flex-1"
            >
              {isLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t("payment.selectPlan")}
            </Button>
          </div>

          <p className="text-xs text-center text-muted-foreground">
            {paymentAvailable
              ? "Checkout opens in Toss. PRO activates only after Supabase entitlement refresh confirms active user_licenses."
              : "Payment checkout is intentionally unavailable in this readiness build."}
          </p>
        </div>
      </DialogContent>
    </Dialog>
  );
}
