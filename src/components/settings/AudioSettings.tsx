import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Mic, Volume2, AlertTriangle } from "lucide-react";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logger } from '@/lib/logger';

type SampleRate = "hz44100" | "hz48000";
type AudioBitrate = "kbps128" | "kbps192" | "kbps256" | "kbps320";

interface AudioDevice {
  name: string;
  device_type: "Microphone" | "SystemAudio";
}

interface AudioSettings {
  record_microphone: boolean;
  microphone_device: string | null;
  microphone_volume: number; // 0-200
  record_system_audio: boolean;
  system_audio_device: string | null;
  system_audio_volume: number; // 0-200
  sample_rate: SampleRate;
  bitrate: AudioBitrate;
}

interface AudioSettingsProps {
  settings: AudioSettings;
  onChange: (settings: AudioSettings) => void;
}

export function AudioSettings({ settings, onChange }: AudioSettingsProps) {
  const { t } = useTranslation();
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([]);
  const [devicesLoaded, setDevicesLoaded] = useState(false);

  // Fetch audio devices on component mount
  useEffect(() => {
    const fetchAudioDevices = async () => {
      try {
        const devices = await invoke<AudioDevice[]>("list_audio_devices");
        setAudioDevices(devices);
      } catch (error) {
        logger.error("[AudioSettings] Failed to fetch audio devices:", error);
        setAudioDevices([]);
      } finally {
        setDevicesLoaded(true);
      }
    };

    fetchAudioDevices();
  }, []);

  const updateSetting = <K extends keyof AudioSettings>(
    key: K,
    value: AudioSettings[K]
  ) => {
    onChange({ ...settings, [key]: value });
  };

  // Filter devices by type
  const microphoneDevices = audioDevices.filter(device => device.device_type === "Microphone");
  const systemAudioDevices = audioDevices.filter(device => device.device_type === "SystemAudio");

  const getSampleRateLabel = (rate: SampleRate): string => {
    const labels: Record<SampleRate, string> = {
      hz44100: t('settings.recordingConfig.audioSettings.audioQuality.sampleRateLabels.hz44100'),
      hz48000: t('settings.recordingConfig.audioSettings.audioQuality.sampleRateLabels.hz48000'),
    };
    return labels[rate];
  };

  const getBitrateLabel = (bitrate: AudioBitrate): string => {
    const labels: Record<AudioBitrate, string> = {
      kbps128: t('settings.recordingConfig.audioSettings.audioQuality.bitrateLabels.kbps128'),
      kbps192: t('settings.recordingConfig.audioSettings.audioQuality.bitrateLabels.kbps192'),
      kbps256: t('settings.recordingConfig.audioSettings.audioQuality.bitrateLabels.kbps256'),
      kbps320: t('settings.recordingConfig.audioSettings.audioQuality.bitrateLabels.kbps320'),
    };
    return labels[bitrate];
  };

  const hasNoDevices = devicesLoaded && audioDevices.length === 0;

  return (
    <div data-testid="audio-settings" className="space-y-6">
      {/* No audio devices warning */}
      {hasNoDevices && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            {t('errors.audioNoDevicesAvailable')}
          </AlertDescription>
        </Alert>
      )}

      {/* No system audio devices warning when system audio enabled */}
      {devicesLoaded && settings.record_system_audio && systemAudioDevices.length === 0 && !hasNoDevices && (
        <Alert>
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            {t('errors.audioDeviceNotFoundHint')}
          </AlertDescription>
        </Alert>
      )}

      {/* Microphone Settings */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold text-base flex items-center gap-2">
            <Mic className="w-4 h-4" />
            {t('settings.recordingConfig.audioSettings.microphoneRecording.title')}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.audioSettings.microphoneRecording.description')}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <Label htmlFor="record_microphone" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.audioSettings.microphoneRecording.enableMicrophone')}
            </Label>
            <Switch
              id="record_microphone"
              checked={settings.record_microphone}
              onCheckedChange={(checked: boolean) => updateSetting("record_microphone", checked)}
            />
          </div>

          {settings.record_microphone && (
            <>
              <div className="space-y-2">
                <Label>{t('settings.recordingConfig.audioSettings.microphoneRecording.microphoneDevice')}</Label>
                <Select
                  value={settings.microphone_device || "default"}
                  onValueChange={(value) =>
                    updateSetting("microphone_device", value === "default" ? null : value)
                  }
                >
                  <SelectTrigger>
                    <SelectValue placeholder={t('settings.recordingConfig.audioSettings.microphoneRecording.selectDevice')} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="default">{t('settings.recordingConfig.audioSettings.microphoneRecording.defaultDevice')}</SelectItem>
                    {microphoneDevices.map((device, index) => (
                      <SelectItem key={`mic-${index}`} value={device.name}>
                        {device.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <Label>{t('settings.recordingConfig.audioSettings.microphoneRecording.microphoneVolume')}</Label>
                  <Badge variant="secondary">{settings.microphone_volume}%</Badge>
                </div>
                <Slider
                  value={[settings.microphone_volume]}
                  onValueChange={([value]) => updateSetting("microphone_volume", value)}
                  min={0}
                  max={200}
                  step={5}
                  className="w-full"
                />
                <div className="flex justify-between text-xs text-muted-foreground">
                  <span>{t('settings.recordingConfig.audioSettings.microphoneRecording.muted')}</span>
                  <span>100%</span>
                  <span>{t('settings.recordingConfig.audioSettings.microphoneRecording.boost')}</span>
                </div>
              </div>
            </>
          )}
        </div>
      </div>

      {/* System Audio Settings */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold text-base flex items-center gap-2">
            <Volume2 className="w-4 h-4" />
            {t('settings.recordingConfig.audioSettings.systemAudioRecording.title')}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.audioSettings.systemAudioRecording.description')}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <Label htmlFor="record_system_audio" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.audioSettings.systemAudioRecording.enableSystemAudio')}
            </Label>
            <Switch
              id="record_system_audio"
              checked={settings.record_system_audio}
              onCheckedChange={(checked: boolean) => updateSetting("record_system_audio", checked)}
            />
          </div>

          {settings.record_system_audio && (
            <>
              <div className="space-y-2">
                <Label>{t('settings.recordingConfig.audioSettings.systemAudioRecording.systemAudioDevice')}</Label>
                <Select
                  value={settings.system_audio_device || "default"}
                  onValueChange={(value) =>
                    updateSetting("system_audio_device", value === "default" ? null : value)
                  }
                >
                  <SelectTrigger>
                    <SelectValue placeholder={t('settings.recordingConfig.audioSettings.systemAudioRecording.selectDevice')} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="default">{t('settings.recordingConfig.audioSettings.systemAudioRecording.defaultDevice')}</SelectItem>
                    {systemAudioDevices.map((device, index) => (
                      <SelectItem key={`sys-${index}`} value={device.name}>
                        {device.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <Label>{t('settings.recordingConfig.audioSettings.systemAudioRecording.systemAudioVolume')}</Label>
                  <Badge variant="secondary">{settings.system_audio_volume}%</Badge>
                </div>
                <Slider
                  value={[settings.system_audio_volume]}
                  onValueChange={([value]) => updateSetting("system_audio_volume", value)}
                  min={0}
                  max={200}
                  step={5}
                  className="w-full"
                />
                <div className="flex justify-between text-xs text-muted-foreground">
                  <span>{t('settings.recordingConfig.audioSettings.systemAudioRecording.muted')}</span>
                  <span>100%</span>
                  <span>{t('settings.recordingConfig.audioSettings.systemAudioRecording.boost')}</span>
                </div>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Audio Quality Settings */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.audioSettings.audioQuality.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.audioSettings.audioQuality.description')}
          </p>
        </div>
        <div className="space-y-4">
          <div className="space-y-2">
            <Label>{t('settings.recordingConfig.audioSettings.audioQuality.sampleRate')}</Label>
            <div className="flex items-center gap-4">
              <div className="flex-1">
                <Select
                  value={settings.sample_rate}
                  onValueChange={(value) => updateSetting("sample_rate", value as SampleRate)}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="hz44100">{getSampleRateLabel("hz44100")}</SelectItem>
                    <SelectItem value="hz48000">{getSampleRateLabel("hz48000")}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              {settings.sample_rate === "hz48000" && (
                <Badge variant="secondary">{t('settings.recordingConfig.audioSettings.audioQuality.recommended')}</Badge>
              )}
            </div>
          </div>

          <div className="space-y-2">
            <Label>{t('settings.recordingConfig.audioSettings.audioQuality.audioBitrate')}</Label>
            <div className="flex items-center gap-4">
              <div className="flex-1">
                <Select
                  value={settings.bitrate}
                  onValueChange={(value) => updateSetting("bitrate", value as AudioBitrate)}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="kbps128">{getBitrateLabel("kbps128")}</SelectItem>
                    <SelectItem value="kbps192">{getBitrateLabel("kbps192")}</SelectItem>
                    <SelectItem value="kbps256">{getBitrateLabel("kbps256")}</SelectItem>
                    <SelectItem value="kbps320">{getBitrateLabel("kbps320")}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              {settings.bitrate === "kbps192" && (
                <Badge variant="secondary">{t('settings.recordingConfig.audioSettings.audioQuality.recommended')}</Badge>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Info Card */}
      {(settings.record_microphone || settings.record_system_audio) && (
        <div className="gaming-panel p-6">
          <div>
            <div className="space-y-2 text-sm">
              <p className="font-semibold">{t('settings.recordingConfig.audioSettings.currentConfiguration.title')}</p>
              {settings.record_microphone && (
                <>
                  <p className="text-muted-foreground">
                    • {t('settings.recordingConfig.audioSettings.currentConfiguration.microphone')}: {t('settings.recordingConfig.audioSettings.currentConfiguration.enabled')} ({settings.microphone_volume}% {t('settings.recordingConfig.audioSettings.currentConfiguration.volume')})
                  </p>
                  <p className="text-muted-foreground">
                    • {t('settings.recordingConfig.audioSettings.currentConfiguration.device')}: {settings.microphone_device || t('settings.recordingConfig.audioSettings.currentConfiguration.default')}
                  </p>
                </>
              )}
              {settings.record_system_audio && (
                <>
                  <p className="text-muted-foreground">
                    • {t('settings.recordingConfig.audioSettings.currentConfiguration.systemAudio')}: {t('settings.recordingConfig.audioSettings.currentConfiguration.enabled')} ({settings.system_audio_volume}% {t('settings.recordingConfig.audioSettings.currentConfiguration.volume')})
                  </p>
                  <p className="text-muted-foreground">
                    • {t('settings.recordingConfig.audioSettings.currentConfiguration.device')}: {settings.system_audio_device || t('settings.recordingConfig.audioSettings.currentConfiguration.default')}
                  </p>
                </>
              )}
              <p className="text-muted-foreground">
                • {t('settings.recordingConfig.audioSettings.currentConfiguration.quality')}: {getSampleRateLabel(settings.sample_rate)} @ {getBitrateLabel(settings.bitrate)}
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}