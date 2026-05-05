import { useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { AlertCircle } from "lucide-react";

export function PaymentSuccess() {
  const { t } = useTranslation();
  const navigate = useNavigate();

  return (
    <div className="min-h-screen flex items-center justify-center p-6 bg-[hsl(240,18%,9%)]">
      <div className="gaming-panel p-6 w-full max-w-md">
        <div className="flex items-center gap-2 mb-1">
          <AlertCircle className="w-6 h-6 text-gaming-magenta" />
          <h2 className="text-lg font-semibold text-gaming-magenta">Payment deferred</h2>
        </div>
        <p className="text-sm text-muted-foreground mb-6">
          Toss checkout and payment confirmation are intentionally disabled in this non-payment readiness build.
        </p>

        <div className="space-y-4">
          <Alert>
            <AlertCircle className="w-4 h-4" />
            <AlertDescription>
              No payment was confirmed by the app. PRO access can only be granted by Supabase user_licenses after a trusted server-side payment path is approved.
            </AlertDescription>
          </Alert>

          <div className="pt-2">
            <Button
              onClick={() => navigate({ to: "/settings" })}
              className="w-full"
            >
              {t('paymentSuccess.error.goToSettings')}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
