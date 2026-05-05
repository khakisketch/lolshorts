import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import { Badge } from "@/components/ui/badge";
import { Clock, Zap } from "lucide-react";

interface EventTiming {
  pre_duration: number;
  post_duration: number;
}

interface ClipTimingSettings {
  default_pre_duration: number;
  default_post_duration: number;
  event_timings: Record<string, EventTiming>;
  merge_consecutive_events: boolean;
  merge_time_threshold: number;
}

interface ClipTimingSettingsProps {
  settings: ClipTimingSettings;
  onChange: (settings: ClipTimingSettings) => void;
}

export function ClipTimingSettings({ settings, onChange }: ClipTimingSettingsProps) {
  const { t } = useTranslation();
  const updateSetting = <K extends keyof ClipTimingSettings>(
    key: K,
    value: ClipTimingSettings[K]
  ) => {
    onChange({ ...settings, [key]: value });
  };

  const updateEventTiming = (eventType: string, timing: Partial<EventTiming>) => {
    const currentTiming = settings.event_timings[eventType] || {
      pre_duration: settings.default_pre_duration,
      post_duration: settings.default_post_duration,
    };

    onChange({
      ...settings,
      event_timings: {
        ...settings.event_timings,
        [eventType]: { ...currentTiming, ...timing },
      },
    });
  };

  const getEventTiming = (eventType: string): EventTiming => {
    return settings.event_timings[eventType] || {
      pre_duration: settings.default_pre_duration,
      post_duration: settings.default_post_duration,
    };
  };

  return (
    <div className="space-y-6">
      {/* Default Timing */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold text-base flex items-center gap-2">
            <Clock className="w-4 h-4" />
            {t('settings.recordingConfig.clipTiming.defaultDuration.title')}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.clipTiming.defaultDuration.description')}
          </p>
        </div>
        <div className="space-y-6">
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label>{t('settings.recordingConfig.clipTiming.defaultDuration.beforeEvent')}</Label>
              <Badge variant="secondary">{settings.default_pre_duration}s</Badge>
            </div>
            <Slider
              value={[settings.default_pre_duration]}
              onValueChange={([value]) => updateSetting("default_pre_duration", value)}
              min={5}
              max={30}
              step={1}
              className="w-full"
            />
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>5s</span>
              <span>15s</span>
              <span>30s</span>
            </div>
          </div>

          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label>{t('settings.recordingConfig.clipTiming.defaultDuration.afterEvent')}</Label>
              <Badge variant="secondary">{settings.default_post_duration}s</Badge>
            </div>
            <Slider
              value={[settings.default_post_duration]}
              onValueChange={([value]) => updateSetting("default_post_duration", value)}
              min={2}
              max={15}
              step={1}
              className="w-full"
            />
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>2s</span>
              <span>7s</span>
              <span>15s</span>
            </div>
          </div>

          <div className="pt-2 text-sm text-muted-foreground">
            {t('settings.recordingConfig.clipTiming.defaultDuration.totalLength')}: {settings.default_pre_duration + settings.default_post_duration}s
            ({settings.default_pre_duration}s {t('settings.recordingConfig.clipTiming.defaultDuration.before')} + {settings.default_post_duration}s {t('settings.recordingConfig.clipTiming.defaultDuration.after')})
          </div>
        </div>
      </div>

      {/* Event-Specific Timing */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold text-base flex items-center gap-2">
            <Zap className="w-4 h-4" />
            {t('settings.recordingConfig.clipTiming.eventSpecific.title')}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.clipTiming.eventSpecific.description')}
          </p>
        </div>
        <div className="space-y-6">
          {/* Multikill */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div>
                <Label className="text-base">{t('settings.recordingConfig.clipTiming.eventSpecific.multikills.title')}</Label>
                <p className="text-xs text-muted-foreground mt-1">
                  {t('settings.recordingConfig.clipTiming.eventSpecific.multikills.description')}
                </p>
              </div>
            </div>
            <div className="pl-4 space-y-3">
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label className="text-sm">{t('settings.recordingConfig.clipTiming.eventSpecific.beforeEvent')}</Label>
                  <Badge variant="outline">{getEventTiming("multikill").pre_duration}s</Badge>
                </div>
                <Slider
                  value={[getEventTiming("multikill").pre_duration]}
                  onValueChange={([value]) =>
                    updateEventTiming("multikill", { pre_duration: value })
                  }
                  min={5}
                  max={30}
                  step={1}
                />
              </div>
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label className="text-sm">{t('settings.recordingConfig.clipTiming.eventSpecific.afterEvent')}</Label>
                  <Badge variant="outline">{getEventTiming("multikill").post_duration}s</Badge>
                </div>
                <Slider
                  value={[getEventTiming("multikill").post_duration]}
                  onValueChange={([value]) =>
                    updateEventTiming("multikill", { post_duration: value })
                  }
                  min={2}
                  max={15}
                  step={1}
                />
              </div>
            </div>
          </div>

          {/* Steals */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div>
                <Label className="text-base">{t('settings.recordingConfig.clipTiming.eventSpecific.objectiveSteals.title')}</Label>
                <p className="text-xs text-muted-foreground mt-1">
                  {t('settings.recordingConfig.clipTiming.eventSpecific.objectiveSteals.description')}
                </p>
              </div>
            </div>
            <div className="pl-4 space-y-3">
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label className="text-sm">{t('settings.recordingConfig.clipTiming.eventSpecific.beforeEvent')}</Label>
                  <Badge variant="outline">{getEventTiming("steal").pre_duration}s</Badge>
                </div>
                <Slider
                  value={[getEventTiming("steal").pre_duration]}
                  onValueChange={([value]) =>
                    updateEventTiming("steal", { pre_duration: value })
                  }
                  min={5}
                  max={30}
                  step={1}
                />
              </div>
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label className="text-sm">{t('settings.recordingConfig.clipTiming.eventSpecific.afterEvent')}</Label>
                  <Badge variant="outline">{getEventTiming("steal").post_duration}s</Badge>
                </div>
                <Slider
                  value={[getEventTiming("steal").post_duration]}
                  onValueChange={([value]) =>
                    updateEventTiming("steal", { post_duration: value })
                  }
                  min={2}
                  max={15}
                  step={1}
                />
              </div>
            </div>
          </div>

          {/* Regular Kills */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <div>
                <Label className="text-base">{t('settings.recordingConfig.clipTiming.eventSpecific.regularKills.title')}</Label>
                <p className="text-xs text-muted-foreground mt-1">
                  {t('settings.recordingConfig.clipTiming.eventSpecific.regularKills.description')}
                </p>
              </div>
            </div>
            <div className="pl-4 space-y-3">
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label className="text-sm">{t('settings.recordingConfig.clipTiming.eventSpecific.beforeEvent')}</Label>
                  <Badge variant="outline">{getEventTiming("kill").pre_duration}s</Badge>
                </div>
                <Slider
                  value={[getEventTiming("kill").pre_duration]}
                  onValueChange={([value]) =>
                    updateEventTiming("kill", { pre_duration: value })
                  }
                  min={5}
                  max={30}
                  step={1}
                />
              </div>
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label className="text-sm">{t('settings.recordingConfig.clipTiming.eventSpecific.afterEvent')}</Label>
                  <Badge variant="outline">{getEventTiming("kill").post_duration}s</Badge>
                </div>
                <Slider
                  value={[getEventTiming("kill").post_duration]}
                  onValueChange={([value]) =>
                    updateEventTiming("kill", { post_duration: value })
                  }
                  min={2}
                  max={15}
                  step={1}
                />
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Event Merging */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.clipTiming.eventMerging.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.clipTiming.eventMerging.description')}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="merge_consecutive" className="cursor-pointer">
                {t('settings.recordingConfig.clipTiming.eventMerging.mergeConsecutive')}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t('settings.recordingConfig.clipTiming.eventMerging.mergeConsecutiveDesc')}
              </p>
            </div>
            <Switch
              id="merge_consecutive"
              checked={settings.merge_consecutive_events}
              onCheckedChange={(checked: boolean) => updateSetting("merge_consecutive_events", checked)}
            />
          </div>

          {settings.merge_consecutive_events && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label>{t('settings.recordingConfig.clipTiming.eventMerging.mergeTimeWindow')}</Label>
                <Badge variant="secondary">{settings.merge_time_threshold}s</Badge>
              </div>
              <Slider
                value={[settings.merge_time_threshold]}
                onValueChange={([value]) => updateSetting("merge_time_threshold", value)}
                min={5}
                max={30}
                step={1}
                className="w-full"
              />
              <div className="flex justify-between text-xs text-muted-foreground">
                <span>5s</span>
                <span>15s</span>
                <span>30s</span>
              </div>
              <p className="text-xs text-muted-foreground">
                {t('settings.recordingConfig.clipTiming.eventMerging.mergeExplanation', { seconds: settings.merge_time_threshold })}
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
