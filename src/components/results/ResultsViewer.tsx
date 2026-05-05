import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from '@tanstack/react-router';
import { useAutoEditResults } from '@/hooks/useAutoEditResults';
import { convertFileSrc } from '@tauri-apps/api/core';
import { formatDuration, formatStorage } from '@/lib/utils';
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
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useConfirmDialog } from '@/components/ui/confirm-dialog';
import { Spinner, SpinnerCenter } from '@/components/ui/spinner';
import { EmptyState } from '@/components/ui/empty-state';
import {
  Clock,
  Trash2,
  Play,
  Upload,
  CheckCircle2,
  XCircle,
  Loader2,
  AlertCircle,
  Film,
  Calendar,
  Sparkles,
  Search,
  Filter,
  Folder,
} from 'lucide-react';
import { AutoEditResultMetadata } from '@/types/autoEdit';
import { logger } from '@/lib/logger';
import { utilsApi } from '@/api/utils';

export function ResultsViewer() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [results, setResults] = useState<AutoEditResultMetadata[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [filterStatus, setFilterStatus] = useState<string>('all');
  const { getAllResults, deleteResult, isLoading, error } = useAutoEditResults();
  const { confirm, ConfirmDialog } = useConfirmDialog();

  const loadResults = useCallback(async () => {
    try {
      const fetchedResults = await getAllResults();
      setResults(fetchedResults);
    } catch (err) {
      logger.error('Failed to load results:', err);
    }
  }, [getAllResults]);

  const filteredResults = results.filter(result => {
    const searchLower = searchQuery.toLowerCase();
    const matchesSearch = 
      result.result_id.toLowerCase().includes(searchLower) ||
      result.job_id.toLowerCase().includes(searchLower);

    if (!matchesSearch) return false;

    if (filterStatus !== 'all') {
      const status = result.youtube_status?.status || 'NotUploaded';
      if (status !== filterStatus) return false;
    }

    return true;
  });

  // Load results on mount
  useEffect(() => {
    loadResults();
  }, [loadResults]);

  const handleDelete = async (resultId: string) => {
    const confirmed = await confirm({
      title: t('results.deleteConfirmTitle'),
      description: t('results.deleteConfirmDescription'),
      confirmText: t('common.delete'),
      cancelText: t('common.cancel'),
      variant: 'danger',
    });

    if (!confirmed) {
      return;
    }

    try {
      await deleteResult(resultId, true);
      setResults(results.filter(r => r.result_id !== resultId));
    } catch (err) {
      logger.error('Failed to delete result:', err);
    }
  };

  const handlePlay = (outputPath: string) => {
    const videoUrl = convertFileSrc(outputPath);
    window.open(videoUrl, '_blank');
  };

  const handleOpenFile = async (outputPath: string) => {
    try {
      await utilsApi.openFileWithDefaultApp(outputPath);
    } catch (err) {
      logger.error('Failed to open file:', err);
    }
  };

  const handleShowInFolder = async (outputPath: string) => {
    try {
      await utilsApi.showInFolder(outputPath);
    } catch (err) {
      logger.error('Failed to show in folder:', err);
    }
  };

  const handlePublish = (result: AutoEditResultMetadata) => {
    navigate({
      to: '/youtube',
      search: {
        path: result.output_path,
        resultId: result.result_id,
      },
    });
  };

  const formatDate = (dateString: string): string => {
    return new Date(dateString).toLocaleDateString();
  };

  const getUploadStatusBadge = (result: AutoEditResultMetadata) => {
    if (!result.youtube_status) {
      return <Badge variant="secondary">{t('results.notUploaded')}</Badge>;
    }

    const { status, progress, error } = result.youtube_status;

    switch (status) {
      case 'NotUploaded':
        return <Badge variant="secondary">{t('results.notUploaded')}</Badge>;
      case 'Queued':
        return <Badge variant="outline"><Loader2 className="w-3 h-3 mr-1 animate-spin" />{t('results.queued')}</Badge>;
      case 'Uploading':
        return <Badge variant="outline"><Upload className="w-3 h-3 mr-1" />{t('results.uploading')} {progress}%</Badge>;
      case 'Processing':
        return <Badge variant="outline"><Loader2 className="w-3 h-3 mr-1 animate-spin" />{t('results.processing')}</Badge>;
      case 'Completed':
        return <Badge variant="default"><CheckCircle2 className="w-3 h-3 mr-1" />{t('results.completed')}</Badge>;
      case 'Failed':
        return (
          <Badge variant="destructive" title={error || t('results.uploadError')}>
            <XCircle className="w-3 h-3 mr-1" />{t('results.failed')}
          </Badge>
        );
      default:
        return <Badge variant="secondary">{t('results.unknown')}</Badge>;
    }
  };

  return (
    <div className="flex flex-col h-full p-6 space-y-6 overflow-auto">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">{t('results.title')}</h1>
          <p className="text-muted-foreground">{t('results.description')}</p>
        </div>
        <Button onClick={loadResults} disabled={isLoading}>
          {isLoading ? <Spinner size="sm" className="mr-2" /> : <Film className="w-4 h-4 mr-2" />}
          {t('results.refresh')}
        </Button>
      </div>

      {/* Error Alert */}
      {error && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {/* Filters */}
      <div className="bg-black/40 rounded-lg border border-white/5 p-4 mb-6">
        <div className="flex gap-4">
          <div className="flex-1 relative">
            <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder={t('results.searchPlaceholder', 'Search by result or job ID...')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9"
            />
          </div>
          <div className="flex items-center gap-2">
            <Filter className="h-4 w-4 text-muted-foreground" />
            <Select value={filterStatus} onValueChange={setFilterStatus}>
              <SelectTrigger className="w-[180px]">
                <SelectValue placeholder={t('results.filterStatusPlaceholder', 'All Statuses')} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t('results.filter.allStatuses', 'All Statuses')}</SelectItem>
                <SelectItem value="Completed">{t('results.completed')}</SelectItem>
                <SelectItem value="Queued">{t('results.queued')}</SelectItem>
                <SelectItem value="Uploading">{t('results.uploading')}</SelectItem>
                <SelectItem value="Processing">{t('results.processing')}</SelectItem>
                <SelectItem value="Failed">{t('results.failed')}</SelectItem>
                <SelectItem value="NotUploaded">{t('results.notUploaded')}</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
      </div>

      {/* Loading State */}
      {isLoading && results.length === 0 && (
        <SpinnerCenter size="lg" label={t('results.loading')} />
      )}

      {/* Empty State */}
      {!isLoading && results.length === 0 && (
        <div className="gaming-panel p-6">
          <div>
            <EmptyState
              icon={Sparkles}
              title={t('results.empty.title')}
              description={t('results.empty.description')}
              action={{
                label: t('results.goToAutoEdit'),
                onClick: () => navigate({ to: '/auto-edit' }),
              }}
              size="lg"
            />
          </div>
        </div>
      )}

      {!isLoading && results.length > 0 && filteredResults.length === 0 && (
        <div className="gaming-panel p-6">
          <EmptyState
            icon={Search}
            title={t('results.noResultsMatchFilters', 'No results match your filters')}
            description={t('results.tryDifferentFilters', 'Try adjusting your search or filter criteria.')}
            action={{
              label: t('results.clearFilters', 'Clear Filters'),
              onClick: () => {
                setSearchQuery('');
                setFilterStatus('all');
              },
            }}
            size="lg"
          />
        </div>
      )}

      {/* Results Grid */}
      {results.length > 0 && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredResults.map((result) => (
            <div key={result.result_id} className="bg-black/40 rounded-lg border border-white/5 flex flex-col overflow-hidden">
              <div className="aspect-video bg-muted relative">
                {result.thumbnail_path ? (
                  <img
                    src={convertFileSrc(result.thumbnail_path)}
                    alt="Result thumbnail"
                    className="absolute inset-0 w-full h-full object-cover"
                    onError={(e) => {
                      e.currentTarget.style.display = 'none';
                      e.currentTarget.nextElementSibling?.classList.remove('hidden');
                    }}
                  />
                ) : null}
                <div className={`absolute inset-0 flex items-center justify-center ${result.thumbnail_path ? 'hidden' : ''}`}>
                  <Film className="h-12 w-12 text-muted-foreground" />
                </div>
                <div className="absolute top-2 right-2">
                  {getUploadStatusBadge(result)}
                </div>
              </div>
              <div className="p-4 mb-2">
                <div className="flex items-start justify-between">
                  <div className="flex-1">
                    <h3 className="text-lg font-semibold flex items-center gap-2">
                      <Film className="w-5 h-5" />
                      {result.target_duration}s {t('results.short')}
                    </h3>
                    <div className="flex items-center gap-2 mt-1">
                      <p className="text-sm text-muted-foreground flex items-center gap-1">
                        <Calendar className="w-3 h-3" />
                        {formatDate(result.created_at)}
                      </p>
                      <Badge variant="outline" className="text-[10px] uppercase tracking-wider opacity-70">
                        AutoEdit Result
                      </Badge>
                    </div>
                  </div>
                </div>
              </div>

              <div className="px-4 pb-4 flex-1 flex flex-col justify-between">
                {/* Video Info */}
                <div className="space-y-2 mb-4">
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">{t('results.duration')}:</span>
                    <span className="font-medium flex items-center gap-1">
                      <Clock className="w-3 h-3" />
                      {formatDuration(result.duration)}
                    </span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">{t('results.clips')}:</span>
                    <span className="font-medium">{result.clip_count}</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">{t('results.fileSize')}:</span>
                    <span className="font-medium">{formatStorage(result.file_size_bytes)}</span>
                  </div>
                  {result.canvas_template_name && (
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground">{t('results.template')}:</span>
                      <span className="font-medium">{result.canvas_template_name}</span>
                    </div>
                  )}
                  {result.has_background_music && (
                    <Badge variant="outline" className="w-fit">
                      {t('results.withMusic')}
                    </Badge>
                  )}
                </div>

                {/* Action Buttons */}
                <div className="flex flex-col gap-2">
                  <div className="flex gap-2">
                    <Button
                      onClick={() => handlePublish(result)}
                      className="flex-1"
                      variant="default"
                    >
                      <Upload className="w-4 h-4 mr-2" />
                      {t('results.publish', 'Publish')}
                    </Button>
                    <Button
                      onClick={() => handleDelete(result.result_id)}
                      variant="destructive"
                      size="icon"
                    >
                      <Trash2 className="w-4 h-4" />
                    </Button>
                  </div>
                  <div className="flex gap-2">
                    <Button
                      onClick={() => handlePlay(result.output_path)}
                      variant="outline"
                      size="sm"
                      className="flex-1"
                    >
                      <Play className="w-3 h-3 mr-2" />
                      {t('results.play')}
                    </Button>
                    <Button
                      onClick={() => handleOpenFile(result.output_path)}
                      variant="outline"
                      size="sm"
                      className="flex-1"
                    >
                      <Film className="w-3 h-3 mr-2" />
                      {t('results.openFile', 'Open File')}
                    </Button>
                    <Button
                      onClick={() => handleShowInFolder(result.output_path)}
                      variant="outline"
                      size="sm"
                      className="flex-1"
                    >
                      <Folder className="w-3 h-3 mr-2" />
                      {t('results.showInFolder', 'Folder')}
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      <ConfirmDialog />
    </div>
  );
}
