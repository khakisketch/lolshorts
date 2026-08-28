import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  RotateCcw,
  RefreshCw,
  XCircle,
} from "lucide-react";
import { VideoError as AutoEditErrorType } from "@/types/autoEdit";

interface AutoEditErrorProps {
  error: AutoEditErrorType;
  onRetry: () => void;
  onStartNew: () => void;
}

export function AutoEditErrorView({
  error,
  onRetry,
  onStartNew,
}: AutoEditErrorProps) {
  const { t } = useTranslation();
  const [showTechnicalDetails, setShowTechnicalDetails] = useState(false);

  return (
    <div className="max-w-2xl mx-auto" data-testid="error-section">
      <div className="gaming-panel p-6 border-gaming-magenta/50">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2 text-gaming-magenta">
            <XCircle className="w-5 h-5" />
            {t("autoEdit.videoGenerationFailed")}
          </h3>
          <p
            className="text-sm text-muted-foreground text-base"
            data-testid="error-message"
          >
            {error.message}
          </p>
        </div>

        <div className="space-y-4">
          {/* Recovery Suggestions */}
          <div data-testid="error-recovery-suggestions">
            <Label className="text-sm font-medium mb-2 block">
              {t("autoEdit.trySolutions")}
            </Label>
            <ul className="space-y-2">
              {error.recovery_suggestions.map((suggestion, idx) => (
                <li key={idx} className="flex items-start gap-2 text-sm">
                  <CheckCircle2 className="w-4 h-4 mt-0.5 flex-shrink-0 text-muted-foreground" />
                  <span>{suggestion}</span>
                </li>
              ))}
            </ul>
          </div>

          {/* Technical Details (expandable) */}
          {error.technical_details && (
            <div>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowTechnicalDetails(!showTechnicalDetails)}
                className="w-full justify-between"
              >
                <span className="text-sm font-medium">
                  {t("autoEdit.technicalDetails")}
                </span>
                {showTechnicalDetails ? (
                  <ChevronUp className="w-4 h-4" />
                ) : (
                  <ChevronDown className="w-4 h-4" />
                )}
              </Button>

              {showTechnicalDetails && (
                <Alert className="mt-2">
                  <AlertDescription className="text-xs font-mono whitespace-pre-wrap">
                    {error.technical_details}
                  </AlertDescription>
                </Alert>
              )}
            </div>
          )}

          <Separator />

          {/* Action Buttons */}
          <div className="flex gap-2">
            <Button
              onClick={onRetry}
              variant="default"
              className="flex-1"
              data-testid="retry-button"
            >
              <RotateCcw className="w-4 h-4 mr-2" />
              {t("autoEdit.retryGeneration")}
            </Button>

            <Button
              onClick={onStartNew}
              variant="outline"
              className="flex-1"
              data-testid="reset-button"
            >
              <RefreshCw className="w-4 h-4 mr-2" />
              {t("autoEdit.startOver")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
