import { useTranslation } from "react-i18next";
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import { Info } from "lucide-react";
import { logger } from '@/lib/logger';

type Resolution = "r1920x1080" | "r2560x1440" | "r3840x2160";
type FrameRate = "fps30" | "fps60" | "fps120" | "fps144";
type BitratePreset = "low" | "medium" | "high" | "very_high";
type VideoCodec = "h264" | "h265" | "av1";
type EncoderPreference = "auto" | "nvenc" | "qsv" | "amf" | "software";

interface VideoSettings {
  resolution: Resolution;
  frame_rate: FrameRate;
  bitrate_preset: BitratePreset;
  codec: VideoCodec;
  encoder: EncoderPreference;
}

interface VideoSettingsProps {
  settings: VideoSettings;
  onChange: (settings: VideoSettings) => void;
}

interface EncoderInfo {
  id: string;
  name: string;
  type: "hardware" | "software";
  vendor: string;
  codecs: string[];
  performance: string;
  quality: string;
}

interface EncoderDetectionResult {
  available: EncoderInfo[];
  auto_detected: string;
  total_count: number;
}

export function VideoSettings({ settings, onChange }: VideoSettingsProps) {
  const { t } = useTranslation();
  const [availableEncoders, setAvailableEncoders] = useState<EncoderInfo[]>([]);
  const [autoDetected, setAutoDetected] = useState<string>("software");
  const [isDetecting, setIsDetecting] = useState(true);

  // Detect available encoders on mount
  useEffect(() => {
    invoke<EncoderDetectionResult>("detect_available_encoders")
      .then((result) => {
        setAvailableEncoders(result.available);
        setAutoDetected(result.auto_detected);
        setIsDetecting(false);
      })
      .catch((error) => {
        logger.error("Failed to detect encoders:", error);
        setIsDetecting(false);
      });
  }, []);

  const updateSetting = <K extends keyof VideoSettings>(
    key: K,
    value: VideoSettings[K]
  ) => {
    onChange({ ...settings, [key]: value });
  };

  const getResolutionLabel = (res: Resolution): string => {
    const labels: Record<Resolution, string> = {
      r1920x1080: t('settings.recordingConfig.videoSettings.resolution.labels.r1920x1080'),
      r2560x1440: t('settings.recordingConfig.videoSettings.resolution.labels.r2560x1440'),
      r3840x2160: t('settings.recordingConfig.videoSettings.resolution.labels.r3840x2160'),
    };
    return labels[res];
  };

  const getFrameRateLabel = (fps: FrameRate): string => {
    const labels: Record<FrameRate, string> = {
      fps30: t('settings.recordingConfig.videoSettings.frameRate.labels.fps30'),
      fps60: t('settings.recordingConfig.videoSettings.frameRate.labels.fps60'),
      fps120: t('settings.recordingConfig.videoSettings.frameRate.labels.fps120'),
      fps144: t('settings.recordingConfig.videoSettings.frameRate.labels.fps144'),
    };
    return labels[fps];
  };

  const getBitrateLabel = (bitrate: BitratePreset): string => {
    const labels: Record<BitratePreset, string> = {
      low: t('settings.recordingConfig.videoSettings.bitratePreset.labels.low'),
      medium: t('settings.recordingConfig.videoSettings.bitratePreset.labels.medium'),
      high: t('settings.recordingConfig.videoSettings.bitratePreset.labels.high'),
      very_high: t('settings.recordingConfig.videoSettings.bitratePreset.labels.veryHigh'),
    };
    return labels[bitrate];
  };

  const getCodecLabel = (codec: VideoCodec): string => {
    const labels: Record<VideoCodec, string> = {
      h264: t('settings.recordingConfig.videoSettings.videoCodec.labels.h264'),
      h265: t('settings.recordingConfig.videoSettings.videoCodec.labels.h265'),
      av1: t('settings.recordingConfig.videoSettings.videoCodec.labels.av1'),
    };
    return labels[codec];
  };

  const getEncoderLabel = (encoder: EncoderPreference): string => {
    const labels: Record<EncoderPreference, string> = {
      auto: t('settings.recordingConfig.videoSettings.encoderPreference.labels.auto'),
      nvenc: t('settings.recordingConfig.videoSettings.encoderPreference.labels.nvenc'),
      qsv: t('settings.recordingConfig.videoSettings.encoderPreference.labels.qsv'),
      amf: t('settings.recordingConfig.videoSettings.encoderPreference.labels.amf'),
      software: t('settings.recordingConfig.videoSettings.encoderPreference.labels.software'),
    };

    // Show detected hardware for "auto" option
    if (encoder === "auto" && !isDetecting && autoDetected !== "software") {
      const detectedEncoder = availableEncoders.find(e => e.id === autoDetected);
      if (detectedEncoder) {
        return `${labels[encoder]} (${detectedEncoder.name})`;
      }
    }

    return labels[encoder];
  };

  // Check if an encoder is available
  const isEncoderAvailable = (encoderId: string): boolean => {
    if (encoderId === "auto") return true; // Auto is always available
    return availableEncoders.some(e => e.id === encoderId);
  };

  const getEstimatedSize = (): string => {
    const bitrateMap: Record<BitratePreset, number> = {
      low: 10,
      medium: 20,
      high: 40,
      very_high: 80,
    };

    const mbps = bitrateMap[settings.bitrate_preset];
    const mbPerMinute = (mbps * 60) / 8; // Convert Mbps to MB per minute

    return `~${mbPerMinute.toFixed(0)} MB/min`;
  };

  return (
    <div className="space-y-6">
      {/* Resolution */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.videoSettings.resolution.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.videoSettings.resolution.description')}
          </p>
        </div>
        <div>
          <div className="flex items-center gap-4">
            <div className="flex-1">
              <Select
                value={settings.resolution}
                onValueChange={(value) => updateSetting("resolution", value as Resolution)}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="r1920x1080">
                    {getResolutionLabel("r1920x1080")}
                  </SelectItem>
                  <SelectItem value="r2560x1440">
                    {getResolutionLabel("r2560x1440")}
                  </SelectItem>
                  <SelectItem value="r3840x2160">
                    {getResolutionLabel("r3840x2160")}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            {settings.resolution === "r1920x1080" && (
              <Badge variant="secondary">{t('settings.recordingConfig.videoSettings.resolution.recommended')}</Badge>
            )}
          </div>
        </div>
      </div>

      {/* Frame Rate */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.videoSettings.frameRate.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.videoSettings.frameRate.description')}
          </p>
        </div>
        <div>
          <div className="flex items-center gap-4">
            <div className="flex-1">
              <Select
                value={settings.frame_rate}
                onValueChange={(value) => updateSetting("frame_rate", value as FrameRate)}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="fps30">{getFrameRateLabel("fps30")}</SelectItem>
                  <SelectItem value="fps60">{getFrameRateLabel("fps60")}</SelectItem>
                  <SelectItem value="fps120">{getFrameRateLabel("fps120")}</SelectItem>
                  <SelectItem value="fps144">{getFrameRateLabel("fps144")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {settings.frame_rate === "fps60" && (
              <Badge variant="secondary">{t('settings.recordingConfig.videoSettings.frameRate.recommended')}</Badge>
            )}
          </div>
        </div>
      </div>

      {/* Bitrate */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.videoSettings.bitratePreset.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.videoSettings.bitratePreset.description')}
          </p>
        </div>
        <div className="space-y-3">
          <Select
            value={settings.bitrate_preset}
            onValueChange={(value) => updateSetting("bitrate_preset", value as BitratePreset)}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="low">{getBitrateLabel("low")}</SelectItem>
              <SelectItem value="medium">{getBitrateLabel("medium")}</SelectItem>
              <SelectItem value="high">{getBitrateLabel("high")}</SelectItem>
              <SelectItem value="very_high">{getBitrateLabel("very_high")}</SelectItem>
            </SelectContent>
          </Select>

          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Info className="w-4 h-4" />
            <span>{t('settings.recordingConfig.videoSettings.bitratePreset.estimatedSize')}: {getEstimatedSize()}</span>
          </div>
        </div>
      </div>

      {/* Codec */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.videoSettings.videoCodec.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.videoSettings.videoCodec.description')}
          </p>
        </div>
        <div>
          <div className="flex items-center gap-4">
            <div className="flex-1">
              <Select
                value={settings.codec}
                onValueChange={(value) => updateSetting("codec", value as VideoCodec)}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="h264">{getCodecLabel("h264")}</SelectItem>
                  <SelectItem value="h265">{getCodecLabel("h265")}</SelectItem>
                  <SelectItem value="av1">{getCodecLabel("av1")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {settings.codec === "h265" && (
              <Badge variant="secondary">{t('settings.recordingConfig.videoSettings.videoCodec.recommended')}</Badge>
            )}
          </div>
        </div>
      </div>

      {/* Encoder */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.videoSettings.encoderPreference.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.videoSettings.encoderPreference.description')}
          </p>
        </div>
        <div>
          <div className="space-y-3">
            <div className="flex items-center gap-4">
              <div className="flex-1">
                <Select
                  value={settings.encoder}
                  onValueChange={(value) => updateSetting("encoder", value as EncoderPreference)}
                  disabled={isDetecting}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {/* Auto option always available */}
                    <SelectItem value="auto">{getEncoderLabel("auto")}</SelectItem>

                    {/* Only show available hardware encoders */}
                    {isEncoderAvailable("nvenc") && (
                      <SelectItem value="nvenc">
                        <div className="flex items-center gap-2">
                          <span>{getEncoderLabel("nvenc")}</span>
                          <Badge variant="outline" className="text-xs">Hardware</Badge>
                        </div>
                      </SelectItem>
                    )}

                    {isEncoderAvailable("qsv") && (
                      <SelectItem value="qsv">
                        <div className="flex items-center gap-2">
                          <span>{getEncoderLabel("qsv")}</span>
                          <Badge variant="outline" className="text-xs">Hardware</Badge>
                        </div>
                      </SelectItem>
                    )}

                    {isEncoderAvailable("amf") && (
                      <SelectItem value="amf">
                        <div className="flex items-center gap-2">
                          <span>{getEncoderLabel("amf")}</span>
                          <Badge variant="outline" className="text-xs">Hardware</Badge>
                        </div>
                      </SelectItem>
                    )}

                    {/* Software encoder always available */}
                    <SelectItem value="software">
                      <div className="flex items-center gap-2">
                        <span>{getEncoderLabel("software")}</span>
                        <Badge variant="secondary" className="text-xs">CPU</Badge>
                      </div>
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>
              {settings.encoder === "auto" && (
                <Badge variant="secondary">{t('settings.recordingConfig.videoSettings.encoderPreference.recommended')}</Badge>
              )}
            </div>

            {/* Hardware detection status */}
            {isDetecting && (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Info className="w-4 h-4" />
                <span>Detecting hardware encoders...</span>
              </div>
            )}

            {!isDetecting && availableEncoders.length > 1 && (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Info className="w-4 h-4" />
                <span>
                  {availableEncoders.filter(e => e.type === "hardware").length} hardware encoder(s) detected
                </span>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Info Card */}
      <div className="gaming-panel p-6">
        <div>
          <div className="space-y-2 text-sm">
            <p className="font-semibold">{t('settings.recordingConfig.videoSettings.currentConfiguration.title')}</p>
            <p className="text-muted-foreground">
              • {t('settings.recordingConfig.videoSettings.currentConfiguration.resolution')}: {getResolutionLabel(settings.resolution)}
            </p>
            <p className="text-muted-foreground">
              • {t('settings.recordingConfig.videoSettings.currentConfiguration.frameRate')}: {getFrameRateLabel(settings.frame_rate)}
            </p>
            <p className="text-muted-foreground">
              • {t('settings.recordingConfig.videoSettings.currentConfiguration.codec')}: {getCodecLabel(settings.codec)}
            </p>
            <p className="text-muted-foreground">
              • {t('settings.recordingConfig.videoSettings.currentConfiguration.encoder')}: {getEncoderLabel(settings.encoder)}
            </p>
            <p className="text-muted-foreground">
              • {t('settings.recordingConfig.videoSettings.currentConfiguration.estimatedSize')}: {getEstimatedSize()}
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
