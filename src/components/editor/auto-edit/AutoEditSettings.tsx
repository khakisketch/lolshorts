import { useTranslation } from "react-i18next";
import { CanvasEditor } from "../canvas/CanvasEditor";
import { AudioMixer } from "../AudioMixer";
import { AutoEditQuotaBadge } from "../AutoEditQuotaBadge";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Video,
  Clock,
  CheckCircle2,
  AlertCircle,
  AlertTriangle,
  Loader2,
  Palette,
  Music,
  Sparkles,
  Info,
  Type,
  AlignLeft,
  Tag,
  ZoomIn,
} from "lucide-react";
import {
  DurationOption,
  GameSelection,
  CanvasTemplate,
  AudioLevels,
  BackgroundMusic,
  AutoEditMetadata,
} from "@/types/autoEdit";
import { formatDuration } from "@/lib/utils";

interface AutoEditSettingsProps {
  availableGames: GameSelection[];
  selectedGameIds: string[];
  /**
   * 다른 화면(홈)에서 이미 골라 넘어온 클립 개수. 0이면 자동 선택.
   *
   * 판정은 `pinnedPathsFor()` 한 곳에서 한다 — 여기 안내와 실제로 보내는 설정이
   * 어긋나면 안 된다.
   */
  pinnedClipCount: number;
  pinnedGameCount: number;
  pinnedDuration: number;
  targetDuration: DurationOption;
  enableEventZoom: boolean;
  enableHookCaptions: boolean;
  currentTemplate: CanvasTemplate | null;
  backgroundMusic: BackgroundMusic | null;
  audioLevels: AudioLevels;
  metadata: AutoEditMetadata;
  isLoading: boolean;
  gamesLoading: boolean;
  isFreeTier: boolean;
  onToggleGame: (gameId: string) => void;
  onUseAutomaticSelection: () => void;
  onSetDuration: (duration: DurationOption) => void;
  onToggleEventZoom: (enabled: boolean) => void;
  onToggleHookCaptions: (enabled: boolean) => void;
  onTemplateChange: (template: CanvasTemplate) => void;
  onBackgroundMusicChange: (music: BackgroundMusic | null) => void;
  onAudioLevelsChange: (levels: Partial<AudioLevels>) => void;
  onMetadataChange: (metadata: Partial<AutoEditMetadata>) => void;
  onGenerate: () => void;
}

export function AutoEditSettings({
  availableGames,
  selectedGameIds,
  pinnedClipCount,
  pinnedGameCount,
  pinnedDuration,
  targetDuration,
  enableEventZoom,
  enableHookCaptions,
  currentTemplate,
  backgroundMusic,
  audioLevels,
  metadata,
  isLoading,
  gamesLoading,
  isFreeTier,
  onToggleGame,
  onUseAutomaticSelection,
  onSetDuration,
  onToggleEventZoom,
  onToggleHookCaptions,
  onTemplateChange,
  onBackgroundMusicChange,
  onAudioLevelsChange,
  onMetadataChange,
  onGenerate,
}: AutoEditSettingsProps) {
  const { t } = useTranslation();

  return (
    <div className="max-w-6xl mx-auto space-y-6">
      {isFreeTier && (
        <Alert className="border-yellow-500/50 bg-yellow-500/10 text-yellow-600">
          <Info className="h-4 w-4" />
          <AlertDescription>
            {t(
              "autoEdit.freeEditionNotice",
              "Free-account renders include a watermark. Paid upgrades are not offered in this public edition.",
            )}
          </AlertDescription>
        </Alert>
      )}

      {/* Game Selection */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <Video className="w-5 h-5" />
            {t("autoEdit.selectGames")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("autoEdit.selectGamesDescription")}
          </p>
        </div>
        <div>
          {gamesLoading ? (
            <div className="flex items-center justify-center p-8">
              <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
            </div>
          ) : availableGames.length === 0 ? (
            <Alert>
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>
                {t("autoEdit.noGamesAvailable")}
              </AlertDescription>
            </Alert>
          ) : (
            <div
              className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3"
              data-testid="game-selection-grid"
            >
              {availableGames.map((game) => (
                <div
                  key={game.game_id}
                  data-testid={`game-card-${game.game_id}`}
                  className={`bg-black/40 rounded-lg border border-white/5 p-4 transition-all ${
                    selectedGameIds.includes(game.game_id)
                      ? "ring-2 ring-primary bg-primary/5"
                      : "hover:bg-white/5"
                  } ${pinnedClipCount > 0 ? "cursor-not-allowed opacity-70" : "cursor-pointer"}`}
                  role="checkbox"
                  tabIndex={0}
                  aria-checked={selectedGameIds.includes(game.game_id)}
                  aria-disabled={pinnedClipCount > 0}
                  onClick={() => {
                    if (pinnedClipCount === 0) onToggleGame(game.game_id);
                  }}
                  onKeyDown={(e) => {
                    if (
                      pinnedClipCount === 0 &&
                      (e.key === "Enter" || e.key === " ")
                    ) {
                      e.preventDefault();
                      onToggleGame(game.game_id);
                    }
                  }}
                >
                  <div className="flex items-start justify-between">
                    <div className="flex-1">
                      <div className="font-medium">{game.champion}</div>
                      <div className="text-sm text-muted-foreground">
                        {game.game_mode}
                      </div>
                      <div className="text-xs text-muted-foreground mt-1">
                        {game.date} •{" "}
                        {t("autoEdit.clipsCount", { count: game.clip_count })}
                      </div>
                    </div>
                    {selectedGameIds.includes(game.game_id) && (
                      <CheckCircle2 className="w-5 h-5 text-primary flex-shrink-0" />
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}

          {selectedGameIds.length > 0 && (
            <div className="mt-4 p-3 bg-primary/10 rounded-lg">
              <p className="text-sm font-medium">
                {t("autoEdit.gamesSelected", { count: selectedGameIds.length })}
              </p>
            </div>
          )}

          {/*
            홈에서 이미 클립을 골라 넘어온 경우. 이걸 말하지 않으면 "목표 60초"를
            보면서 30초짜리 결과를 받고도 이유를 알 수 없다 — 그 순간 길이 설정은
            거짓말이 된다.
          */}
          {pinnedClipCount > 0 && (
            <div
              className="mt-3 rounded-lg border border-primary/30 bg-primary/5 p-3"
              data-testid="auto-edit-pinned-clips"
            >
              <p
                className="text-sm font-medium"
                style={{ wordBreak: "keep-all" }}
              >
                {t("autoEdit.pinnedClips.summary", {
                  clips: pinnedClipCount,
                  games: pinnedGameCount,
                  duration: formatDuration(pinnedDuration),
                })}
              </p>
              <p
                className="mt-0.5 text-xs text-muted-foreground"
                style={{ wordBreak: "keep-all" }}
              >
                {t("autoEdit.pinnedClips.description")}
              </p>
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="mt-3"
                onClick={onUseAutomaticSelection}
                data-testid="use-automatic-selection"
              >
                {t("autoEdit.pinnedClips.useAutomatic")}
              </Button>
            </div>
          )}
        </div>

        {/* Selection Rationale */}
        {selectedGameIds.length > 0 && (
          <div className="mt-6 p-4 bg-muted/30 rounded-lg border border-white/5">
            <div className="flex items-center gap-2 text-sm font-medium mb-2">
              <Info className="w-4 h-4 text-primary" />
              {t("autoEdit.selectionRationaleTitle")}
            </div>
            <div className="text-xs text-muted-foreground space-y-2">
              <p>
                {/* 고른 클립이 있으면 자동 선택 설명은 사실이 아니다. */}
                {pinnedClipCount > 0
                  ? t("autoEdit.pinnedClips.rationale", {
                      count: pinnedClipCount,
                    })
                  : t("autoEdit.rationaleDescription")}
              </p>

              <div className="flex flex-wrap gap-2">
                {availableGames
                  .filter((g) => selectedGameIds.includes(g.game_id))
                  .map((g) => (
                    <Badge
                      key={g.game_id}
                      variant="outline"
                      className="text-[10px] px-1.5 py-0"
                    >
                      {g.champion} ({g.clip_count} clips)
                    </Badge>
                  ))}
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Duration Selection */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <Clock className="w-5 h-5" />
            {t("autoEdit.targetDuration")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("autoEdit.targetDurationDescription")}
          </p>
        </div>
        <div className="grid grid-cols-3 gap-3">
          {([60, 120, 180] as DurationOption[]).map((duration) => (
            <div
              key={duration}
              data-testid={`duration-${duration}`}
              className={`bg-black/40 rounded-lg border border-white/5 p-6 text-center cursor-pointer transition-all ${
                targetDuration === duration
                  ? "ring-2 ring-primary bg-primary/5"
                  : "hover:bg-white/5"
              }`}
              role="radio"
              tabIndex={0}
              aria-checked={targetDuration === duration}
              onClick={() => onSetDuration(duration)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSetDuration(duration);
                }
              }}
            >
              <div className="text-3xl font-bold text-gaming-cyan">
                {duration}s
              </div>
              <div className="text-sm text-muted-foreground mt-1">
                {duration === 60 && t("autoEdit.quickShort")}
                {duration === 120 && t("autoEdit.standard")}
                {duration === 180 && t("autoEdit.extended")}
              </div>
            </div>
          ))}
        </div>

        {pinnedClipCount > 0 && pinnedDuration > targetDuration && (
          <Alert
            className="mt-4 border-yellow-500/50 bg-yellow-500/10 text-yellow-600"
            data-testid="pinned-duration-warning"
          >
            <AlertTriangle className="h-4 w-4" />
            <AlertDescription>
              {t("autoEdit.pinnedClips.overTargetWarning", {
                estimated: formatDuration(pinnedDuration),
                target: formatDuration(targetDuration),
              })}
            </AlertDescription>
          </Alert>
        )}

        {/* Experimental: event zoom */}
        <div className="mt-4 flex items-center justify-between p-3 bg-black/40 rounded-lg border border-white/5">
          <div className="flex items-start gap-2">
            <ZoomIn className="w-4 h-4 mt-0.5 text-muted-foreground" />
            <div>
              <div className="flex items-center gap-2">
                <Label
                  htmlFor="enable-event-zoom"
                  className="text-sm font-medium cursor-pointer"
                >
                  {t("autoEdit.eventZoom.label")}
                </Label>
                <Badge variant="outline" className="text-[10px] px-1.5 py-0">
                  {t("autoEdit.eventZoom.experimental")}
                </Badge>
              </div>
              <p className="text-xs text-muted-foreground mt-0.5">
                {t("autoEdit.eventZoom.description")}
              </p>
            </div>
          </div>
          <Switch
            id="enable-event-zoom"
            checked={enableEventZoom}
            onCheckedChange={onToggleEventZoom}
            data-testid="enable-event-zoom-toggle"
          />
        </div>

        {/*
          훅 자막 — 기본 켜짐. 자막 없는 세로 클립은 쇼츠가 아니라 세로 상자에
          든 클립이라, 아무것도 건드리지 않은 사용자의 결과물이 그대로 올릴 만해야
          한다. 직접 자막을 얹는 사람을 위해 끌 수 있게만 남긴다.
        */}
        <label
          htmlFor="enable-hook-captions"
          className="mt-3 flex min-h-[44px] cursor-pointer items-center justify-between gap-3 rounded-lg border border-white/5 bg-black/40 p-3"
        >
          <div className="flex items-start gap-2">
            <Type className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
            <div>
              <div className="text-sm font-medium">
                {t("autoEdit.hookCaption.label")}
              </div>
              <p
                className="mt-0.5 text-xs text-muted-foreground"
                style={{ wordBreak: "keep-all" }}
              >
                {t("autoEdit.hookCaption.description")}
              </p>
            </div>
          </div>
          <Switch
            id="enable-hook-captions"
            checked={enableHookCaptions}
            onCheckedChange={onToggleHookCaptions}
            data-testid="enable-hook-captions-toggle"
          />
        </label>
      </div>

      {/* Shorts Planning */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <Type className="w-5 h-5" />
            {t("autoEdit.shortsPlanning")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("autoEdit.shortsPlanningDescription")}
          </p>
        </div>
        <div className="space-y-4">
          <div className="space-y-2">
            <Label className="text-sm font-medium flex items-center gap-2">
              <Type className="w-3 h-3" />
              {t("autoEdit.metadataTitleLabel")}
            </Label>
            <Input
              value={metadata.title}
              onChange={(e) => onMetadataChange({ title: e.target.value })}
              placeholder={t("autoEdit.titlePlaceholder")}
              className="bg-black/40 border-white/5"
            />
          </div>
          <div className="space-y-2">
            <Label className="text-sm font-medium flex items-center gap-2">
              <AlignLeft className="w-3 h-3" />
              {t("autoEdit.metadataCaptionLabel")}
            </Label>
            <textarea
              value={metadata.caption}
              onChange={(e) => onMetadataChange({ caption: e.target.value })}
              placeholder={t("autoEdit.captionPlaceholder")}
              className="w-full min-h-[80px] p-3 rounded-md bg-black/40 border border-white/5 text-sm focus:ring-2 focus:ring-primary outline-none transition-all"
            />
          </div>
          <div className="space-y-2">
            <Label className="text-sm font-medium flex items-center gap-2">
              <Tag className="w-3 h-3" />
              {t("autoEdit.metadataTagsLabel")}
            </Label>
            <Input
              value={metadata.tags.join(", ")}
              onChange={(e) =>
                onMetadataChange({
                  tags: e.target.value
                    .split(",")
                    .map((t) => t.trim())
                    .filter(Boolean),
                })
              }
              placeholder={t("autoEdit.tagsPlaceholder")}
              className="bg-black/40 border-white/5"
            />
          </div>
        </div>
      </div>

      {/* Optional Enhancements */}

      <Tabs defaultValue="canvas" className="w-full">
        <TabsList className="grid w-full grid-cols-2">
          <TabsTrigger value="canvas" data-testid="canvas-tab">
            <Palette className="w-4 h-4 mr-2" />
            {t("autoEdit.canvasOverlay")}
          </TabsTrigger>
          <TabsTrigger value="audio" data-testid="audio-tab">
            <Music className="w-4 h-4 mr-2" />
            {t("autoEdit.backgroundMusic")}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="canvas" className="mt-4">
          <div className="bg-black/40 rounded-lg border border-white/5 p-6">
            <CanvasEditor
              template={currentTemplate}
              onTemplateChange={onTemplateChange}
            />
          </div>
        </TabsContent>

        <TabsContent value="audio" className="mt-4">
          <AudioMixer
            backgroundMusic={backgroundMusic}
            audioLevels={audioLevels}
            onBackgroundMusicChange={onBackgroundMusicChange}
            onAudioLevelsChange={onAudioLevelsChange}
          />
        </TabsContent>
      </Tabs>

      {/* Generate Button */}
      <div className="flex items-center justify-end gap-3">
        <AutoEditQuotaBadge />
        <Button
          size="lg"
          onClick={onGenerate}
          disabled={selectedGameIds.length === 0 || isLoading}
          data-testid="generate-button"
        >
          {isLoading ? (
            <>
              <Loader2 className="w-5 h-5 mr-2 animate-spin" />
              {t("autoEdit.starting")}
            </>
          ) : (
            <>
              <Sparkles className="w-5 h-5 mr-2" />
              {t("autoEdit.generateShort")}
            </>
          )}
        </Button>
      </div>
    </div>
  );
}
