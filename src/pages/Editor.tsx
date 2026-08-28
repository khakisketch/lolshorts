import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { dirname } from "@tauri-apps/api/path";
import { useEditorStore } from "@/stores/editorStore";
import { useEditor } from "@/hooks/useEditor";
import { useStorage } from "@/hooks/useStorage";
import { EditorLayout } from "@/components/editor/EditorLayout";
import { ClipLibrary } from "@/components/editor/ClipLibrary";
import { VideoPreview } from "@/components/editor/VideoPreview";
import { CompositionSettings } from "@/components/editor/CompositionSettings";
import { EffectsPanel } from "@/components/editor/EffectsPanel";
import { Timeline } from "@/components/editor/Timeline";
import { ExportModal } from "@/components/editor/ExportModal";
import { StudioModeNav } from "@/components/editor/StudioModeNav";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Loader2, Video, AlertCircle } from "lucide-react";
import { logger } from "@/lib/logger";
import { VideoErrorBoundary } from "@/components/ErrorBoundary";
import { useToast } from "@/components/ui/use-toast";

export function Editor() {
  const { t } = useTranslation();
  const { toast } = useToast();
  const {
    selectedGameId,
    setSelectedGameId,
    availableClips,
    setAvailableClips,
    selectedClipId,
  } = useEditorStore();
  const { loadGameClips, isLoading, error } = useEditor();
  const {
    getAllGames,
    isLoading: isLoadingGames,
    error: gamesError,
  } = useStorage();

  const [games, setGames] = useState<
    Array<{ id: string; date: string; name: string }>
  >([]);
  const [isExportModalOpen, setIsExportModalOpen] = useState(false);
  const [effectsOutputDir, setEffectsOutputDir] = useState<string | null>(null);

  // Effects (slow-motion/color grading/text overlay) apply to whichever clip
  // is currently selected for preview; derive its containing directory as
  // the default output location for the generated effect_*.mp4 files.
  const effectsInputPath =
    availableClips.find((c) => c.file_path === selectedClipId)?.file_path ??
    null;

  useEffect(() => {
    if (!effectsInputPath) {
      setEffectsOutputDir(null);
      return;
    }
    dirname(effectsInputPath)
      .then(setEffectsOutputDir)
      .catch((err) => {
        logger.error("Failed to resolve effects output directory:", err);
        setEffectsOutputDir(null);
      });
  }, [effectsInputPath]);

  const loadGames = useCallback(async () => {
    try {
      // Same guard as Games/Results: a failed or missing IPC returns null and
      // `.map` on it throws before the empty state can render.
      const allGames = (await getAllGames()) ?? [];
      const gameList = allGames.map((game) => ({
        id: game.game_id,
        date: new Date(game.start_time).toLocaleDateString(),
        name: `${game.champion} - ${game.game_mode}`,
      }));
      setGames(gameList);
    } catch (err) {
      logger.error("Failed to load games:", err);
      toast({
        title: t("editor.error.loadGamesFailed"),
        variant: "destructive",
      });
    }
  }, [getAllGames, t, toast]);

  useEffect(() => {
    loadGames();
  }, [loadGames]);

  useEffect(() => {
    if (selectedGameId) {
      const loadClips = async () => {
        try {
          const clips = await loadGameClips(selectedGameId);
          setAvailableClips(clips);
        } catch (err) {
          logger.error("Failed to load clips:", err);
          toast({
            title: t("editor.error.loadClipsFailed"),
            variant: "destructive",
          });
        }
      };

      loadClips();
    }
  }, [selectedGameId, loadGameClips, setAvailableClips, t, toast]);

  const handleGameSelect = (gameId: string) => {
    setSelectedGameId(gameId);
  };

  // Show game selection screen if no game selected
  if (!selectedGameId) {
    return (
      <div className="flex h-full flex-col gap-4 p-6">
        <StudioModeNav active="manual" />
        <div className="flex flex-1 items-center justify-center">
          <div className="gaming-panel w-full max-w-md p-6">
            <div className="mb-1 flex items-center gap-2">
              <Video className="h-6 w-6 text-gaming-cyan" />
              <h2
                className="text-lg font-semibold"
                data-autofocus
                tabIndex={-1}
              >
                {t("editor.selectGameTitle")}
              </h2>
            </div>
            <p className="mb-6 text-sm text-muted-foreground">
              {t("editor.selectGameDescription")}
            </p>

            {isLoadingGames ? (
              <div className="flex flex-col items-center justify-center gap-3 p-8">
                <Loader2 className="w-8 h-8 animate-spin text-gaming-cyan" />
                <p className="text-sm text-muted-foreground">
                  {t("editor.loadingGames")}
                </p>
              </div>
            ) : gamesError ? (
              <div className="space-y-4">
                <div className="flex items-center gap-2">
                  <AlertCircle className="w-5 h-5 text-gaming-magenta" />
                  <h3 className="text-base font-semibold text-gaming-magenta">
                    {t("editor.errorLoadingGames")}
                  </h3>
                </div>
                <Alert variant="destructive">
                  <AlertDescription>{gamesError}</AlertDescription>
                </Alert>
                <Button
                  onClick={() => loadGames()}
                  variant="outline"
                  className="w-full"
                >
                  {t("editor.retry")}
                </Button>
              </div>
            ) : games.length === 0 ? (
              <Alert>
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>
                  {t("editor.noGamesAvailable")}
                </AlertDescription>
              </Alert>
            ) : (
              <>
                <div className="space-y-2">
                  <label className="text-sm font-medium">
                    {t("editor.selectGame")}
                  </label>
                  <Select onValueChange={handleGameSelect}>
                    <SelectTrigger>
                      <SelectValue placeholder={t("editor.chooseGame")} />
                    </SelectTrigger>
                    <SelectContent>
                      {games.map((game) => (
                        <SelectItem key={game.id} value={game.id}>
                          <div className="flex items-center justify-between w-full">
                            <span>{game.name}</span>
                            <Badge variant="outline" className="ml-2">
                              {game.date}
                            </Badge>
                          </div>
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>

                <div className="mt-4 p-4 bg-black/40 rounded-lg border border-white/5 text-sm text-muted-foreground">
                  <p>{t("editor.selectGamePrompt")}</p>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    );
  }

  // Show loading state while clips are loading
  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center space-y-4">
          <Loader2 className="w-12 h-12 animate-spin text-gaming-cyan mx-auto" />
          <p className="text-muted-foreground">{t("editor.loadingClips")}</p>
        </div>
      </div>
    );
  }

  // Show error state if clips failed to load
  if (error) {
    return (
      <div className="flex items-center justify-center h-full p-6">
        <div className="gaming-panel p-6 w-full max-w-md">
          <div className="flex items-center gap-2 mb-1">
            <AlertCircle className="w-6 h-6 text-gaming-magenta" />
            <h2 className="text-lg font-semibold text-gaming-magenta">
              {t("editor.errorLoadingClips")}
            </h2>
          </div>
          <div className="space-y-4 mt-4">
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
            <div className="flex gap-2">
              <Button
                variant="outline"
                onClick={() => setSelectedGameId(null)}
                className="flex-1"
              >
                {t("editor.backToGameSelection")}
              </Button>
              <Button
                onClick={() => window.location.reload()}
                className="flex-1"
              >
                {t("editor.retry")}
              </Button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Show empty state when game is selected but has no clips
  if (availableClips.length === 0 && selectedGameId && !isLoading && !error) {
    return (
      <div className="flex items-center justify-center h-full p-6">
        <div className="gaming-panel p-6 w-full max-w-md text-center">
          <Video className="w-12 h-12 text-muted-foreground mx-auto mb-4" />
          <h2 className="text-lg font-semibold mb-2">{t("editor.noClips")}</h2>
          <p className="text-sm text-muted-foreground mb-4">
            {t("editor.noClipsDescription")}
          </p>
          <Button variant="outline" onClick={() => setSelectedGameId(null)}>
            {t("editor.backToGameSelection")}
          </Button>
        </div>
      </div>
    );
  }

  // Main editor interface
  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Header Bar */}
      <div className="border-b border-white/5 p-4 bg-[hsl(240,18%,9%)]">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-4">
            <Video className="w-6 h-6 text-gaming-cyan" />
            <div>
              <h2 className="text-lg font-semibold">{t("editor.title")}</h2>
              <p className="text-sm text-muted-foreground">
                {t("editor.gameWithClips", {
                  gameId: selectedGameId,
                  count: availableClips.length,
                })}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setSelectedGameId(null)}
            >
              {t("editor.changeGame")}
            </Button>
            <Button
              size="sm"
              onClick={() => setIsExportModalOpen(true)}
              disabled={availableClips.length === 0}
            >
              {t("editor.export.exportVideo")}
            </Button>
          </div>
        </div>
        <div className="mt-3">
          <StudioModeNav active="manual" />
        </div>
      </div>

      {/* Editor Layout */}
      <div className="flex-1 overflow-hidden">
        <EditorLayout
          clipLibrary={<ClipLibrary />}
          videoPreview={
            <VideoErrorBoundary>
              <VideoPreview />
            </VideoErrorBoundary>
          }
          compositionSettings={
            <CompositionSettings onExport={() => setIsExportModalOpen(true)} />
          }
          effectsPanel={
            <EffectsPanel
              inputPath={effectsInputPath}
              outputDir={effectsOutputDir}
            />
          }
          timeline={<Timeline />}
        />
      </div>

      <ExportModal
        isOpen={isExportModalOpen}
        onClose={() => setIsExportModalOpen(false)}
      />
    </div>
  );
}
