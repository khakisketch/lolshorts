import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { BackgroundMusic, AudioLevels } from "@/types/autoEdit";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Separator } from "@/components/ui/separator";
import { Music, Upload, X, Volume2, AlertCircle } from "lucide-react";
import { logger } from "@/lib/logger";
import { utilsApi } from "@/api/utils";

interface AudioMixerProps {
  backgroundMusic: BackgroundMusic | null;
  audioLevels: AudioLevels;
  onBackgroundMusicChange: (music: BackgroundMusic | null) => void;
  onAudioLevelsChange: (levels: Partial<AudioLevels>) => void;
}

export function AudioMixer({
  backgroundMusic,
  audioLevels,
  onBackgroundMusicChange,
  onAudioLevelsChange,
}: AudioMixerProps) {
  const { t } = useTranslation();

  const handleSelectMusic = useCallback(async () => {
    try {
      const staged = await utilsApi.selectAndStageExternalMedia("audio");
      if (staged) {
        onBackgroundMusicChange({
          file_path: staged.path,
          loop_music: backgroundMusic?.loop_music ?? true,
        });
      }
    } catch (error) {
      logger.error("Failed to select music file:", error);
      alert(t("errors.fileSelectionFailed"));
    }
  }, [backgroundMusic, onBackgroundMusicChange, t]);

  const handleRemoveMusic = useCallback(() => {
    onBackgroundMusicChange(null);
  }, [onBackgroundMusicChange]);

  const handleLoopToggle = useCallback(
    (checked: boolean) => {
      if (backgroundMusic) {
        onBackgroundMusicChange({
          ...backgroundMusic,
          loop_music: checked,
        });
      }
    },
    [backgroundMusic, onBackgroundMusicChange],
  );

  return (
    <div className="gaming-panel p-6 h-full" data-testid="audio-mixer">
      <div className="mb-4">
        <h3 className="text-lg font-semibold flex items-center gap-2">
          <Music className="w-5 h-5" />
          {t("editor.audioMixer.title")}
        </h3>
      </div>
      <div className="space-y-6">
        {/* Background Music Upload */}
        <div className="space-y-3">
          <Label>{t("editor.audioMixer.backgroundMusic")}</Label>

          {!backgroundMusic ? (
            <div className="border-2 border-dashed rounded-lg p-6 text-center">
              <Music className="w-12 h-12 mx-auto mb-3 text-muted-foreground" />
              <p className="text-sm text-muted-foreground mb-3">
                {t("editor.audioMixer.addMusicDesc")}
              </p>
              <Button
                onClick={handleSelectMusic}
                variant="outline"
                data-testid="upload-music-button"
              >
                <Upload className="w-4 h-4 mr-2" />
                {t("editor.audioMixer.uploadMusic")}
              </Button>
            </div>
          ) : (
            <div className="bg-black/40 rounded-lg border border-white/5 p-4">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <Music className="w-4 h-4" />
                  <span className="text-sm font-medium truncate">
                    {backgroundMusic.file_path.split("/").pop() ||
                      "Background Music"}
                  </span>
                </div>
                <Button size="icon" variant="ghost" onClick={handleRemoveMusic}>
                  <X className="w-4 h-4" />
                </Button>
              </div>

              <div className="flex items-center justify-between">
                <Label htmlFor="loop-music" className="text-sm">
                  {t("editor.audioMixer.loopMusic")}
                </Label>
                <Switch
                  id="loop-music"
                  checked={backgroundMusic.loop_music}
                  onCheckedChange={handleLoopToggle}
                  data-testid="loop-music-toggle"
                />
              </div>

              {!backgroundMusic.loop_music && (
                <Alert className="mt-3">
                  <AlertCircle className="h-4 w-4" />
                  <AlertDescription className="text-xs">
                    {t("editor.audioMixer.noLoopWarning")}
                  </AlertDescription>
                </Alert>
              )}
            </div>
          )}
        </div>

        <Separator />

        {/* Volume Controls */}
        <div className="space-y-4">
          <div>
            <Label className="flex items-center justify-between mb-2">
              <span className="flex items-center gap-2">
                <Volume2 className="w-4 h-4" />
                {t("editor.audioMixer.gameAudio")}
              </span>
              <span
                className="text-sm font-mono text-muted-foreground"
                data-testid="game-audio-value"
              >
                {audioLevels.game_audio}%
              </span>
            </Label>
            <Slider
              value={[audioLevels.game_audio]}
              onValueChange={([value]) =>
                onAudioLevelsChange({ game_audio: value })
              }
              min={0}
              max={100}
              step={1}
              className="w-full"
              data-testid="game-audio-slider"
            />
            <div className="flex justify-between text-xs text-muted-foreground mt-1">
              <span>{t("editor.audioMixer.silent")}</span>
              <span>{t("editor.audioMixer.full")}</span>
            </div>
          </div>

          <div>
            <Label className="flex items-center justify-between mb-2">
              <span className="flex items-center gap-2">
                <Music className="w-4 h-4" />
                {t("editor.audioMixer.backgroundMusic")}
              </span>
              <span
                className="text-sm font-mono text-muted-foreground"
                data-testid="music-audio-value"
              >
                {audioLevels.background_music}%
              </span>
            </Label>
            <Slider
              value={[audioLevels.background_music]}
              onValueChange={([value]) =>
                onAudioLevelsChange({ background_music: value })
              }
              min={0}
              max={100}
              step={1}
              className="w-full"
              disabled={!backgroundMusic}
              data-testid="music-audio-slider"
            />
            <div className="flex justify-between text-xs text-muted-foreground mt-1">
              <span>{t("editor.audioMixer.silent")}</span>
              <span>{t("editor.audioMixer.full")}</span>
            </div>
          </div>
        </div>

        {/* Audio Balance Info */}
        <Alert data-testid="audio-tip">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="text-xs">
            {t("editor.audioMixer.mixTip")}
          </AlertDescription>
        </Alert>

        {/* Recommended Presets */}
        <div className="space-y-2">
          <Label className="text-xs">
            {t("editor.audioMixer.quickPresets")}
          </Label>
          <div className="grid grid-cols-2 gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                onAudioLevelsChange({ game_audio: 100, background_music: 0 })
              }
              data-testid="preset-game-only"
            >
              {t("editor.audioMixer.presetGameOnly")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                onAudioLevelsChange({ game_audio: 70, background_music: 30 })
              }
              data-testid="preset-balanced"
            >
              {t("editor.audioMixer.presetBalanced")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                onAudioLevelsChange({ game_audio: 40, background_music: 60 })
              }
              data-testid="preset-music-focus"
            >
              {t("editor.audioMixer.presetMusicFocus")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                onAudioLevelsChange({ game_audio: 0, background_music: 100 })
              }
              data-testid="preset-music-only"
            >
              {t("editor.audioMixer.presetMusicOnly")}
            </Button>
          </div>
        </div>

        {/* Final mix visualization */}
        <div className="space-y-2">
          <Label className="text-xs">{t("editor.audioMixer.mixPreview")}</Label>
          <div
            className="flex h-8 rounded overflow-hidden border"
            data-testid="mix-preview"
          >
            <div
              className="bg-blue-500 flex items-center justify-center text-xs font-medium text-white"
              style={{ width: `${audioLevels.game_audio}%` }}
              data-testid="mix-preview-game"
            >
              {audioLevels.game_audio > 15 && t("editor.audioMixer.game")}
            </div>
            <div
              className="bg-purple-500 flex items-center justify-center text-xs font-medium text-white"
              style={{ width: `${audioLevels.background_music}%` }}
              data-testid="mix-preview-music"
            >
              {audioLevels.background_music > 15 &&
                t("editor.audioMixer.music")}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
