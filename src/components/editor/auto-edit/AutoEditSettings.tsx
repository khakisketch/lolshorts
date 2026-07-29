import { useTranslation } from 'react-i18next';
import { CanvasEditor } from '../canvas/CanvasEditor';
import { AudioMixer } from '../AudioMixer';
import { AutoEditQuotaBadge } from '../AutoEditQuotaBadge';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  Video,
  Clock,
  CheckCircle2,
  AlertCircle,
  Loader2,
  Palette,
  Music,
  Sparkles,
  Info,
  Type,
  AlignLeft,
  Tag,
  ZoomIn,
} from 'lucide-react';
import { DurationOption, GameSelection, CanvasTemplate, AudioLevels, BackgroundMusic, AutoEditMetadata } from '@/types/autoEdit';

interface AutoEditSettingsProps {
  availableGames: GameSelection[];
  selectedGameIds: string[];
  targetDuration: DurationOption;
  enableEventZoom: boolean;
  currentTemplate: CanvasTemplate | null;
  backgroundMusic: BackgroundMusic | null;
  audioLevels: AudioLevels;
  metadata: AutoEditMetadata;
  isLoading: boolean;
  gamesLoading: boolean;
  isFreeTier: boolean;
  onToggleGame: (gameId: string) => void;
  onSetDuration: (duration: DurationOption) => void;
  onToggleEventZoom: (enabled: boolean) => void;
  onTemplateChange: (template: CanvasTemplate) => void;
  onBackgroundMusicChange: (music: BackgroundMusic | null) => void;
  onAudioLevelsChange: (levels: Partial<AudioLevels>) => void;
  onMetadataChange: (metadata: Partial<AutoEditMetadata>) => void;
  onGenerate: () => void;
}

export function AutoEditSettings({
  availableGames,
  selectedGameIds,
  targetDuration,
  enableEventZoom,
  currentTemplate,
  backgroundMusic,
  audioLevels,
  metadata,
  isLoading,
  gamesLoading,
  isFreeTier,
  onToggleGame,
  onSetDuration,
  onToggleEventZoom,
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
            {t('autoEdit.watermarkNotice')}{' '}
            <span className="font-bold underline cursor-pointer">{t('autoEdit.upgradeToPro')}</span>
          </AlertDescription>
        </Alert>
      )}

      {/* Game Selection */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <Video className="w-5 h-5" />
            {t('autoEdit.selectGames')}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t('autoEdit.selectGamesDescription')}
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
                {t('autoEdit.noGamesAvailable')}
              </AlertDescription>
            </Alert>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3" data-testid="game-selection-grid">
              {availableGames.map(game => (
                <div
                  key={game.game_id}
                  data-testid={`game-card-${game.game_id}`}
                  className={`bg-black/40 rounded-lg border border-white/5 p-4 cursor-pointer transition-all ${
                    selectedGameIds.includes(game.game_id)
                      ? 'ring-2 ring-primary bg-primary/5'
                      : 'hover:bg-white/5'
                  }`}
                  role="checkbox"
                  tabIndex={0}
                  aria-checked={selectedGameIds.includes(game.game_id)}
                  onClick={() => onToggleGame(game.game_id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
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
                        {game.date} • {t('autoEdit.clipsCount', { count: game.clip_count })}
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
                {t('autoEdit.gamesSelected', { count: selectedGameIds.length })}
              </p>
            </div>
          )}
        </div>

        {/* Selection Rationale */}
        {selectedGameIds.length > 0 && (
          <div className="mt-6 p-4 bg-muted/30 rounded-lg border border-white/5">
            <div className="flex items-center gap-2 text-sm font-medium mb-2">
              <Info className="w-4 h-4 text-primary" />
              {t('autoEdit.selectionRationaleTitle', 'Selection Rationale')}
            </div>
            <div className="text-xs text-muted-foreground space-y-2">
                <p>
                  {t('autoEdit.rationaleDescription', 
                    `Selected based on ${selectedGameIds.length} game(s) with a target duration of ${targetDuration}s. ` +
                    `The system will select clips based on event priority and usage history to fit your target duration.`
                  )}
                </p>

              <div className="flex flex-wrap gap-2">
                {availableGames
                  .filter(g => selectedGameIds.includes(g.game_id))
                  .map(g => (
                    <Badge key={g.game_id} variant="outline" className="text-[10px] px-1.5 py-0">
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
            {t('autoEdit.targetDuration')}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t('autoEdit.targetDurationDescription')}
          </p>
        </div>
        <div className="grid grid-cols-3 gap-3">
          {([60, 120, 180] as DurationOption[]).map(duration => (
            <div
              key={duration}
              data-testid={`duration-${duration}`}
              className={`bg-black/40 rounded-lg border border-white/5 p-6 text-center cursor-pointer transition-all ${
                targetDuration === duration
                  ? 'ring-2 ring-primary bg-primary/5'
                  : 'hover:bg-white/5'
              }`}
              role="radio"
              tabIndex={0}
              aria-checked={targetDuration === duration}
              onClick={() => onSetDuration(duration)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onSetDuration(duration);
                }
              }}
            >
              <div className="text-3xl font-bold text-gaming-cyan">
                {duration}s
              </div>
              <div className="text-sm text-muted-foreground mt-1">
                {duration === 60 && t('autoEdit.quickShort')}
                {duration === 120 && t('autoEdit.standard')}
                {duration === 180 && t('autoEdit.extended')}
              </div>
            </div>
          ))}
        </div>

        {/* Experimental: event zoom */}
        <div className="mt-4 flex items-center justify-between p-3 bg-black/40 rounded-lg border border-white/5">
          <div className="flex items-start gap-2">
            <ZoomIn className="w-4 h-4 mt-0.5 text-muted-foreground" />
            <div>
              <div className="flex items-center gap-2">
                <Label htmlFor="enable-event-zoom" className="text-sm font-medium cursor-pointer">
                  {t('autoEdit.eventZoom.label', 'Auto zoom on kill events')}
                </Label>
                <Badge variant="outline" className="text-[10px] px-1.5 py-0">
                  {t('autoEdit.eventZoom.experimental', 'Experimental')}
                </Badge>
              </div>
              <p className="text-xs text-muted-foreground mt-0.5">
                {t('autoEdit.eventZoom.description', 'Automatically zooms in at kill/event timestamps.')}
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
      </div>

      {/* Shorts Planning */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <Type className="w-5 h-5" />
            {t('autoEdit.shortsPlanning', 'Shorts Planning')}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t('autoEdit.shortsPlanningDescription', 'Plan your title, caption, and tags for the final output.')}
          </p>
        </div>
        <div className="space-y-4">
          <div className="space-y-2">
            <Label className="text-sm font-medium flex items-center gap-2">
              <Type className="w-3 h-3" />
              {t('autoEdit.metadataTitleLabel', 'Title')}
            </Label>
            <Input
              value={metadata.title}
              onChange={(e) => onMetadataChange({ title: e.target.value })}
              placeholder={t('autoEdit.titlePlaceholder', 'Enter a catchy title...')}
              className="bg-black/40 border-white/5"
            />
          </div>
          <div className="space-y-2">
            <Label className="text-sm font-medium flex items-center gap-2">
              <AlignLeft className="w-3 h-3" />
              {t('autoEdit.metadataCaptionLabel', 'Caption')}
            </Label>
            <textarea
              value={metadata.caption}
              onChange={(e) => onMetadataChange({ caption: e.target.value })}
              placeholder={t('autoEdit.captionPlaceholder', 'Add a description or call to action...')}
              className="w-full min-h-[80px] p-3 rounded-md bg-black/40 border border-white/5 text-sm focus:ring-2 focus:ring-primary outline-none transition-all"
            />
          </div>
          <div className="space-y-2">
            <Label className="text-sm font-medium flex items-center gap-2">
              <Tag className="w-3 h-3" />
              {t('autoEdit.metadataTagsLabel', 'Tags')}
            </Label>
            <Input
              value={metadata.tags.join(', ')}
              onChange={(e) => onMetadataChange({ tags: e.target.value.split(',').map(t => t.trim()).filter(Boolean) })}
              placeholder={t('autoEdit.tagsPlaceholder', 'gaming, lol, highlights...')}
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
            {t('autoEdit.canvasOverlay')}
          </TabsTrigger>
          <TabsTrigger value="audio" data-testid="audio-tab">
            <Music className="w-4 h-4 mr-2" />
            {t('autoEdit.backgroundMusic')}
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
              {t('autoEdit.starting')}
            </>
          ) : (
            <>
              <Sparkles className="w-5 h-5 mr-2" />
              {t('autoEdit.generateShort')}
            </>
          )}
        </Button>
      </div>
    </div>
  );
}
