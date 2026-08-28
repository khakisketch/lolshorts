/**
 * Recording Controls Component
 *
 * Manual control interface for recording operations
 */

import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Label } from "@/components/ui/label";
import { recordingApi } from "@/api/recording";
import { settingsApi } from "@/api/settings";
import { Play, Square, Save, Settings, AlertCircle } from "lucide-react";
import { toast } from "@/components/ui/use-toast";
import { VideoQuality, RecordingSettings } from "@/types";
import { getErrorMessage } from "@/lib/utils";
import { useRecordingStore } from "@/stores/recordingStore";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

// Simplified interface for this simplified UI component
interface SimpleSettings {
  audio_enabled: boolean;
  audio_device_id: string;
  video_quality: VideoQuality;
  hardware_encoding: boolean;
}

export function RecordingControls() {
  const { t } = useTranslation();
  const {
    status: { isRecording },
    readiness,
    audioActive,
    micActive,
  } = useRecordingStore();
  const [isLoading, setIsLoading] = useState(false);
  const [replayDuration, setReplayDuration] = useState(60);

  const criticalBlockers =
    readiness?.blockers.filter((b) => b.severity === "critical") || [];
  const warnings =
    readiness?.blockers.filter((b) => b.severity === "warning") || [];
  const isBlocked = criticalBlockers.length > 0;
  const hasWarnings = warnings.length > 0;

  // Store full settings to preserve other fields
  const [fullSettings, setFullSettings] = useState<RecordingSettings | null>(
    null,
  );

  // Local state for UI
  const [simpleSettings, setSimpleSettings] = useState<SimpleSettings>({
    audio_enabled: true,
    audio_device_id: "default",
    video_quality: "high",
    hardware_encoding: true,
  });

  // Load settings on mount
  useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
    try {
      const settings = await settingsApi.getRecordingSettings();
      setFullSettings(settings);

      // Map Nested -> Flat
      setSimpleSettings({
        audio_enabled:
          settings.audio.record_system_audio ||
          settings.audio.record_microphone,
        audio_device_id: settings.audio.system_audio_device || "default",
        video_quality: "high", // Note: This mapping is lossy as backend doesn't have explicit 'quality' enum field in top level
        hardware_encoding: settings.video.encoder !== "software",
      });
    } catch {
      // Settings load failure - will use defaults
    }
  };

  const getRecordingErrorMessage = (
    error: unknown,
  ): { title: string; description: string } => {
    const msg = getErrorMessage(error).toLowerCase();
    if (
      msg.includes("game") ||
      msg.includes("lcu") ||
      msg.includes("not detected") ||
      (msg.includes("not found") && msg.includes("process"))
    ) {
      return {
        title: t("errors.gameNotDetected"),
        description: t("errors.gameNotDetectedHint"),
      };
    }
    if (
      msg.includes("ffmpeg") ||
      msg.includes("binary") ||
      msg.includes("executable")
    ) {
      return {
        title: t("errors.ffmpegNotFound"),
        description: t("errors.ffmpegNotFoundHint"),
      };
    }
    if (
      msg.includes("audio") ||
      msg.includes("device") ||
      msg.includes("microphone")
    ) {
      return {
        title: t("errors.audioDeviceNotFound"),
        description: t("errors.audioDeviceNotFoundHint"),
      };
    }
    return {
      title: t("recordingControls.toast.failedToStart"),
      description: getErrorMessage(error),
    };
  };

  const handleStartAutoCapture = async () => {
    setIsLoading(true);
    try {
      await recordingApi.startAutoCapture();
      toast({
        title: t("recordingControls.toast.autoCaptureStarted"),
        description: t("recordingControls.toast.autoCaptureStartedDesc"),
      });
    } catch (error) {
      const { title, description } = getRecordingErrorMessage(error);
      toast({
        title,
        description,
        variant: "destructive",
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handleStopAutoCapture = async () => {
    setIsLoading(true);
    try {
      await recordingApi.stopAutoCapture();
      toast({
        title: t("recordingControls.toast.autoCaptureStopped"),
        description: t("recordingControls.toast.autoCaptureStoppedDesc"),
      });
    } catch (error) {
      toast({
        title: t("recordingControls.toast.failedToStop"),
        description: getErrorMessage(error),
        variant: "destructive",
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handleSaveReplay = async () => {
    setIsLoading(true);
    try {
      await recordingApi.saveReplay(replayDuration);
      toast({
        title: t("recordingControls.toast.replaySaved"),
        description: t("recordingControls.toast.replaySavedDesc", {
          duration: replayDuration,
        }),
      });
    } catch (error) {
      toast({
        title: t("recordingControls.toast.failedToSave"),
        description: getErrorMessage(error),
        variant: "destructive",
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handleSaveSettings = async () => {
    if (!fullSettings) return;

    setIsLoading(true);
    try {
      // Map Flat -> Nested (preserve other fields from fullSettings)
      const newSettings: RecordingSettings = {
        ...fullSettings,
        audio: {
          ...fullSettings.audio,
          record_system_audio: simpleSettings.audio_enabled,
          system_audio_device:
            simpleSettings.audio_device_id === "default"
              ? null
              : simpleSettings.audio_device_id,
        },
        video: {
          ...fullSettings.video,
          // Simple logic: if hardware encoding enabled, assume auto (which prefers GPU)
          encoder: simpleSettings.hardware_encoding ? "auto" : "software",
          // Note: video_quality mapping logic would go here if we implemented presets fully
        },
      };

      await settingsApi.saveRecordingSettings(newSettings);
      setFullSettings(newSettings);

      toast({
        title: t("recordingControls.toast.settingsSaved"),
        description: t("recordingControls.toast.settingsSavedDesc"),
      });
    } catch (error) {
      toast({
        title: t("recordingControls.toast.failedToSaveSettings"),
        description: getErrorMessage(error),
        variant: "destructive",
      });
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div data-testid="recording-controls" className="space-y-4">
      {/* Auto-Capture Controls */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("recordingControls.autoCapture.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("recordingControls.autoCapture.description")}
          </p>
        </div>

        {/* Readiness Warning */}
        {(isBlocked || hasWarnings) && (
          <Alert
            variant={isBlocked ? "destructive" : "default"}
            className="mb-4 bg-black/40 border-white/10"
          >
            <AlertCircle
              className={`h-4 w-4 ${isBlocked ? "text-red-500" : "text-yellow-500"}`}
            />
            <AlertTitle
              className={isBlocked ? "text-red-500" : "text-yellow-500"}
            >
              {isBlocked
                ? t("recordingControls.readiness.blocked")
                : t("recordingControls.readiness.warning")}
            </AlertTitle>
            <AlertDescription className="text-xs">
              <ul className="list-disc list-inside space-y-1">
                {[...criticalBlockers, ...warnings].map((b, i) => (
                  // The backend ships these messages in English. Translate by the
                  // stable `id` (its `code`), falling back to the raw message so a
                  // new backend code shows something real instead of a bare key.
                  <li key={i}>
                    {t(`recordingControls.readiness.codes.${b.id}`, {
                      defaultValue: b.message,
                    })}
                  </li>
                ))}
              </ul>
            </AlertDescription>
          </Alert>
        )}

        <div className="space-y-4">
          <div className="flex gap-2">
            {!isRecording ? (
              <Button
                onClick={handleStartAutoCapture}
                disabled={isLoading || isBlocked}
                className="flex-1"
                data-testid="start-auto-capture"
              >
                <Play className="mr-2 h-4 w-4" />
                {t("recordingControls.autoCapture.start")}
              </Button>
            ) : (
              <Button
                onClick={handleStopAutoCapture}
                disabled={isLoading}
                variant="destructive"
                className="flex-1"
                data-testid="stop-auto-capture"
              >
                <Square className="mr-2 h-4 w-4" />
                {t("recordingControls.autoCapture.stop")}
              </Button>
            )}
          </div>

          <div className="text-sm text-muted-foreground">
            {isRecording
              ? `✓ ${t("recordingControls.autoCapture.active")}`
              : t("recordingControls.autoCapture.inactive")}
          </div>

          {isRecording && audioActive === false && (
            <Alert
              variant="destructive"
              className="bg-black/40 border-white/10"
            >
              <AlertCircle className="h-4 w-4 text-red-500" />
              <AlertDescription className="text-xs">
                {t("recordingControls.audioNotCaptured")}
              </AlertDescription>
            </Alert>
          )}

          {isRecording &&
            micActive === false &&
            fullSettings?.audio.record_microphone && (
              <Alert
                variant="destructive"
                className="bg-black/40 border-white/10"
              >
                <AlertCircle className="h-4 w-4 text-red-500" />
                <AlertDescription className="text-xs">
                  {t("recordingControls.micNotCaptured")}
                </AlertDescription>
              </Alert>
            )}
        </div>
      </div>

      {/* Manual Replay Save */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("recordingControls.manualReplay.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("recordingControls.manualReplay.description")}
          </p>
        </div>
        <div className="space-y-4">
          <div className="space-y-2">
            <div className="flex justify-between">
              <Label>
                {t("recordingControls.manualReplay.replayDuration")}
              </Label>
              <span className="text-sm text-muted-foreground">
                {replayDuration}s
              </span>
            </div>
            <Slider
              value={[replayDuration]}
              onValueChange={([value]) => setReplayDuration(value)}
              min={10}
              max={60}
              step={10}
              disabled={!isRecording}
            />
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>10s</span>
              <span>30s</span>
              <span>60s</span>
            </div>
          </div>

          <Button
            onClick={handleSaveReplay}
            disabled={!isRecording || isLoading || isBlocked}
            className="w-full"
            data-testid="save-replay-button"
          >
            <Save className="mr-2 h-4 w-4" />
            {t("recordingControls.manualReplay.saveReplay")}
          </Button>

          <div className="text-sm text-muted-foreground">
            {t("recordingControls.manualReplay.hotkeyHint")}
          </div>
        </div>
      </div>

      {/* Recording Settings */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <Settings className="h-5 w-5" />
            {t("recordingControls.settings.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("recordingControls.settings.description")}
          </p>
        </div>
        <div className="space-y-4">
          {/* Video Quality */}
          <div className="space-y-2">
            <Label>{t("recordingControls.settings.videoQuality")}</Label>
            <Select
              value={simpleSettings.video_quality}
              onValueChange={(value: VideoQuality) =>
                setSimpleSettings({ ...simpleSettings, video_quality: value })
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="low">
                  {t("recordingControls.settings.qualities.low")}
                </SelectItem>
                <SelectItem value="medium">
                  {t("recordingControls.settings.qualities.medium")}
                </SelectItem>
                <SelectItem value="high">
                  {t("recordingControls.settings.qualities.high")}
                </SelectItem>
                <SelectItem value="ultra">
                  {t("recordingControls.settings.qualities.ultra")}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* Hardware Encoding */}
          <div className="flex items-center justify-between">
            <Label>{t("recordingControls.settings.hardwareEncoding")}</Label>
            <Button
              variant={simpleSettings.hardware_encoding ? "default" : "outline"}
              size="sm"
              aria-pressed={simpleSettings.hardware_encoding}
              onClick={() =>
                setSimpleSettings({
                  ...simpleSettings,
                  hardware_encoding: !simpleSettings.hardware_encoding,
                })
              }
            >
              {simpleSettings.hardware_encoding
                ? t("recordingControls.settings.enabled")
                : t("recordingControls.settings.disabled")}
            </Button>
          </div>

          {/* Audio */}
          <div className="flex items-center justify-between">
            <Label>{t("recordingControls.settings.audioCapture")}</Label>
            <Button
              variant={simpleSettings.audio_enabled ? "default" : "outline"}
              size="sm"
              aria-pressed={simpleSettings.audio_enabled}
              onClick={() =>
                setSimpleSettings({
                  ...simpleSettings,
                  audio_enabled: !simpleSettings.audio_enabled,
                })
              }
            >
              {simpleSettings.audio_enabled
                ? t("recordingControls.settings.enabled")
                : t("recordingControls.settings.disabled")}
            </Button>
          </div>

          <Button
            onClick={handleSaveSettings}
            disabled={isLoading || !fullSettings}
            className="w-full"
          >
            {t("recordingControls.settings.saveSettings")}
          </Button>
        </div>
      </div>
    </div>
  );
}
