import { useNavigate, useSearch } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { XCircle, AlertCircle } from "lucide-react";

interface PaymentFailSearchParams {
  code?: string;
  message?: string;
}

export function PaymentFail() {
  const { t } = useTranslation();
  const searchParams = useSearch({
    from: "/payment/fail",
  }) as PaymentFailSearchParams;
  const navigate = useNavigate();

  const errorCode = searchParams.code;
  const errorMessage = searchParams.message;

  const getErrorDetails = () => {
    if (!errorCode) {
      return {
        title: "Payment deferred",
        description:
          "Checkout is unavailable in this readiness build. No payment was attempted.",
      };
    }

    switch (errorCode) {
      case "PAY_PROCESS_CANCELED":
        return {
          title: t("paymentFail.errors.cancelled.title"),
          description: t("paymentFail.errors.cancelled.description"),
        };
      case "PAY_PROCESS_ABORTED":
        return {
          title: t("paymentFail.errors.aborted.title"),
          description: t("paymentFail.errors.aborted.description"),
        };
      case "REJECT_CARD_COMPANY":
        return {
          title: t("paymentFail.errors.cardDeclined.title"),
          description: t("paymentFail.errors.cardDeclined.description"),
        };
      default:
        return {
          title: t("paymentFail.errors.default.title"),
          description:
            errorMessage || t("paymentFail.errors.default.description"),
        };
    }
  };

  const errorDetails = getErrorDetails();

  const handleRetry = () => {
    sessionStorage.removeItem("pending_order_id");
    sessionStorage.removeItem("pending_amount");
    navigate({ to: "/settings" });
  };

  return (
    <div className="min-h-screen flex items-center justify-center p-6 bg-[hsl(240,18%,9%)]">
      <div className="gaming-panel p-6 w-full max-w-md">
        <div className="flex items-center gap-2 mb-1">
          <XCircle className="w-6 h-6 text-gaming-magenta" />
          <h2 className="text-lg font-semibold text-gaming-magenta">
            {errorDetails.title}
          </h2>
        </div>
        <p className="text-sm text-muted-foreground mb-6">
          {errorDetails.description}
        </p>

        <div className="space-y-4">
          {errorMessage && (
            <Alert variant="destructive">
              <AlertCircle className="w-4 h-4" />
              <AlertDescription>{errorMessage}</AlertDescription>
            </Alert>
          )}

          {errorCode && (
            <div className="text-xs text-muted-foreground">
              <p>
                {t("paymentFail.errorCode")} {errorCode}
              </p>
            </div>
          )}

          <div className="p-4 bg-black/40 border border-white/5 rounded-lg">
            <p className="text-sm font-semibold mb-2">
              {t("paymentFail.whatCanYouDo")}
            </p>
            <ul className="text-sm text-muted-foreground space-y-1 list-disc list-inside">
              <li>
                Return to Settings and continue with the current local
                workflows.
              </li>
              <li>Do not retry live checkout until payment QA is approved.</li>
            </ul>
          </div>

          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={() => navigate({ to: "/" })}
              className="flex-1"
            >
              {t("paymentFail.goHome")}
            </Button>
            <Button onClick={handleRetry} className="flex-1">
              {t("paymentFail.tryAgain")}
            </Button>
          </div>

          <p className="text-xs text-center text-muted-foreground">
            {t("paymentFail.noCharges")}
          </p>
        </div>
      </div>
    </div>
  );
}
