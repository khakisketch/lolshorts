import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";

interface GameModeSettings {
  record_ranked_solo: boolean;
  record_ranked_flex: boolean;
  record_normal: boolean;
  record_quick_play: boolean;
  record_aram: boolean;
  record_arena: boolean;
  record_special: boolean;
  record_custom: boolean;
  record_practice: boolean;
}

interface GameModeSettingsProps {
  settings: GameModeSettings;
  onChange: (settings: GameModeSettings) => void;
}

export function GameModeSettings({
  settings,
  onChange,
}: GameModeSettingsProps) {
  const { t } = useTranslation();
  const updateSetting = (key: keyof GameModeSettings, value: boolean) => {
    onChange({ ...settings, [key]: value });
  };

  const applyPreset = (preset: "competitive" | "all" | "ranked-only") => {
    switch (preset) {
      case "competitive":
        onChange({
          record_ranked_solo: true,
          record_ranked_flex: true,
          record_normal: true,
          record_quick_play: true,
          record_aram: true,
          record_arena: true,
          record_special: false,
          record_custom: false,
          record_practice: false,
        });
        break;
      case "all":
        onChange({
          record_ranked_solo: true,
          record_ranked_flex: true,
          record_normal: true,
          record_quick_play: true,
          record_aram: true,
          record_arena: true,
          record_special: true,
          record_custom: true,
          record_practice: true,
        });
        break;
      case "ranked-only":
        onChange({
          record_ranked_solo: true,
          record_ranked_flex: true,
          record_normal: false,
          record_quick_play: false,
          record_aram: false,
          record_arena: false,
          record_special: false,
          record_custom: false,
          record_practice: false,
        });
        break;
    }
  };

  return (
    <div className="space-y-6">
      {/* Presets */}
      <div>
        <h3 className="text-sm font-semibold mb-3">
          {t("settings.recordingConfig.gameModes.quickPresets")}
        </h3>
        <div className="flex gap-2 flex-wrap">
          <Button
            variant="outline"
            size="sm"
            onClick={() => applyPreset("competitive")}
          >
            {t("settings.recordingConfig.gameModes.competitiveModes")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => applyPreset("ranked-only")}
          >
            {t("settings.recordingConfig.gameModes.rankedOnly")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => applyPreset("all")}
          >
            {t("settings.recordingConfig.gameModes.allModes")}
          </Button>
        </div>
      </div>

      {/* Ranked Modes */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("settings.recordingConfig.gameModes.rankedModes.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("settings.recordingConfig.gameModes.rankedModes.description")}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="record_ranked_solo" className="cursor-pointer">
                {t(
                  "settings.recordingConfig.gameModes.rankedModes.rankedSoloDuo",
                )}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t(
                  "settings.recordingConfig.gameModes.rankedModes.rankedSoloDuoDesc",
                )}
              </p>
            </div>
            <Switch
              id="record_ranked_solo"
              checked={settings.record_ranked_solo}
              onCheckedChange={(checked: boolean) =>
                updateSetting("record_ranked_solo", checked)
              }
            />
          </div>

          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="record_ranked_flex" className="cursor-pointer">
                {t("settings.recordingConfig.gameModes.rankedModes.rankedFlex")}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t(
                  "settings.recordingConfig.gameModes.rankedModes.rankedFlexDesc",
                )}
              </p>
            </div>
            <Switch
              id="record_ranked_flex"
              checked={settings.record_ranked_flex}
              onCheckedChange={(checked: boolean) =>
                updateSetting("record_ranked_flex", checked)
              }
            />
          </div>
        </div>
      </div>

      {/* Normal Modes */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("settings.recordingConfig.gameModes.normalModes.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("settings.recordingConfig.gameModes.normalModes.description")}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="record_normal" className="cursor-pointer">
                {t(
                  "settings.recordingConfig.gameModes.normalModes.normalDraftBlind",
                )}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t(
                  "settings.recordingConfig.gameModes.normalModes.normalDraftBlindDesc",
                )}
              </p>
            </div>
            <Switch
              id="record_normal"
              checked={settings.record_normal}
              onCheckedChange={(checked: boolean) =>
                updateSetting("record_normal", checked)
              }
            />
          </div>

          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="record_quick_play" className="cursor-pointer">
                {t("settings.recordingConfig.gameModes.normalModes.quickPlay")}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t(
                  "settings.recordingConfig.gameModes.normalModes.quickPlayDesc",
                )}
              </p>
            </div>
            <Switch
              id="record_quick_play"
              checked={settings.record_quick_play}
              onCheckedChange={(checked: boolean) =>
                updateSetting("record_quick_play", checked)
              }
            />
          </div>
        </div>
      </div>

      {/* Alternative Modes */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("settings.recordingConfig.gameModes.alternativeModes.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t(
              "settings.recordingConfig.gameModes.alternativeModes.description",
            )}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="record_aram" className="cursor-pointer">
                {t("settings.recordingConfig.gameModes.alternativeModes.aram")}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t(
                  "settings.recordingConfig.gameModes.alternativeModes.aramDesc",
                )}
              </p>
            </div>
            <Switch
              id="record_aram"
              checked={settings.record_aram}
              onCheckedChange={(checked: boolean) =>
                updateSetting("record_aram", checked)
              }
            />
          </div>

          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="record_arena" className="cursor-pointer">
                {t("settings.recordingConfig.gameModes.alternativeModes.arena")}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t(
                  "settings.recordingConfig.gameModes.alternativeModes.arenaDesc",
                )}
              </p>
            </div>
            <Switch
              id="record_arena"
              checked={settings.record_arena}
              onCheckedChange={(checked: boolean) =>
                updateSetting("record_arena", checked)
              }
            />
          </div>

          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="record_special" className="cursor-pointer">
                {t(
                  "settings.recordingConfig.gameModes.alternativeModes.specialEvents",
                )}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t(
                  "settings.recordingConfig.gameModes.alternativeModes.specialEventsDesc",
                )}
              </p>
            </div>
            <Switch
              id="record_special"
              checked={settings.record_special}
              onCheckedChange={(checked: boolean) =>
                updateSetting("record_special", checked)
              }
            />
          </div>
        </div>
      </div>

      {/* Practice Modes */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("settings.recordingConfig.gameModes.practiceModes.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("settings.recordingConfig.gameModes.practiceModes.description")}
          </p>
        </div>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="record_custom" className="cursor-pointer">
                {t(
                  "settings.recordingConfig.gameModes.practiceModes.customGames",
                )}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t(
                  "settings.recordingConfig.gameModes.practiceModes.customGamesDesc",
                )}
              </p>
            </div>
            <Switch
              id="record_custom"
              checked={settings.record_custom}
              onCheckedChange={(checked: boolean) =>
                updateSetting("record_custom", checked)
              }
            />
          </div>

          <div className="flex items-center justify-between">
            <div className="flex-1">
              <Label htmlFor="record_practice" className="cursor-pointer">
                {t(
                  "settings.recordingConfig.gameModes.practiceModes.practiceTool",
                )}
              </Label>
              <p className="text-xs text-muted-foreground mt-1">
                {t(
                  "settings.recordingConfig.gameModes.practiceModes.practiceToolDesc",
                )}
              </p>
            </div>
            <Switch
              id="record_practice"
              checked={settings.record_practice}
              onCheckedChange={(checked: boolean) =>
                updateSetting("record_practice", checked)
              }
            />
          </div>
        </div>
      </div>
    </div>
  );
}
