import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Copy, Check, ShieldAlert, Info, Loader2 } from "lucide-react";
import { utilsApi } from "@/api/utils";
import { youtubeApi } from "@/api/youtube";
import { lcuApi } from "@/api/lcu";
import { toast } from "@/components/ui/use-toast";
import { logger } from "@/lib/logger";

const disconnectedYouTubeAuthStatus = {
  authenticated: false,
  expires_at: null,
  has_refresh_token: false,
};

const formatMetric = (value: unknown): string => {
  return typeof value === "number" && Number.isFinite(value)
    ? value.toFixed(1)
    : "Unknown";
};

export function SupportSummary() {
  const { t } = useTranslation();
  const [isGenerating, setIsGenerating] = useState(false);
  const [copied, setCopied] = useState(false);

  const generateSummary = async () => {
    setIsGenerating(true);
    try {
      const [
        version,
        diagnostics,
        system,
        disk,
        lcuConnected,
        youtubeAuthStatus,
      ] = await Promise.all([
        utilsApi.getAppVersion(),
        utilsApi.getDiagnosticsStatus(),
        utilsApi.getSystemMetrics(),
        utilsApi.getDiskSpaceInfo(),
        lcuApi.checkStatus(),
        youtubeApi
          .getAuthStatus()
          .then((authStatus) => authStatus ?? disconnectedYouTubeAuthStatus)
          .catch(() => disconnectedYouTubeAuthStatus),
      ]);

      const summary = `
--- LoLShorts Support Summary ---
Timestamp: ${new Date().toISOString()}
App Version: ${version}
LCU Connected: ${lcuConnected}
YouTube Authenticated: ${youtubeAuthStatus.authenticated}

[Privacy Boundary]
Included: app version, LCU connection state, YouTube connected state, coarse system metrics, disk capacity, diagnostics.
Excluded: OAuth tokens, refresh tokens, passwords, payment keys, Supabase keys, signing keys, private media, screenshots, and personal files.

[System Metrics]
CPU: ${formatMetric(system?.total_cpu_percent)}%
RAM Available: ${formatMetric(system?.available_ram_gb)} GB
Disk Available: ${formatMetric(system?.available_disk_gb)} GB

[Disk Space Info]
Measured: ${disk?.known === true}
Total: ${formatMetric(disk?.known ? disk.total_gb : undefined)} GB
Used: ${formatMetric(disk?.known ? disk.used_gb : undefined)} GB
Available: ${formatMetric(disk?.known ? disk.available_gb : undefined)} GB

[Diagnostics]
Overall Status: ${diagnostics.overall_status}
Checks:
${diagnostics.checks.map((c) => `- ${c.label}: ${c.status} (${c.message})`).join("\\n")}

---------------------------------
`.trim();

      if (!navigator.clipboard?.writeText) {
        throw new Error("Clipboard API is unavailable");
      }

      await navigator.clipboard.writeText(summary);
      setCopied(true);
      toast({
        title: t("support.summary.copiedTitle", "Summary Copied"),
        description: t(
          "support.summary.copiedDesc",
          "You can now paste this into your support ticket.",
        ),
      });
      setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      logger.error("Failed to generate support summary:", error);
      toast({
        title: t("support.summary.errorTitle", "Error"),
        description: t(
          "support.summary.errorDesc",
          "Failed to generate support summary or copy it to the clipboard.",
        ),
        variant: "destructive",
      });
    } finally {
      setIsGenerating(false);
    }
  };

  return (
    <div className="gaming-panel p-6">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <ShieldAlert className="h-5 w-5 text-gaming-magenta" />
          <h3 className="text-lg font-bold">
            {t("support.summary.title", "Support & Diagnostics")}
          </h3>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={generateSummary}
          disabled={isGenerating}
          className="flex items-center gap-2"
        >
          {isGenerating ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : copied ? (
            <Check className="h-4 w-4 text-green-400" />
          ) : (
            <Copy className="h-4 w-4" />
          )}
          {copied
            ? t("support.summary.copied", "Copied!")
            : t("support.summary.copyButton", "Copy Support Summary")}
        </Button>
      </div>

      <Alert className="bg-black/20 border-white/5">
        <Info className="h-4 w-4 text-muted-foreground" />
        <AlertDescription className="text-xs text-muted-foreground">
          {t(
            "support.summary.disclaimer",
            "The support summary contains app version, connection state, system metrics, disk capacity, and diagnostic checks. It does NOT include passwords, OAuth tokens, refresh tokens, payment keys, private keys, private media, screenshots, or personal files.",
          )}
        </AlertDescription>
      </Alert>
    </div>
  );
}
