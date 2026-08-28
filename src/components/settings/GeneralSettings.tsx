import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

interface GeneralSettingsProps {
  settings: {
    minimize_to_tray: boolean;
    show_notifications: boolean;
    show_replay_popup: boolean;
    crash_reporting_enabled: boolean;
    overlay_enabled: boolean;
  };
  onChange: (settings: GeneralSettingsProps["settings"]) => void;
}

export function GeneralSettings({ settings, onChange }: GeneralSettingsProps) {
  const { t } = useTranslation();

  const handleChange = (key: string, value: boolean) => {
    onChange({ ...settings, [key]: value });
  };

  return (
    <div className="gaming-panel p-6">
      <div className="space-y-6">
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="minimize-tray">
              {t("generalSettings.minimizeToTray.label")}
            </Label>
            <p className="text-sm text-muted-foreground">
              {t("generalSettings.minimizeToTray.description")}
            </p>
          </div>
          <Switch
            id="minimize-tray"
            checked={settings.minimize_to_tray}
            onCheckedChange={(checked) =>
              handleChange("minimize_to_tray", checked)
            }
          />
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="notifications">
              {t("generalSettings.notifications.label")}
            </Label>
            <p className="text-sm text-muted-foreground">
              {t("generalSettings.notifications.description")}
            </p>
          </div>
          <Switch
            id="notifications"
            checked={settings.show_notifications}
            onCheckedChange={(checked) =>
              handleChange("show_notifications", checked)
            }
          />
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="replay-popup">
              {t("generalSettings.replayPopup.label")}
            </Label>
            <p className="text-sm text-muted-foreground">
              {t("generalSettings.replayPopup.description")}
            </p>
          </div>
          <Switch
            id="replay-popup"
            checked={settings.show_replay_popup}
            onCheckedChange={(checked) =>
              handleChange("show_replay_popup", checked)
            }
          />
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="crash-reporting">
              {t("generalSettings.crashReporting.label")}
            </Label>
            <p className="text-sm text-muted-foreground">
              {t("generalSettings.crashReporting.description")}
            </p>
          </div>
          <Switch
            id="crash-reporting"
            checked={settings.crash_reporting_enabled}
            onCheckedChange={(checked) =>
              handleChange("crash_reporting_enabled", checked)
            }
          />
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="overlay-enabled">
              {t("generalSettings.overlay.label", "In-Game Overlay")}
            </Label>
            <p className="text-sm text-muted-foreground">
              {t(
                "generalSettings.overlay.description",
                "Show recording status and clip save results during gameplay",
              )}
            </p>
          </div>
          <Switch
            id="overlay-enabled"
            checked={settings.overlay_enabled}
            onCheckedChange={(checked) =>
              handleChange("overlay_enabled", checked)
            }
          />
        </div>
      </div>
    </div>
  );
}
