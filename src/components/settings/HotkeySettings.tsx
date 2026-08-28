import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Keyboard, RotateCcw } from "lucide-react";

interface HotkeySettings {
  manual_save_clip: string;
  toggle_recording: string;
  delete_last_clip: string;
}

interface HotkeySettingsProps {
  settings: HotkeySettings;
  onChange: (settings: HotkeySettings) => void;
}

export function HotkeySettings({ settings, onChange }: HotkeySettingsProps) {
  const { t } = useTranslation();
  const [recording, setRecording] = useState<keyof HotkeySettings | null>(null);

  const updateHotkey = (key: keyof HotkeySettings, value: string) => {
    onChange({ ...settings, [key]: value });
  };

  const resetToDefaults = () => {
    onChange({
      manual_save_clip: "F8",
      toggle_recording: "F9",
      delete_last_clip: "F10",
    });
  };

  const startRecording = (key: keyof HotkeySettings) => {
    setRecording(key);
  };

  const handleKeyDown = (
    event: React.KeyboardEvent,
    key: keyof HotkeySettings,
  ) => {
    if (!recording || recording !== key) return;

    event.preventDefault();
    event.stopPropagation();

    let hotkey = "";

    // Build hotkey string
    if (event.ctrlKey) hotkey += "Ctrl+";
    if (event.altKey) hotkey += "Alt+";
    if (event.shiftKey) hotkey += "Shift+";

    // Add main key
    if (
      event.key === "Control" ||
      event.key === "Alt" ||
      event.key === "Shift"
    ) {
      return; // Don't capture modifier keys alone
    }

    if (event.key.length === 1) {
      hotkey += event.key.toUpperCase();
    } else {
      hotkey += event.key;
    }

    updateHotkey(key, hotkey);
    setRecording(null);
  };

  const getHotkeyDisplay = (key: keyof HotkeySettings): string => {
    if (recording === key) {
      return t("settings.recordingConfig.hotkeys.pressAnyKey");
    }
    return settings[key];
  };

  return (
    <div className="space-y-6">
      {/* Info Card */}
      <div className="gaming-panel p-6">
        <div>
          <div className="flex items-start gap-3">
            <Keyboard className="w-5 h-5 text-muted-foreground mt-0.5" />
            <div className="space-y-1 text-sm">
              <p className="font-semibold">
                {t("settings.recordingConfig.hotkeys.globalHotkeys.title")}
              </p>
              <p className="text-muted-foreground">
                {t(
                  "settings.recordingConfig.hotkeys.globalHotkeys.description",
                )}
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Manual Save Clip */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("settings.recordingConfig.hotkeys.manualSaveClip.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("settings.recordingConfig.hotkeys.manualSaveClip.description")}
          </p>
        </div>
        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <div className="flex-1">
              <Input
                value={getHotkeyDisplay("manual_save_clip")}
                onFocus={() => startRecording("manual_save_clip")}
                onBlur={() => setRecording(null)}
                onKeyDown={(e: React.KeyboardEvent<HTMLInputElement>) =>
                  handleKeyDown(e, "manual_save_clip")
                }
                readOnly
                className={
                  recording === "manual_save_clip" ? "border-primary" : ""
                }
                placeholder={t("settings.recordingConfig.hotkeys.clickToSet")}
              />
            </div>
            {settings.manual_save_clip !== "F8" && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => updateHotkey("manual_save_clip", "F8")}
              >
                <RotateCcw className="w-4 h-4" />
              </Button>
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            {t("settings.recordingConfig.hotkeys.manualSaveClip.defaultF8")}
          </p>
        </div>
      </div>

      {/* Toggle Recording */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("settings.recordingConfig.hotkeys.toggleRecording.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("settings.recordingConfig.hotkeys.toggleRecording.description")}
          </p>
        </div>
        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <div className="flex-1">
              <Input
                value={getHotkeyDisplay("toggle_recording")}
                onFocus={() => startRecording("toggle_recording")}
                onBlur={() => setRecording(null)}
                onKeyDown={(e: React.KeyboardEvent<HTMLInputElement>) =>
                  handleKeyDown(e, "toggle_recording")
                }
                readOnly
                className={
                  recording === "toggle_recording" ? "border-primary" : ""
                }
                placeholder={t("settings.recordingConfig.hotkeys.clickToSet")}
              />
            </div>
            {settings.toggle_recording !== "F9" && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => updateHotkey("toggle_recording", "F9")}
              >
                <RotateCcw className="w-4 h-4" />
              </Button>
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            {t("settings.recordingConfig.hotkeys.toggleRecording.defaultF9")}
          </p>
        </div>
      </div>

      {/* Delete Last Clip */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("settings.recordingConfig.hotkeys.deleteLastClip.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("settings.recordingConfig.hotkeys.deleteLastClip.description")}
          </p>
        </div>
        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <div className="flex-1">
              <Input
                value={getHotkeyDisplay("delete_last_clip")}
                onFocus={() => startRecording("delete_last_clip")}
                onBlur={() => setRecording(null)}
                onKeyDown={(e: React.KeyboardEvent<HTMLInputElement>) =>
                  handleKeyDown(e, "delete_last_clip")
                }
                readOnly
                className={
                  recording === "delete_last_clip" ? "border-primary" : ""
                }
                placeholder={t("settings.recordingConfig.hotkeys.clickToSet")}
              />
            </div>
            {settings.delete_last_clip !== "F10" && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => updateHotkey("delete_last_clip", "F10")}
              >
                <RotateCcw className="w-4 h-4" />
              </Button>
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            {t("settings.recordingConfig.hotkeys.deleteLastClip.defaultF10")}
          </p>
        </div>
      </div>

      {/* Reset All */}
      <div className="pt-4">
        <Button variant="outline" onClick={resetToDefaults}>
          <RotateCcw className="w-4 h-4 mr-2" />
          {t("settings.recordingConfig.hotkeys.resetAllHotkeys")}
        </Button>
      </div>
    </div>
  );
}
