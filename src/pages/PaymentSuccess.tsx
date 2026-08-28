import { useEffect, useMemo, useState } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { AlertCircle, CheckCircle2, Loader2 } from "lucide-react";
import { paymentApi } from "@/api/payment";
import { useAuthStore } from "@/lib/auth";

interface PaymentSuccessSearchParams {
  paymentKey?: string;
  orderId?: string;
  amount?: string;
}

export function PaymentSuccess() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const searchParams = useSearch({
    from: "/payment/success",
  }) as PaymentSuccessSearchParams;
  const refreshEntitlement = useAuthStore((state) => state.refreshEntitlement);
  const [status, setStatus] = useState<
    "confirming" | "confirmed" | "missing" | "failed"
  >("confirming");
  const [message, setMessage] = useState(
    "Confirming payment with the billing server.",
  );

  const amount = useMemo(
    () => Number(searchParams.amount),
    [searchParams.amount],
  );

  useEffect(() => {
    let cancelled = false;

    const confirm = async () => {
      if (
        !searchParams.paymentKey ||
        !searchParams.orderId ||
        !Number.isFinite(amount) ||
        amount <= 0
      ) {
        setStatus("missing");
        setMessage(
          "Payment redirect did not include the required Toss confirmation parameters.",
        );
        return;
      }

      setStatus("confirming");
      setMessage("Confirming payment with the billing server.");

      try {
        await paymentApi.confirmPayment(
          searchParams.paymentKey,
          searchParams.orderId,
          amount,
        );
        if (typeof refreshEntitlement === "function") {
          await refreshEntitlement();
        }
        if (!cancelled) {
          setStatus("confirmed");
          setMessage(
            "Payment was confirmed by the server. PRO status will appear only after Supabase user_licenses is active.",
          );
        }
      } catch (error) {
        if (!cancelled) {
          setStatus("failed");
          setMessage(
            error instanceof Error
              ? error.message
              : "Payment confirmation failed.",
          );
        }
      }
    };

    void confirm();

    return () => {
      cancelled = true;
    };
  }, [
    amount,
    refreshEntitlement,
    searchParams.orderId,
    searchParams.paymentKey,
  ]);

  const isConfirmed = status === "confirmed";
  const isConfirming = status === "confirming";

  return (
    <div className="min-h-screen flex items-center justify-center p-6 bg-[hsl(240,18%,9%)]">
      <div className="gaming-panel p-6 w-full max-w-md">
        <div className="flex items-center gap-2 mb-1">
          {isConfirming ? (
            <Loader2 className="w-6 h-6 animate-spin text-gaming-magenta" />
          ) : isConfirmed ? (
            <CheckCircle2 className="w-6 h-6 text-green-500" />
          ) : (
            <AlertCircle className="w-6 h-6 text-gaming-magenta" />
          )}
          <h2 className="text-lg font-semibold text-gaming-magenta">
            {isConfirming
              ? "Confirming payment"
              : isConfirmed
                ? "Payment confirmed"
                : "Payment not activated"}
          </h2>
        </div>
        <p className="text-sm text-muted-foreground mb-6">{message}</p>

        <div className="space-y-4">
          <Alert variant={isConfirmed ? "default" : "destructive"}>
            <AlertCircle className="w-4 h-4" />
            <AlertDescription>
              The app never grants PRO from redirect parameters alone. Access is
              refreshed from Supabase user_licenses after server-side Toss
              confirmation.
            </AlertDescription>
          </Alert>

          <div className="pt-2">
            <Button
              onClick={() => navigate({ to: "/settings" })}
              disabled={isConfirming}
              className="w-full"
            >
              {t("paymentSuccess.error.goToSettings")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
