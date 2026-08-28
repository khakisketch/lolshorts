import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import {
  CheckCircle2,
  Download,
  Play,
  RefreshCw,
  Upload,
  LayoutDashboard,
  AlertCircle,
  ShieldCheck,
} from "lucide-react";
import { formatStorage } from "@/lib/utils";
import { utilsApi } from "@/api/utils";
import { logger } from "@/lib/logger";
import {
  AutoEditOutput,
  AutoEditResult as AutoEditResultType,
  PlatformPreset,
} from "@/types/autoEdit";
import { ShareDialog } from "@/components/results/ShareDialog";
import { videoApi } from "@/api/video";

interface AutoEditResultProps {
  result: AutoEditResultType;
  onStartNew: () => void;
  onRegenerate: () => void;
}

function ReadinessCheck({
  label,
  passed,
  message,
}: {
  label: string;
  passed: boolean;
  message: string;
}) {
  return (
    <div className="flex items-center justify-between text-xs py-1">
      <div className="flex items-center gap-2">
        {passed ? (
          <CheckCircle2 className="w-3 h-3 text-green-500" />
        ) : (
          <AlertCircle className="w-3 h-3 text-yellow-500" />
        )}
        <span className={passed ? "text-foreground" : "text-muted-foreground"}>
          {label}
        </span>
      </div>
      <span
        className={`text-[10px] ${passed ? "text-green-600" : "text-yellow-600"}`}
      >
        {message}
      </span>
    </div>
  );
}

export function AutoEditResult({
  result,
  onStartNew,
  onRegenerate,
}: AutoEditResultProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [isShareDialogOpen, setIsShareDialogOpen] = useState(false);
  const [exportingPreset, setExportingPreset] = useState<PlatformPreset | null>(
    null,
  );
  const [shareTarget, setShareTarget] = useState({
    path: result.output_path,
    resultId: result.job_id,
  });
  const outputs: AutoEditOutput[] = result.outputs?.length
    ? result.outputs
    : [
        {
          result_id: result.job_id,
          output_path: result.output_path,
          duration: result.duration,
          clips_used: result.clips_used,
          file_size_bytes: result.file_size_bytes,
          output_kind: "short",
        },
      ];
  const computeReadiness = () => {
    const checks = [
      {
        label: t("autoEdit.readiness.duration", "Duration"),
        passed: result.duration > 0,
        message:
          result.duration > 0
            ? t("autoEdit.readiness.durationOk", "Verified")
            : t("autoEdit.readiness.durationWarn", "Needs review"),
      },
      {
        label: t("autoEdit.readiness.clips", "Clip Count"),
        passed: result.clips_used > 0,
        message:
          result.clips_used > 0
            ? t("autoEdit.readiness.clipsOk", "Verified")
            : t("autoEdit.readiness.clipsWarn", "Needs review"),
      },
      {
        label: t("autoEdit.readiness.output", "Output file"),
        passed: result.output_path.length > 0,
        message: result.output_path
          ? t("autoEdit.readiness.outputOk", "Created")
          : t("autoEdit.readiness.outputWarn", "Missing"),
      },
      {
        label: t("autoEdit.readiness.fileSize", "File data"),
        passed: result.file_size_bytes > 0,
        message:
          result.file_size_bytes > 0
            ? t("autoEdit.readiness.fileSizeOk", "Verified")
            : t("autoEdit.readiness.fileSizeWarn", "Needs review"),
      },
    ];
    return { isReady: checks.every((check) => check.passed), checks };
  };

  const readiness = computeReadiness();

  const handleOpenLocation = async () => {
    try {
      await utilsApi.showInFolder(result.output_path);
    } catch (error) {
      logger.error("Failed to show in folder:", error);
      alert(t("errors.failedToOpenFileLocation", { error: String(error) }));
    }
  };

  const handlePlayVideo = async () => {
    try {
      await utilsApi.openFileWithDefaultApp(result.output_path);
    } catch (error) {
      logger.error("Failed to open video:", error);
      alert(t("errors.failedToPlayVideo", { error: String(error) }));
    }
  };

  const handlePlatformExport = async (
    preset: Exclude<PlatformPreset, "youtube_shorts">,
  ) => {
    try {
      setExportingPreset(preset);
      const receipt = await videoApi.startPlatformExport(
        outputs[0].result_id,
        preset,
      );
      for (let attempt = 0; attempt < 240; attempt += 1) {
        const job = await videoApi.getMediaJob(receipt.job_id);
        if (job.status === "complete") {
          const path = job.parts[0]?.output_path;
          if (path) await utilsApi.showInFolder(path);
          return;
        }
        if (job.status === "failed" || job.status === "discarded") {
          throw new Error(job.error_message ?? t("platformExport.failed"));
        }
        await new Promise((resolve) => window.setTimeout(resolve, 500));
      }
      throw new Error(t("platformExport.failed"));
    } catch (error) {
      logger.error("Failed to export platform copy:", error);
      alert(t("errors.exportFailed", { error: String(error) }));
    } finally {
      setExportingPreset(null);
    }
  };

  const handleShare = async (output: AutoEditOutput) => {
    const report = await videoApi.revalidateAutoEditResult(output.result_id);
    if (!report || report.status === "unknown") {
      alert(t("outputValidation.unknown"));
      return;
    }
    if (report.status === "invalid") {
      alert(report.issues.map((issue) => issue.message).join("\n"));
      return;
    }
    if (
      report.status === "warning" &&
      !window.confirm(t("platformExport.confirmWarning"))
    )
      return;
    setShareTarget({ path: output.output_path, resultId: output.result_id });
    setIsShareDialogOpen(true);
  };

  return (
    <div className="max-w-2xl mx-auto space-y-6" data-testid="result-section">
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2 text-green-600">
            <CheckCircle2 className="w-6 h-6" />
            {t("autoEdit.shortGeneratedSuccessfully")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("autoEdit.shortReady")}
          </p>
        </div>
        <div className="space-y-6">
          {/* Result Details */}
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1">
              <div className="text-sm text-muted-foreground">
                {t("autoEdit.duration")}
              </div>
              <div className="text-2xl font-bold">
                {Math.round(result.duration)}s
              </div>
            </div>
            <div className="space-y-1">
              <div className="text-sm text-muted-foreground">
                {t("autoEdit.clipsUsed")}
              </div>
              <div className="text-2xl font-bold">{result.clips_used}</div>
            </div>
            <div className="space-y-1">
              <div className="text-sm text-muted-foreground">
                {t("autoEdit.fileSize")}
              </div>
              <div className="text-2xl font-bold">
                {formatStorage(result.file_size_bytes)}
              </div>
            </div>
            <div className="space-y-1">
              <div className="text-sm text-muted-foreground">
                {t("autoEdit.jobId")}
              </div>
              <div className="text-xs font-mono truncate">{result.job_id}</div>
            </div>
          </div>

          <Separator />

          {/* Shorts Readiness */}
          <div className="p-4 bg-black/40 rounded-lg border border-white/5">
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2 font-medium">
                <ShieldCheck
                  className={`w-4 h-4 ${readiness.isReady ? "text-green-500" : "text-yellow-500"}`}
                />
                {t("autoEdit.shortsReadiness", "Shorts Planning Check")}
              </div>
              <Badge
                variant={readiness.isReady ? "default" : "secondary"}
                className={readiness.isReady ? "bg-green-600" : ""}
              >
                {readiness.isReady
                  ? t("autoEdit.readiness.ready", "Looks Good")
                  : t("autoEdit.readiness.notReady", "Needs Review")}
              </Badge>
            </div>
            <div className="space-y-1">
              {readiness.checks.map((check) => (
                <ReadinessCheck key={check.label} {...check} />
              ))}
            </div>
          </div>

          {/* Output Path */}
          <div className="space-y-2">
            <Label className="text-sm font-medium">
              {t("autoEdit.outputFile")}
            </Label>
            <div className="p-3 bg-muted rounded-lg">
              <code
                className="text-xs break-all"
                data-testid="output-file-path"
              >
                {result.output_path}
              </code>
            </div>
          </div>

          {outputs.length > 1 && (
            <div className="space-y-2" data-testid="series-outputs">
              <Label className="text-sm font-medium">
                {t("autoEdit.result.seriesOutputs", "Series outputs")} (
                {outputs.length})
              </Label>
              {outputs.map((output, index) => (
                <div
                  key={output.result_id}
                  className="flex items-center gap-2 rounded-lg border border-white/10 p-3"
                >
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium">
                      {t("autoEdit.result.part", "Part")}{" "}
                      {output.part_index ?? index + 1}/
                      {output.part_count ?? outputs.length}
                    </div>
                    <div className="truncate text-xs text-muted-foreground">
                      {output.output_path}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() =>
                      utilsApi.openFileWithDefaultApp(output.output_path)
                    }
                  >
                    <Play className="h-3.5 w-3.5" />
                    <span className="sr-only">{t("autoEdit.playVideo")}</span>
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => void handleShare(output)}
                  >
                    <Upload className="h-3.5 w-3.5" />
                    <span className="sr-only">
                      {t("autoEdit.result.sharePart", "Share part")}
                    </span>
                  </Button>
                </div>
              ))}
            </div>
          )}

          {/* Actions */}
          <div className="flex flex-col gap-3">
            <div className="flex gap-3">
              <Button
                onClick={handleOpenLocation}
                className="flex-1"
                data-testid="open-location-button"
              >
                <Download className="w-4 h-4 mr-2" />
                {t("autoEdit.openFileLocation")}
              </Button>
              <Button
                onClick={handlePlayVideo}
                variant="outline"
                className="flex-1"
                data-testid="play-video-button"
              >
                <Play className="w-4 h-4 mr-2" />
                {t("autoEdit.playVideo")}
              </Button>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <Button
                type="button"
                variant="outline"
                disabled={exportingPreset !== null}
                onClick={() => void handlePlatformExport("tiktok")}
              >
                TikTok{" "}
                {exportingPreset === "tiktok"
                  ? t("platformExport.exporting")
                  : t("common.export", "Export")}
              </Button>
              <Button
                type="button"
                variant="outline"
                disabled={exportingPreset !== null}
                onClick={() => void handlePlatformExport("instagram_reels")}
              >
                Reels{" "}
                {exportingPreset === "instagram_reels"
                  ? t("platformExport.exporting")
                  : t("common.export", "Export")}
              </Button>
            </div>
            <div className="flex gap-3">
              <Button
                onClick={() => navigate({ to: "/results" })}
                className="flex-1"
                variant="secondary"
              >
                <LayoutDashboard className="w-4 h-4 mr-2" />
                {t("results.title")}
              </Button>
              <Button
                onClick={() => void handleShare(outputs[0])}
                className="flex-1"
                variant="default"
                data-testid="share-result-button"
              >
                <Upload className="w-4 h-4 mr-2" />
                {t("youtube.upload.uploadToYouTube")}
              </Button>
            </div>
          </div>

          <div className="flex gap-3">
            <Button onClick={onRegenerate} variant="outline" className="flex-1">
              <RefreshCw className="w-4 h-4 mr-2" />
              {t("autoEdit.regenerate", "Regenerate with Same Config")}
            </Button>
            <Button onClick={onStartNew} variant="outline" className="flex-1">
              <RefreshCw className="w-4 h-4 mr-2" />
              {t("autoEdit.createAnotherShort")}
            </Button>
          </div>
        </div>
      </div>
      <ShareDialog
        open={isShareDialogOpen}
        onOpenChange={setIsShareDialogOpen}
        videoPath={shareTarget.path}
        resultId={shareTarget.resultId}
      />
    </div>
  );
}
