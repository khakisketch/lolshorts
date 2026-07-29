/**
 * Clip Library Component
 *
 * Displays, filters, and manages recorded gameplay clips
 */

import { useState, useEffect, useCallback } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Spinner } from '@/components/ui/spinner';
import { EmptyState } from '@/components/ui/empty-state';
import { Play, Trash2, Edit, Download, Search, Filter, Eye, Film, SearchX } from 'lucide-react';
import { storageApi } from '@/api/storage';
import { ClipMetadata } from '@/types/storage';
import { videoApi } from '@/api/video';
import { utilsApi } from '@/api/utils';
import { toast } from '@/components/ui/use-toast';
import { VideoModal } from './video/VideoModal';
import { logger } from '@/lib/logger';
import { getErrorMessage } from '@/lib/utils';

// Clip and Game interfaces removed (imported from api)

export function ClipLibrary() {
  const { t } = useTranslation();
  const [clips, setClips] = useState<ClipMetadata[]>([]);
  const [games, setGames] = useState<string[]>([]);
  const [selectedGame, setSelectedGame] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [filterPriority, setFilterPriority] = useState<string>('all');
  const [isLoading, setIsLoading] = useState(false);
  const [thumbnailGeneration, setThumbnailGeneration] = useState<Set<string>>(new Set());

  // Video player state
  const [isVideoModalOpen, setIsVideoModalOpen] = useState(false);
  const [currentVideoSrc, setCurrentVideoSrc] = useState<string>('');
  const [currentVideoTitle, setCurrentVideoTitle] = useState<string>('');

  const loadGames = useCallback(async () => {
    setIsLoading(true);
    try {
      const gameList = await storageApi.listGames();
      setGames(gameList);

      if (gameList.length > 0) {
        setSelectedGame(gameList[0]);
      }
    } catch (error) {
      logger.error('Failed to load games:', error);
      toast({
        title: t('clips.errors.loadGamesFailed'),
        description: getErrorMessage(error),
        variant: 'destructive',
      });
    } finally {
      setIsLoading(false);
    }
  }, [t]);

  const generateClipThumbnail = async (clip: ClipMetadata): Promise<string | null> => {
    try {
      const thumbnailPath = await videoApi.generateClipThumbnail(clip.file_path);
      return thumbnailPath;
    } catch (error) {
      logger.error('Failed to generate thumbnail for clip', clip.file_path, ':', error);
      return null;
    }
  };

  const loadClipsForGame = useCallback(async (gameId: string) => { // Change gameId type to string
    setIsLoading(true);
    try {
      // Call actual backend to get clips
      const clipList = await storageApi.listClips(gameId); // Pass string directly

      // Set clips first
      setClips(clipList);


      // Then generate thumbnails for clips that don't have them yet
      clipList.forEach(clip => {
        setThumbnailGeneration(prev => new Set(prev).add(clip.file_path));

        generateClipThumbnail(clip)
          .then(thumbnailPath => {
            if (thumbnailPath) {
              setClips(prev => prev.map(c =>
                c.file_path === clip.file_path
                  ? { ...c, thumbnail_path: thumbnailPath }
                  : c
              ));
            }
          })
          .finally(() => {
            setThumbnailGeneration(prev => {
              const newSet = new Set(prev);
              newSet.delete(clip.file_path);
              return newSet;
            });
          });
      });
    } catch (error) {
      logger.error('Failed to load clips:', error);
      toast({
        title: t('clips.errors.loadClipsFailed'),
        description: getErrorMessage(error),
        variant: 'destructive',
      });
      setClips([]);
    } finally {
      setIsLoading(false);
    }
  }, [t]);

  useEffect(() => {
    loadGames();
  }, [loadGames]);

  useEffect(() => {
    if (selectedGame !== null) {
      loadClipsForGame(selectedGame);
    }
  }, [selectedGame, loadClipsForGame]);

  // Keyboard navigation support... (remains same)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl/Cmd + K for search focus
      if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
        e.preventDefault();
        const searchInput = document.querySelector('[data-testid="clip-search-input"]') as HTMLInputElement;
        searchInput?.focus();
      }

      // Ctrl/Cmd + F for filter focus
      if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
        e.preventDefault();
        const filterSelect = document.querySelector('[data-testid="priority-filter-select"]') as HTMLSelectElement;
        filterSelect?.focus();
      }

      // Escape to close video modal
      if (e.key === 'Escape' && isVideoModalOpen) {
        handleCloseVideoModal();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isVideoModalOpen, selectedGame]);

  const handleDeleteClip = async (clipFilePath: string) => {
    try {
      if (!selectedGame) {
        throw new Error('No game selected');
      }

      // Call actual backend to delete clip
      await storageApi.deleteClip(selectedGame, clipFilePath);

      // Remove from local state
      setClips(clips.filter(c => c.file_path !== clipFilePath));

      toast({
        title: t('clips.deleteSuccess'),
        description: t('clips.deleteDescription'),
      });
    } catch (error) {
      toast({
        title: t('clips.errors.deleteFailed'),
        description: getErrorMessage(error),
        variant: 'destructive',
      });
    }
  };

  const handlePlayClip = (clip: ClipMetadata) => {
    // `file://` 은 WebView2 에서 로드되지 않는다 — Tauri 는 로컬 파일을 asset
    // protocol 로만 내주고, 그 변환이 `convertFileSrc` 다. 이 화면만 직접
    // `file://` 를 조립하고 있어서 썸네일은 회색 사각형, 재생은 빈 플레이어였다
    // (ClipCard/VideoPreview/TimelineClip 은 모두 convertFileSrc 를 쓴다).
    setCurrentVideoSrc(convertFileSrc(clip.file_path));
    const eventLabel = typeof clip.event_type === 'string' ? clip.event_type : Object.keys(clip.event_type)[0];
    setCurrentVideoTitle(`${eventLabel} ${t('clips.at')} ${formatTime(clip.event_time)}`);
    setIsVideoModalOpen(true);
  };

  const handleCloseVideoModal = () => {
    setIsVideoModalOpen(false);
    setCurrentVideoSrc('');
    setCurrentVideoTitle('');
  };

  const handleEditClip = async (clip: ClipMetadata) => {
    try {
      // Show in folder for editing access
      await utilsApi.showInFolder(clip.file_path);
    } catch (error) {
      logger.error('Failed to show clip for editing:', error);
      toast({
        title: t('clips.errors.editFailed'),
        description: getErrorMessage(error),
        variant: 'destructive',
      });
    }
  };

  const handleDownloadClip = async (clip: ClipMetadata) => {
    try {
      await utilsApi.showInFolder(clip.file_path);
      toast({
        title: t('clips.downloadOpened'),
        description: t('clips.downloadOpenedDescription'),
      });
    } catch (error) {
      logger.error('Failed to open file location:', error);
      toast({
        title: t('clips.errors.downloadFailed'),
        description: getErrorMessage(error),
        variant: 'destructive',
      });
    }
  };

  const getPriorityColor = (priority: number): string => {
    if (priority >= 5) return 'bg-purple-500';
    if (priority >= 4) return 'bg-red-500';
    if (priority >= 3) return 'bg-orange-500';
    return 'bg-blue-500';
  };

  const getPriorityLabel = (priority: number): string => {
    if (priority >= 5) return t('clips.priority.legendary');
    if (priority >= 4) return t('clips.priority.epic');
    if (priority >= 3) return t('clips.priority.rare');
    return t('clips.priority.common');
  };

  const formatTime = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  const formatDate = (isoString: string): string => {
    const date = new Date(isoString);
    return date.toLocaleDateString() + ' ' + date.toLocaleTimeString();
  };

  const filteredClips = clips.filter(clip => {
    // Priority filter
    if (filterPriority !== 'all' && clip.priority.toString() !== filterPriority) {
      return false;
    }

    // Search filter
    const eventLabel = typeof clip.event_type === 'string' ? clip.event_type : Object.keys(clip.event_type)[0];
    if (searchQuery && !eventLabel.toLowerCase().includes(searchQuery.toLowerCase())) {
      return false;
    }

    return true;
  });

  return (
    <div data-testid="clip-library" className="space-y-4">
      {/* Header with Game Selector */}
      <div className="gaming-panel p-6">
        <div className="flex items-center justify-between">
          <div className="mb-4">
            <h3 className="text-lg font-semibold" id="clips-title">{t('clips.title')}</h3>
            <p className="text-sm text-muted-foreground" id="clips-description">
              {t('clips.description')}
            </p>
          </div>
          <Select
            value={selectedGame || ''}
            onValueChange={(value) => setSelectedGame(value)}
            data-testid="game-selector"
            aria-labelledby="clips-title"
            aria-describedby="clips-description"
          >
            <SelectTrigger className="w-[250px]" aria-label={t('clips.selectGame')}>
              <SelectValue placeholder={t('clips.selectGame')} />
            </SelectTrigger>
            <SelectContent>
              {games.map((gameId) => (
                <SelectItem key={gameId} value={gameId}>
                  {gameId}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Filters */}
      <div className="bg-black/40 rounded-lg border border-white/5 p-4">
        <div className="flex gap-4">
          <div className="flex-1 relative">
            <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder={t('clips.searchPlaceholder')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9"
              data-testid="clip-search-input"
              aria-label={t('clips.searchPlaceholder')}
            />
          </div>
          <div className="flex items-center gap-2">
            <Filter className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
            <Select value={filterPriority} onValueChange={setFilterPriority} data-testid="priority-filter-select" aria-label={t('clips.filterPriority')}>
              <SelectTrigger className="w-[150px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t('clips.filter.allPriorities')}</SelectItem>
                <SelectItem value="5">{t('clips.priority.legendary')} (5)</SelectItem>
                <SelectItem value="4">{t('clips.priority.epic')} (4)</SelectItem>
                <SelectItem value="3">{t('clips.priority.rare')} (3)</SelectItem>
                <SelectItem value="2">{t('clips.priority.common')} (2)</SelectItem>
                <SelectItem value="1">{t('clips.priority.basic')} (1)</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
      </div>

      {/* Clips Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4" data-testid="clips-grid" role="region" aria-label={t('clips.clipsGrid')}>
        {isLoading ? (
          <div className="gaming-panel p-6 col-span-full">
            <div className="py-12">
              <div className="flex items-center justify-center">
                <Spinner size="lg" label={t('clips.loading')} />
              </div>
            </div>
          </div>
        ) : filteredClips.length === 0 ? (
          <div className="gaming-panel p-6 col-span-full">
            <div>
              {clips.length === 0 ? (
                <EmptyState
                  icon={Film}
                  title={t('clips.noClipsYet')}
                  description={t('clips.noClipsDescription')}
                  size="md"
                />
              ) : (
                <EmptyState
                  icon={SearchX}
                  title={t('clips.noClipsMatchFilters')}
                  description={t('clips.tryDifferentFilters')}
                  action={{
                    label: t('clips.clearFilters'),
                    onClick: () => {
                      setSearchQuery('');
                      setFilterPriority('all');
                    },
                    variant: 'outline',
                  }}
                  size="md"
                />
              )}
            </div>
          </div>
        ) : (
          filteredClips.map((clip) => (
            <div key={clip.file_path} className="bg-black/40 rounded-lg border border-white/5 overflow-hidden">
              <div className="aspect-video bg-muted relative">
                {/* Video thumbnail or placeholder */}
                {clip.thumbnail_path ? (
                  <img
                    src={convertFileSrc(clip.thumbnail_path)}
                    alt={`${typeof clip.event_type === 'string' ? clip.event_type : Object.keys(clip.event_type)[0]} thumbnail`}
                    className="absolute inset-0 w-full h-full object-cover"
                    onError={(e) => {
                      // Fallback to placeholder if image fails to load
                      e.currentTarget.style.display = 'none';
                      e.currentTarget.nextElementSibling?.classList.remove('hidden');
                    }}
                  />
                ) : null}

                {/* Loading indicator for thumbnail generation */}
                {thumbnailGeneration.has(clip.file_path) ? (
                  <div className="absolute inset-0 flex items-center justify-center bg-black/50">
                    <div className="text-center">
                      <Spinner size="md" className="text-white mx-auto mb-2" />
                      <div className="text-white text-xs">{t('clips.generatingThumbnail')}</div>
                    </div>
                  </div>
                ) : (
                  // Fallback placeholder when no thumbnail
                  <div className={`absolute inset-0 flex items-center justify-center ${clip.thumbnail_path ? 'hidden' : ''}`}>
                    <Play className="h-12 w-12 text-muted-foreground" />
                  </div>
                )}

                {/* Priority Badge */}
                <div className="absolute top-2 right-2">
                  <Badge className={getPriorityColor(clip.priority)}>
                    {getPriorityLabel(clip.priority)}
                  </Badge>
                </div>

                {/* Duration */}
                <div className="absolute bottom-2 right-2 bg-black/70 px-2 py-1 rounded text-xs text-white">
                  {Math.round(clip.duration)}s
                </div>
              </div>

              <div className="p-4">
                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <h3 className="font-semibold">{typeof clip.event_type === 'string' ? clip.event_type : Object.keys(clip.event_type)[0]}</h3>
                    <span className="text-xs text-muted-foreground">
                      @{formatTime(clip.event_time)}
                    </span>
                  </div>

                  <div className="text-xs text-muted-foreground">
                    {formatDate(clip.created_at)}
                  </div>

                  <div className="flex gap-2 pt-2">
                    <Button
                      variant="default"
                      size="sm"
                      className="flex-1"
                      onClick={() => handlePlayClip(clip)}
                      data-testid={`play-clip-${clip.file_path}`}
                      aria-label={`${t('clips.actions.play')} ${typeof clip.event_type === 'string' ? clip.event_type : Object.keys(clip.event_type)[0]} ${t('clips.at')} ${formatTime(clip.event_time)}`}
                    >
                      <Eye className="mr-1 h-3 w-3" />
                      {t('clips.actions.play')}
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => handleEditClip(clip)}
                      data-testid={`edit-clip-${clip.file_path}`}
                      aria-label={`${t('clips.actions.edit')} ${typeof clip.event_type === 'string' ? clip.event_type : Object.keys(clip.event_type)[0]}`}
                    >
                      <Edit className="h-3 w-3" />
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => handleDownloadClip(clip)}
                      data-testid={`download-clip-${clip.file_path}`}
                      aria-label={`${t('clips.actions.download')} ${typeof clip.event_type === 'string' ? clip.event_type : Object.keys(clip.event_type)[0]}`}
                    >
                      <Download className="h-3 w-3" />
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => handleDeleteClip(clip.file_path)}
                      data-testid={`delete-clip-${clip.file_path}`}
                      aria-label={`${t('clips.actions.delete')} ${typeof clip.event_type === 'string' ? clip.event_type : Object.keys(clip.event_type)[0]}`}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Video Modal */}
      <VideoModal
        isOpen={isVideoModalOpen}
        onClose={handleCloseVideoModal}
        src={currentVideoSrc}
        title={currentVideoTitle}
        autoPlay={true}
      />
    </div>
  );
}
