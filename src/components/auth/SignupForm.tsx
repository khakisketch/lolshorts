import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useAuthStore } from "@/lib/auth";
import { Button } from "@/components/ui/button";

interface SignupFormProps {
  onSwitchToLogin?: () => void;
  onSuccess?: () => void;
}

export function SignupForm({ onSwitchToLogin, onSuccess }: SignupFormProps) {
  const { t } = useTranslation();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [confirmationEmail, setConfirmationEmail] = useState<string | null>(
    null,
  );
  const { signup, isLoading, error, clearError } = useAuthStore();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    clearError();

    try {
      const result = await signup({
        email,
        password,
        confirm_password: confirmPassword,
      });
      if (result === "confirmation_required") {
        setConfirmationEmail(email);
        return;
      }
      onSuccess?.();
    } catch {
      // Error is already set in store
    }
  };

  if (confirmationEmail) {
    return (
      <div className="gaming-panel p-6 w-full max-w-md border-0 shadow-none">
        <div
          role="status"
          data-testid="signup-confirmation-required"
          className="space-y-4"
        >
          <div>
            <h3 className="text-lg font-semibold">
              {t("auth.confirmEmailTitle")}
            </h3>
            <p className="mt-2 text-sm text-muted-foreground">
              {t("auth.confirmEmailDescription", { email: confirmationEmail })}
            </p>
          </div>
          {onSwitchToLogin && (
            <Button
              type="button"
              className="w-full"
              onClick={onSwitchToLogin}
              data-testid="signup-confirmation-login"
            >
              {t("auth.returnToLogin")}
            </Button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="gaming-panel p-6 w-full max-w-md border-0 shadow-none">
      <div className="mb-4">
        <h3 className="text-lg font-semibold">{t("auth.signupTitle")}</h3>
        <p className="text-sm text-muted-foreground">
          {t("auth.signupDescription")}
        </p>
      </div>
      <div>
        <form onSubmit={handleSubmit} className="space-y-4">
          {error && (
            <div className="bg-destructive/15 text-destructive px-4 py-3 rounded-md text-sm">
              {error.startsWith("errors.") ? t(error) : error}
            </div>
          )}

          <div className="space-y-2">
            <label htmlFor="email" className="text-sm font-medium">
              {t("auth.email")}
            </label>
            <input
              id="email"
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@example.com"
              required
              data-testid="signup-email-input"
              className="w-full px-3 py-2 border border-input rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-ring"
            />
          </div>

          <div className="space-y-2">
            <label htmlFor="password" className="text-sm font-medium">
              {t("auth.password")}
            </label>
            <input
              id="password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="••••••••"
              required
              minLength={8}
              data-testid="signup-password-input"
              className="w-full px-3 py-2 border border-input rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-ring"
            />
            <p className="text-xs text-muted-foreground">
              {t("auth.passwordMinLength")}
            </p>
          </div>

          <div className="space-y-2">
            <label htmlFor="confirmPassword" className="text-sm font-medium">
              {t("auth.confirmPassword")}
            </label>
            <input
              id="confirmPassword"
              type="password"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              placeholder="••••••••"
              required
              minLength={8}
              data-testid="confirm-password-input"
              className="w-full px-3 py-2 border border-input rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-ring"
            />
          </div>

          <Button
            type="submit"
            disabled={isLoading}
            className="w-full"
            data-testid="sign-up-button"
          >
            {isLoading ? t("auth.signingUp") : t("auth.signup")}
          </Button>

          {onSwitchToLogin && (
            <div className="text-center text-sm">
              {t("auth.haveAccount")}{" "}
              <button
                type="button"
                onClick={onSwitchToLogin}
                className="text-primary hover:underline"
              >
                {t("auth.login")}
              </button>
            </div>
          )}

          <p className="text-xs text-center text-muted-foreground mt-4">
            {t("auth.termsAgreement")}
          </p>
        </form>
      </div>
    </div>
  );
}
