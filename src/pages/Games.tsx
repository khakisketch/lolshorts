import { useState, useEffect, useCallback } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import { useConfirmDialog } from "@/components/ui/confirm-dialog";


import { SpinnerCenter } from "@/components/ui/spinner";
import { Skeleton, SkeletonStats } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/ui/empty-state";
import { useStorage } from "@/hooks/useStorage";
import { useEditorStore } from "@/stores/editorStore";
import { formatDuration, formatStorage, pageStyles, getErrorMessage } from "@/lib/utils";
import { GameMetadata } from "@/types/storage";
import { Trash2, Scissors, Calendar, Clock, Trophy, Gamepad2, Search, Filter, Sparkles } from "lucide-react";

import { logger } from '@/lib/logger';

export function Games() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { listGames, getGameMetadata, deleteGame, getStorageStats, isLoading, error } = useStorage();
  const { confirm, ConfirmDialog } = useConfirmDialog();
  const setSelectedGameId = useEditorStore((state) => state.setSelectedGameId);
  const [games, setGames] = useState<string[]>([]);
  const [gamesData, setGamesData] = useState<Map<string, GameMetadata>>(new Map());
  const [stats, setStats] = useState({ total_games: 0, total_clips: 0, total_size_bytes: 0 });
  const [searchQuery, setSearchQuery] = useState('');
  const [filterMode, setFilterMode] = useState<string>('all');


  const loadGames = useCallback(async () => {
    try {
      // An IPC miss/failure returns null; assigning it and then iterating threw
      // before the empty state could ever render.
      const loadedGames = (await listGames()) ?? [];
      setGames(loadedGames);

      const dataMap = new Map<string, GameMetadata>();
      for (const gameId of loadedGames) {
        try {
          const metadata = await getGameMetadata(gameId);
          dataMap.set(gameId, metadata);
        } catch (err) {
          logger.error(`Failed to load metadata for game ${gameId}:`, err);
        }
      }
      setGamesData(dataMap);
    } catch (err) {
      logger.error("Failed to load games:", err);
    }
  }, [listGames, getGameMetadata]);

  const loadStats = useCallback(async () => {
    try {
      const storageStats = await getStorageStats();
      setStats(storageStats);
    } catch (err) {
      logger.error("Failed to load stats:", err);
    }
  }, [getStorageStats]);

  const filteredGames = games.filter(gameId => {
    const metadata = gamesData.get(gameId);
    if (!metadata) return true;

    // Search filter
    const searchLower = searchQuery.toLowerCase();
    const matchesSearch = 
      metadata.champion.toLowerCase().includes(searchLower) ||
      metadata.game_mode.toLowerCase().includes(searchLower) ||
      metadata.game_id.toLowerCase().includes(searchLower);

    if (!matchesSearch) return false;

    // Mode filter
    if (filterMode !== 'all' && metadata.game_mode !== filterMode) {
      return false;
    }

    return true;
  });

  useEffect(() => {
    loadGames();
    loadStats();
  }, [loadGames, loadStats]);

  const handleDeleteGame = async (gameId: string) => {
    const confirmed = await confirm({
      title: t('games.deleteConfirmTitle'),
      description: t('games.deleteConfirmDescription'),
      confirmText: t('common.delete'),
      cancelText: t('common.cancel'),
      variant: 'danger',
    });

    if (!confirmed) {
      return;
    }

    try {
      await deleteGame(gameId);
      await loadGames();
      await loadStats();
    } catch (err) {
      logger.error("Failed to delete game:", err);
    }
  };

  /** "다듬기" — 이 게임의 클립을 편집기에서 손본다. */
  const handlePolish = (gameId: string) => {
    setSelectedGameId(gameId);
    navigate({ to: '/editor', search: { gameId } });
  };

  /**
   * "하이라이트 만들기" — 이 게임의 클립들을 한 편의 영상으로 합친다.
   *
   * 자동 파일럿 방향에서는 결국 게임이 끝나면 알아서 돌아야 하지만, 그 자동 실행이
   * 아직 없다. 이 진입점까지 없애면 앱에 산출물을 만들 방법이 하나도 남지 않는다
   * (`start_auto_edit` 호출부는 AutoEditPanel 하나뿐이다).
   */
  const handleCreateHighlight = (gameId: string) => {
    setSelectedGameId(gameId);
    navigate({ to: '/auto-edit', search: { gameId } });
  };

  const getResultClass = (result: string | null) => {
    if (result === "Win") return "text-gaming-cyan border-gaming-cyan/40 bg-gaming-cyan/10";
    if (result === "Loss") return "text-gaming-magenta border-gaming-magenta/40 bg-gaming-magenta/10";
    return "text-muted-foreground border-white/20 bg-white/5";
  };

  if (isLoading && games.length === 0) {
    return (
      <div className="space-y-6">
        <div className="flex items-center justify-between mb-6">
          <Skeleton className="h-9 w-48" />
          <Skeleton className="h-9 w-24" />
        </div>
        <SkeletonStats />
        <div className="space-y-4">
          {[1, 2, 3].map((i) => (
            <div key={i} className="bg-black/40 rounded-lg border border-white/5 p-4">
              <div className="flex items-start justify-between">
                <div className="space-y-2">
                  <Skeleton className="h-6 w-48" />
                  <Skeleton className="h-4 w-32" />
                </div>
                <div className="flex gap-2">
                  <Skeleton className="h-9 w-28" />
                  <Skeleton className="h-9 w-28" />
                  <Skeleton className="h-9 w-9" />
                </div>
              </div>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mt-4">
                {[1, 2, 3, 4].map((j) => (
                  <div key={j} className="space-y-1">
                    <Skeleton variant="text" className="w-16" />
                    <Skeleton className="h-5 w-24" />
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className={pageStyles.container}>
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2">
        <h2 className="text-xl font-bold">{t('games.recordedGames')}</h2>
        <Button onClick={loadGames} variant="outline" size="sm">
          {t('games.refresh')}
        </Button>
      </div>

      {/* Storage Stats */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
        <div className="bg-black/40 rounded-lg border border-white/5 p-4">
          <p className="text-sm text-muted-foreground">{t('games.stats.totalGames')}</p>
          <p className="text-3xl font-bold text-gaming-cyan mt-1">{stats.total_games}</p>
        </div>
        <div className="bg-black/40 rounded-lg border border-white/5 p-4">
          <p className="text-sm text-muted-foreground">{t('games.stats.totalClips')}</p>
          <p className="text-3xl font-bold text-gaming-cyan mt-1">{stats.total_clips}</p>
        </div>
        <div className="bg-black/40 rounded-lg border border-white/5 p-4">
          <p className="text-sm text-muted-foreground">{t('games.stats.storageUsed')}</p>
          <p className="text-3xl font-bold text-gaming-cyan mt-1">{formatStorage(stats.total_size_bytes)}</p>
        </div>
      </div>

      {error && (
        <div className="p-4 mb-6 bg-gaming-magenta/10 border border-gaming-magenta/30 rounded-lg">
          <p className="text-sm text-gaming-magenta">{getErrorMessage(error)}</p>
        </div>
      )}

      {/* Filters — only worth showing once there is something to filter. */}
      {games.length > 0 && (
        <div className="bg-black/40 rounded-lg border border-white/5 p-4">
          <div className="flex flex-col sm:flex-row gap-3">
            <div className="flex-1 relative">
              <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder={t('games.searchPlaceholder')}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-9"
              />
            </div>
            <div className="flex items-center gap-2">
              <Filter className="h-4 w-4 text-muted-foreground shrink-0" />
              <Select value={filterMode} onValueChange={setFilterMode}>
                <SelectTrigger className="w-full sm:w-[180px]">
                  <SelectValue placeholder={t('games.filterModePlaceholder')} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{t('games.filter.allModes')}</SelectItem>
                  {Array.from(new Set(Array.from(gamesData.values()).map(g => g.game_mode))).map(mode => (
                    <SelectItem key={mode} value={mode}>{mode}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
        </div>
      )}

      {/* Games List */}
      {games.length === 0 ? (
        <div className="gaming-panel p-8">
          <EmptyState
            icon={Gamepad2}
            title={t('games.noGamesRecorded')}
            description={
              <span style={{ wordBreak: 'keep-all' }}>
                {t('games.startRecordingPrompt')}
              </span>
            }
            action={{
              label: t('games.goHome'),
              onClick: () => navigate({ to: '/' }),
            }}
            size="lg"
          />
        </div>
      ) : filteredGames.length === 0 ? (
        <div className="gaming-panel p-8">
          <EmptyState
            icon={Search}
            title={t('games.noGamesMatchFilters')}
            description={t('games.tryDifferentFilters')}
            action={{
              label: t('games.clearFilters'),
              onClick: () => {
                setSearchQuery('');
                setFilterMode('all');
              },
            }}
            size="lg"
          />
        </div>
      ) : (
        <div className="space-y-4">
          {filteredGames.map((gameId) => {
            const gameMetadata = gamesData.get(gameId);

            if (!gameMetadata) {
              return (
                <div key={gameId} className="bg-black/40 rounded-lg border border-white/5 p-4">
                  <div className="flex items-start justify-between">
                    <div className="space-y-2">
                      <Skeleton className="h-6 w-48" />
                      <Skeleton className="h-4 w-32" />
                    </div>
                  </div>
                  <SpinnerCenter size="md" label={t('games.loadingGameData')} className="py-4" />
                </div>
              );
            }

            return (
              <div key={gameId} className="gaming-panel p-0 overflow-hidden">
                <div className="p-4">
                  <div className="flex items-start justify-between">
                    <div className="flex-1">
                      <div className="flex items-center gap-2 mb-1">
                        <Trophy className="w-5 h-5 text-gaming-cyan" />
                        <span className="text-base font-semibold">
                          {gameMetadata.champion} - {gameMetadata.game_mode}
                        </span>
                        {gameMetadata.result && (
                          <span className={`text-xs font-bold px-2 py-0.5 rounded border ${getResultClass(gameMetadata.result)}`}>
                            {gameMetadata.result === 'Win'
                              ? t('games.game.victory')
                              : gameMetadata.result === 'Loss'
                                ? t('games.game.defeat')
                                : t('games.game.remake')}
                          </span>
                        )}
                        <Badge variant="outline" className="text-[10px] tracking-wider opacity-70">
                          {t('games.badge.recorded')}
                        </Badge>
                      </div>
                      <p className="text-sm text-muted-foreground">
                        {t('games.game.gameId')}: {gameMetadata.game_id}
                      </p>
                    </div>
                    <div className="flex gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        className="min-h-[44px]"
                        onClick={() => handlePolish(gameMetadata.game_id)}
                        data-testid={`game-polish-${gameMetadata.game_id}`}
                      >
                        <Scissors className="w-4 h-4 mr-2" />
                        {t('results.polish')}
                      </Button>
                      <Button
                        size="sm"
                        className="min-h-[44px]"
                        onClick={() => handleCreateHighlight(gameMetadata.game_id)}
                        data-testid={`game-make-highlight-${gameMetadata.game_id}`}
                      >
                        <Sparkles className="w-4 h-4 mr-2" />
                        {t('results.makeHighlight')}
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="min-h-[44px] text-gaming-magenta hover:text-gaming-magenta hover:bg-gaming-magenta/10"
                        onClick={() => handleDeleteGame(gameMetadata.game_id)}
                        aria-label={t('games.game.delete')}
                      >
                        <Trash2 className="w-4 h-4" />
                      </Button>
                    </div>
                  </div>
                </div>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm px-4 pb-4 border-t border-white/5 pt-4 bg-black/20">
                  <div>
                    <p className="text-muted-foreground flex items-center gap-1">
                      <Calendar className="w-4 h-4" />
                      {t('games.game.date')}
                    </p>
                    <p className="font-medium">
                      {new Date(gameMetadata.start_time).toLocaleDateString()}
                    </p>
                  </div>
                  <div>
                    <p className="text-muted-foreground flex items-center gap-1">
                      <Clock className="w-4 h-4" />
                      {t('games.game.duration')}
                    </p>
                    <p className="font-medium">
                      {gameMetadata.end_time
                        ? formatDuration((new Date(gameMetadata.end_time).getTime() - new Date(gameMetadata.start_time).getTime()) / 1000)
                        : t('games.game.inProgress', 'In Progress')}
                    </p>
                    <Badge variant="outline" className={gameMetadata.end_time ? "text-green-500 border-green-500/30 bg-green-500/5" : "text-yellow-500 border-yellow-500/30 bg-yellow-500/5"}>
                      {gameMetadata.end_time ? t('games.game.complete') : t('games.game.inProgress')}
                    </Badge>
                   </div>

                  <div>
                    <p className="text-muted-foreground">{t('games.game.kda')}</p>
                    <p className="font-medium text-gaming-cyan">
                      {gameMetadata.kda
                        ? `${gameMetadata.kda.kills} / ${gameMetadata.kda.deaths} / ${gameMetadata.kda.assists}`
                        : '-'}
                    </p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">{t('games.game.recorded')}</p>
                    <p className="font-medium">
                      {new Date(gameMetadata.start_time).toLocaleString()}
                    </p>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <ConfirmDialog />
    </div>
  );
}
