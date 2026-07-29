import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  applyHighlightPreset,
  SELECTABLE_HIGHLIGHT_PRESETS,
  type SelectableHighlightPreset,
} from "./highlightPreset";

interface EventFilterSettings {
  record_kills: boolean;
  record_multikills: boolean;
  record_first_blood: boolean;
  record_deaths: boolean;
  record_shutdown: boolean;
  record_assists: boolean;
  record_dragon: boolean;
  record_baron: boolean;
  record_elder: boolean;
  record_herald: boolean;
  record_turret: boolean;
  record_inhibitor: boolean;
  record_nexus: boolean;
  record_ace: boolean;
  record_game_end: boolean;
  record_steal: boolean;
  min_priority: number;
}

interface EventFilterSettingsProps {
  settings: EventFilterSettings;
  onChange: (settings: EventFilterSettings) => void;
}

export function EventFilterSettings({ settings, onChange }: EventFilterSettingsProps) {
  const { t } = useTranslation();
  const updateSetting = (key: keyof EventFilterSettings, value: boolean | number) => {
    onChange({ ...settings, [key]: value });
  };

  // 빠른 프리셋은 기본 화면의 "어떤 장면을 담을까"와 같은 조합을 쓴다.
  // 여기서만 다른 조합을 쓰면 고급에서 프리셋을 눌러도 기본 화면은 "직접 설정"이
  // 되어버려, 같은 앱이 두 가지 언어로 말하게 된다.
  const applyPreset = (preset: SelectableHighlightPreset) => {
    onChange(applyHighlightPreset(preset, settings));
  };

  const getPriorityLabel = (priority: number): string => {
    const labels = {
      1: t('settings.recordingConfig.eventFilter.priorityLabels.allEvents'),
      2: t('settings.recordingConfig.eventFilter.priorityLabels.importantEvents'),
      3: t('settings.recordingConfig.eventFilter.priorityLabels.highPriority'),
      4: t('settings.recordingConfig.eventFilter.priorityLabels.criticalMoments'),
      5: t('settings.recordingConfig.eventFilter.priorityLabels.epicPlaysOnly'),
    };
    return labels[priority as keyof typeof labels] || t('settings.recordingConfig.eventFilter.priorityLabels.custom');
  };

  return (
    <div className="space-y-6">
      {/* Presets */}
      <div>
        <h3 className="text-sm font-semibold mb-3">{t('settings.recordingConfig.eventFilter.quickPresets')}</h3>
        <div className="flex gap-2 flex-wrap">
          {SELECTABLE_HIGHLIGHT_PRESETS.map((preset) => (
            <Button
              key={preset}
              variant="outline"
              size="sm"
              onClick={() => applyPreset(preset)}
              data-testid={`preset-${preset}`}
            >
              {t(`settings.basic.highlights.options.${preset}.label`)}
            </Button>
          ))}
        </div>
      </div>

      {/* Priority Filter */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.eventFilter.priorityFilter')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.eventFilter.priorityFilterDescription')}
          </p>
        </div>
        <div className="space-y-4">
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label>{t('settings.recordingConfig.eventFilter.minimumPriority')}</Label>
              <Badge variant="secondary">{getPriorityLabel(settings.min_priority)}</Badge>
            </div>
            <Slider
              value={[settings.min_priority]}
              onValueChange={([value]) => updateSetting("min_priority", value)}
              min={1}
              max={5}
              step={1}
              className="w-full"
              data-testid="priority-filter-slider"
            />
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>{t('settings.recordingConfig.eventFilter.priorityScale.all')}</span>
              <span>{t('settings.recordingConfig.eventFilter.priorityScale.important')}</span>
              <span>{t('settings.recordingConfig.eventFilter.priorityScale.high')}</span>
              <span>{t('settings.recordingConfig.eventFilter.priorityScale.critical')}</span>
              <span>{t('settings.recordingConfig.eventFilter.priorityScale.epic')}</span>
            </div>
          </div>
        </div>
      </div>

      {/* Kill Events */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.eventFilter.killEvents.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.eventFilter.killEvents.description')}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <Label htmlFor="record_kills" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.killEvents.kills')}
            </Label>
            <Switch
              id="record_kills"
              checked={settings.record_kills}
              onCheckedChange={(checked: boolean) => updateSetting("record_kills", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_multikills" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.killEvents.multikills')}
            </Label>
            <Switch
              id="record_multikills"
              checked={settings.record_multikills}
              onCheckedChange={(checked: boolean) => updateSetting("record_multikills", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_first_blood" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.killEvents.firstBlood')}
            </Label>
            <Switch
              id="record_first_blood"
              checked={settings.record_first_blood}
              onCheckedChange={(checked: boolean) => updateSetting("record_first_blood", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_deaths" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.killEvents.deaths')}
            </Label>
            <Switch
              id="record_deaths"
              checked={settings.record_deaths}
              onCheckedChange={(checked: boolean) => updateSetting("record_deaths", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_shutdown" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.killEvents.shutdown')}
            </Label>
            <Switch
              id="record_shutdown"
              checked={settings.record_shutdown}
              onCheckedChange={(checked: boolean) => updateSetting("record_shutdown", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_assists" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.killEvents.assists')}
            </Label>
            <Switch
              id="record_assists"
              checked={settings.record_assists}
              onCheckedChange={(checked: boolean) => updateSetting("record_assists", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_ace" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.killEvents.ace')}
            </Label>
            <Switch
              id="record_ace"
              checked={settings.record_ace}
              onCheckedChange={(checked: boolean) => updateSetting("record_ace", checked)}
            />
          </div>
        </div>
      </div>

      {/* Objective Events */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.eventFilter.objectiveEvents.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.eventFilter.objectiveEvents.description')}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <Label htmlFor="record_dragon" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.objectiveEvents.dragon')}
            </Label>
            <Switch
              id="record_dragon"
              checked={settings.record_dragon}
              onCheckedChange={(checked: boolean) => updateSetting("record_dragon", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_baron" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.objectiveEvents.baronNashor')}
            </Label>
            <Switch
              id="record_baron"
              checked={settings.record_baron}
              onCheckedChange={(checked: boolean) => updateSetting("record_baron", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_elder" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.objectiveEvents.elderDragon')}
            </Label>
            <Switch
              id="record_elder"
              checked={settings.record_elder}
              onCheckedChange={(checked: boolean) => updateSetting("record_elder", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_herald" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.objectiveEvents.riftHerald')}
            </Label>
            <Switch
              id="record_herald"
              checked={settings.record_herald}
              onCheckedChange={(checked: boolean) => updateSetting("record_herald", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_steal" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.objectiveEvents.objectiveSteals')}
            </Label>
            <Switch
              id="record_steal"
              checked={settings.record_steal}
              onCheckedChange={(checked: boolean) => updateSetting("record_steal", checked)}
            />
          </div>
        </div>
      </div>

      {/* Structure Events */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('settings.recordingConfig.eventFilter.structureEvents.title')}</h3>
          <p className="text-sm text-muted-foreground">
            {t('settings.recordingConfig.eventFilter.structureEvents.description')}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <Label htmlFor="record_turret" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.structureEvents.turrets')}
            </Label>
            <Switch
              id="record_turret"
              checked={settings.record_turret}
              onCheckedChange={(checked: boolean) => updateSetting("record_turret", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_inhibitor" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.structureEvents.inhibitors')}
            </Label>
            <Switch
              id="record_inhibitor"
              checked={settings.record_inhibitor}
              onCheckedChange={(checked: boolean) => updateSetting("record_inhibitor", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_nexus" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.structureEvents.nexus')}
            </Label>
            <Switch
              id="record_nexus"
              checked={settings.record_nexus}
              onCheckedChange={(checked: boolean) => updateSetting("record_nexus", checked)}
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="record_game_end" className="flex-1 cursor-pointer">
              {t('settings.recordingConfig.eventFilter.structureEvents.gameEnd')}
            </Label>
            <Switch
              id="record_game_end"
              checked={settings.record_game_end}
              onCheckedChange={(checked: boolean) => updateSetting("record_game_end", checked)}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
