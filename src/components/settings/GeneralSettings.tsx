import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { StorageSettings } from "@/types";

interface GeneralSettingsProps {
  settings: {
    auto_start_with_league: boolean;
    minimize_to_tray: boolean;
    show_notifications: boolean;
    show_replay_popup: boolean;
    crash_reporting_enabled: boolean;
    overlay_enabled: boolean;
    storage: StorageSettings;
  };
  onChange: (settings: GeneralSettingsProps['settings']) => void;
}

export function GeneralSettings({ settings, onChange }: GeneralSettingsProps) {
  const { t } = useTranslation();

  const handleChange = (key: string, value: boolean) => {
    onChange({ ...settings, [key]: value });
  };

  const handleStorageChange = (key: keyof StorageSettings, value: boolean | number) => {
    onChange({ ...settings, storage: { ...settings.storage, [key]: value } });
  };

  return (
    <div className="gaming-panel p-6">
      <div className="space-y-6">
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="auto-start">{t('generalSettings.autoStart.label')}</Label>
            <p className="text-sm text-muted-foreground">
              {t('generalSettings.autoStart.description')}
            </p>
          </div>
          <Switch
            id="auto-start"
            checked={settings.auto_start_with_league}
            onCheckedChange={(checked) => handleChange("auto_start_with_league", checked)}
          />
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="minimize-tray">{t('generalSettings.minimizeToTray.label')}</Label>
            <p className="text-sm text-muted-foreground">
              {t('generalSettings.minimizeToTray.description')}
            </p>
          </div>
          <Switch
            id="minimize-tray"
            checked={settings.minimize_to_tray}
            onCheckedChange={(checked) => handleChange("minimize_to_tray", checked)}
          />
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="notifications">{t('generalSettings.notifications.label')}</Label>
            <p className="text-sm text-muted-foreground">
              {t('generalSettings.notifications.description')}
            </p>
          </div>
          <Switch
            id="notifications"
            checked={settings.show_notifications}
            onCheckedChange={(checked) => handleChange("show_notifications", checked)}
          />
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="replay-popup">{t('generalSettings.replayPopup.label')}</Label>
            <p className="text-sm text-muted-foreground">
              {t('generalSettings.replayPopup.description')}
            </p>
          </div>
          <Switch
            id="replay-popup"
            checked={settings.show_replay_popup}
            onCheckedChange={(checked) => handleChange("show_replay_popup", checked)}
          />
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="crash-reporting">{t('generalSettings.crashReporting.label')}</Label>
            <p className="text-sm text-muted-foreground">
              {t('generalSettings.crashReporting.description')}
            </p>
          </div>
          <Switch
            id="crash-reporting"
            checked={settings.crash_reporting_enabled}
            onCheckedChange={(checked) => handleChange("crash_reporting_enabled", checked)}
          />
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="overlay-enabled">{t('generalSettings.overlay.label', 'In-Game Overlay')}</Label>
            <p className="text-sm text-muted-foreground">
              {t('generalSettings.overlay.description', 'Show recording indicator and event feed during gameplay')}
            </p>
          </div>
          <Switch
            id="overlay-enabled"
            checked={settings.overlay_enabled}
            onCheckedChange={(checked) => handleChange("overlay_enabled", checked)}
          />
        </div>

        <div className="border-t border-border pt-6">
          <h4 className="text-sm font-semibold mb-4">{t('generalSettings.storage.title')}</h4>
          <div className="space-y-6">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label htmlFor="auto-delete">{t('generalSettings.storage.autoDelete.label')}</Label>
                <p className="text-sm text-muted-foreground">
                  {t('generalSettings.storage.autoDelete.description')}
                </p>
              </div>
              <Switch
                id="auto-delete"
                checked={settings.storage.auto_delete_enabled}
                onCheckedChange={(checked) => handleStorageChange("auto_delete_enabled", checked)}
              />
            </div>

            {settings.storage.auto_delete_enabled && (
              <>
                <div className="flex items-center justify-between gap-4">
                  <div className="space-y-0.5">
                    <Label htmlFor="auto-delete-days">{t('generalSettings.storage.autoDeleteDays.label')}</Label>
                    <p className="text-sm text-muted-foreground">
                      {t('generalSettings.storage.autoDeleteDays.description')}
                    </p>
                  </div>
                  <Input
                    id="auto-delete-days"
                    type="number"
                    min={1}
                    max={365}
                    value={settings.storage.auto_delete_days}
                    onChange={(e) => handleStorageChange("auto_delete_days", Math.max(1, parseInt(e.target.value, 10) || 1))}
                    className="w-24 text-right"
                  />
                </div>

                <div className="flex items-center justify-between gap-4">
                  <div className="space-y-0.5">
                    <Label htmlFor="max-storage-gb">{t('generalSettings.storage.maxStorageGb.label')}</Label>
                    <p className="text-sm text-muted-foreground">
                      {t('generalSettings.storage.maxStorageGb.description')}
                    </p>
                  </div>
                  <Input
                    id="max-storage-gb"
                    type="number"
                    min={1}
                    max={2000}
                    value={settings.storage.max_storage_gb}
                    onChange={(e) => handleStorageChange("max_storage_gb", Math.max(1, parseInt(e.target.value, 10) || 1))}
                    className="w-24 text-right"
                  />
                </div>

                <div className="flex items-center justify-between">
                  <div className="space-y-0.5">
                    <Label htmlFor="delete-exported">{t('generalSettings.storage.deleteExported.label')}</Label>
                    <p className="text-sm text-muted-foreground">
                      {t('generalSettings.storage.deleteExported.description')}
                    </p>
                  </div>
                  <Switch
                    id="delete-exported"
                    checked={settings.storage.delete_exported_clips}
                    onCheckedChange={(checked) => handleStorageChange("delete_exported_clips", checked)}
                  />
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
