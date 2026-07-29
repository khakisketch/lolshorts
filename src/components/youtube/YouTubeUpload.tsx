import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useYouTube } from '@/hooks/useYouTube';
import { useAutoEditResults } from '@/hooks/useAutoEditResults';
import { useToast } from '@/components/ui/use-toast';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Progress } from '@/components/ui/progress';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Upload,
  Film,
  CheckCircle,
  AlertTriangle,
  RefreshCw,
  WifiOff,
  Clock,
  Calendar,
  X,
  ListOrdered,
  Sparkles,
} from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { PrivacyStatus } from '@/types/youtube';
import { UploadMetadata } from '@/hooks/useYouTube';
import { youtubeApi } from '@/api/youtube';
import { logger } from '@/lib/logger';
import { validateYoutubeUpload, getFieldError, type ValidationResult } from '@/lib/validation';


type UploadErrorType = 'quota' | 'network' | 'auth_expired' | 'generic' | null;

interface YouTubeUploadProps {
  initialPath?: string;
  resultId?: string;
}

function classifyUploadError(error: unknown): UploadErrorType {
  const msg = String(error).toLowerCase();
  if (msg.includes('quota') || msg.includes('ratelimitexceeded') || msg.includes('daily limit')) {
    return 'quota';
  }
  if (
    msg.includes('auth') ||
    msg.includes('token') ||
    msg.includes('expired') ||
    msg.includes('unauthorized') ||
    msg.includes('401')
  ) {
    return 'auth_expired';
  }
  if (
    msg.includes('network') ||
    msg.includes('connection') ||
    msg.includes('timeout') ||
    msg.includes('fetch') ||
    msg.includes('econnrefused')
  ) {
    return 'network';
  }
  return 'generic';
}

export function YouTubeUpload({ initialPath, resultId }: YouTubeUploadProps = {}) {
  const { t } = useTranslation();
  const { getResult, updateYouTubeStatus: updateResultStatus } = useAutoEditResults();
  const { toast } = useToast();
  const {
    authStatus,
    isLoading,
    error,
    queueError,
    uploadProgress,
    uploadVideo,
    startProgressPolling,
    stopProgressPolling,
    uploadQueue,
    loadQueue,
  } = useYouTube();

  const [videoPath, setVideoPath] = useState('');
  const [thumbnailPath, setThumbnailPath] = useState('');
  const [metadata, setMetadata] = useState<UploadMetadata>({
    title: '',
    description: '',
    tags: [],
    privacy_status: 'unlisted' as PrivacyStatus,
  });

  useEffect(() => {
    const initializeUpload = async () => {
      if (resultId) {
        try {
          const result = await getResult(resultId);
          setVideoPath(result.output_path);
          setThumbnailPath(result.thumbnail_path || '');
          
          const filename = result.output_path.split(/[\\/]/).pop()?.replace(/\.[^/.]+$/, '') || '';
          setMetadata((prev) => ({ ...prev, title: filename }));
          return; // Success, no need to check initialPath
        } catch (err) {
          logger.error('Failed to fetch result metadata, falling back to initialPath:', err);
        }
      }
      
      if (initialPath) {
        setVideoPath(initialPath);
        const filename = initialPath.split(/[\\/]/).pop()?.replace(/\.[^/.]+$/, '') || '';
        setMetadata((prev) => ({ ...prev, title: filename }));
      }
    };

    initializeUpload();
  }, [initialPath, resultId, getResult]);

  const [tagsInput, setTagsInput] = useState('');
  const [uploadSuccess, setUploadSuccess] = useState(false);
  const [uploadErrorType, setUploadErrorType] = useState<UploadErrorType>(null);
  const [lastUploadArgs, setLastUploadArgs] = useState<{
    path: string;
    meta: UploadMetadata;
    thumb?: string;
  } | null>(null);

  const [scheduleMode, setScheduleMode] = useState<'now' | 'schedule'>('now');
  const [scheduledAt, setScheduledAt] = useState('');
  const [scheduleSuccess, setScheduleSuccess] = useState(false);
  const [scheduleError, setScheduleError] = useState<string | null>(null);
  const [queueLoading, setQueueLoading] = useState(false);
  const [validationResult, setValidationResult] = useState<ValidationResult | null>(null);


  const handleSelectVideo = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Video', extensions: ['mp4', 'mov', 'avi', 'mkv', 'webm'] }],
      });


      if (selected && typeof selected === 'string') {
        setVideoPath(selected);
        if (!metadata.title) {
          const filename = selected.split(/[\\/]/).pop()?.replace(/\.[^/.]+$/, '') || '';
          setMetadata((prev) => ({ ...prev, title: filename }));
        }
      }
    } catch (err) {
      logger.error('File selection error:', err);
      toast({
        title: t('toast.error'),
        description: t('errors.fileSelectionFailed'),
        variant: 'destructive',
      });
    }
  };

  const handleSelectThumbnail = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Image', extensions: ['jpg', 'jpeg', 'png'] }],
      });

      if (selected && typeof selected === 'string') {
        setThumbnailPath(selected);
      }
    } catch (err) {
      logger.error('File selection error:', err);
      toast({
        title: t('toast.error'),
        description: t('errors.fileSelectionFailed'),
        variant: 'destructive',
      });
    }
  };

  const executeUpload = async (path: string, meta: UploadMetadata, thumb?: string) => {
    setUploadSuccess(false);
    setUploadErrorType(null);
    try {
      if (resultId) {
        try {
          await updateResultStatus(resultId, {
            video_id: null,
            status: 'Uploading',
            upload_started_at: new Date().toISOString(),
            upload_completed_at: null,
            progress: 0,
            error: null,
          });
        } catch (err) {
          logger.error('Failed to set result status to Uploading:', err);
        }
      }

      startProgressPolling();
      const video = await uploadVideo(path, meta, thumb);
      
      if (resultId) {
        try {
          await updateResultStatus(resultId, {
            video_id: video.id,
            status: 'Completed',
            upload_started_at: new Date().toISOString(), // Approximation
            upload_completed_at: new Date().toISOString(),
            progress: 100,
            error: null,
          });
        } catch (err) {
          logger.error('Failed to set result status to Completed:', err);
        }
      }

      setUploadSuccess(true);
      stopProgressPolling();
      setLastUploadArgs(null);
      setVideoPath('');
      setThumbnailPath('');
      setMetadata({ title: '', description: '', tags: [], privacy_status: 'unlisted' });
      setTagsInput('');
    } catch (err) {
      logger.error('Upload error:', err);
      stopProgressPolling();
      
      if (resultId) {
        try {
          await updateResultStatus(resultId, {
            video_id: null,
            status: 'Failed',
            upload_started_at: new Date().toISOString(), // Approximation
            upload_completed_at: null,
            progress: 0,
            error: String(err),
          });
        } catch (statusErr) {
          logger.error('Failed to set result status to Failed:', statusErr);
        }
      }

      setUploadErrorType(classifyUploadError(err));
      setLastUploadArgs({ path, meta, thumb });
    }
  };

  const handleUpload = async () => {
    if (!videoPath || !metadata.title) return;

    const result = validateYoutubeUpload({
      title: metadata.title,
      description: metadata.description,
      tags: tagsInput,
      privacyStatus: metadata.privacy_status as 'public' | 'unlisted' | 'private',
    });
    if (!result.valid) {
      setValidationResult(result);
      return;
    }
    setValidationResult(null);

    const tags = tagsInput
      .split(',')
      .map((tag) => tag.trim())
      .filter((tag) => tag.length > 0);

    const uploadMetadata: UploadMetadata = { ...metadata, tags };
    await executeUpload(videoPath, uploadMetadata, thumbnailPath || undefined);
  };

  const handleSchedule = async () => {
    if (!videoPath || !metadata.title) return;
    
    const tags = tagsInput
      .split(',')
      .map((tag) => tag.trim())
      .filter((tag) => tag.length > 0);
    
    setScheduleSuccess(false);
    setScheduleError(null);
    try {
      // Convert local datetime-local value to ISO 8601
      const isoScheduledAt = scheduledAt ? new Date(scheduledAt).toISOString() : undefined;
      await youtubeApi.scheduleUpload(
        videoPath,
        metadata.title,
        metadata.description,
        tags,
        metadata.privacy_status,
        thumbnailPath || undefined,
        isoScheduledAt,
      );

      if (resultId) {
        try {
          await updateResultStatus(resultId, {
            video_id: null,
            status: 'Queued',
            upload_started_at: null,
            upload_completed_at: null,
            progress: 0,
            error: null,
          });
        } catch (err) {
          logger.error('Failed to set result status to Queued:', err);
        }
      }

      setScheduleSuccess(true);
      setVideoPath('');
      setThumbnailPath('');
      setMetadata({ title: '', description: '', tags: [], privacy_status: 'unlisted' });
      setTagsInput('');
      setScheduledAt('');
      setScheduleMode('now');
      await loadQueue();
    } catch (err) {
      logger.error('Schedule error:', err);
      setScheduleError(String(err));

      if (resultId) {
        try {
          await updateResultStatus(resultId, {
            video_id: null,
            status: 'Failed',
            upload_started_at: null,
            upload_completed_at: null,
            progress: 0,
            error: String(err),
          });
        } catch (statusErr) {
          logger.error('Failed to set result status to Failed after schedule error:', statusErr);
        }
      }
    }
  };


  const handleCancelScheduled = async (id: string) => {
    setQueueLoading(true);
    try {
      await youtubeApi.cancelScheduledUpload(id);
      await loadQueue();
    } catch (err) {
      logger.error('Cancel scheduled upload error:', err);
    } finally {
      setQueueLoading(false);
    }
  };


  const handleRetry = async () => {
    if (!lastUploadArgs) return;
    await executeUpload(lastUploadArgs.path, lastUploadArgs.meta, lastUploadArgs.thumb);
  };

  const getUploadPercentage = () => {
    if (!uploadProgress) return 0;
    return Math.round(uploadProgress.percentage);
  };

  const getStatusBadge = (status: string | undefined) => {
    const s = status || 'pending';
    switch (s) {
      case 'completed':
        return <Badge className="bg-green-500 text-white hover:bg-green-600">{t('youtube.schedule.status.completed', 'Completed')}</Badge>;
      case 'failed':
        return <Badge variant="destructive">{t('youtube.schedule.status.failed', 'Failed')}</Badge>;
      case 'running':
        return <Badge variant="secondary" className="animate-pulse">{t('youtube.schedule.status.running', 'Running')}</Badge>;
      case 'cancelled':
        return <Badge variant="outline" className="text-muted-foreground">{t('youtube.schedule.status.cancelled', 'Cancelled')}</Badge>;
      default:
        return <Badge variant="secondary">{t('youtube.schedule.status.pending', 'Pending')}</Badge>;
    }
  };

  if (!authStatus.authenticated) {
    return (
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">{t('youtube.upload.uploadVideo')}</h3>
          <p className="text-sm text-muted-foreground">{t('youtube.upload.connectAccountFirst')}</p>
        </div>
        <Alert>
          <AlertDescription>{t('youtube.upload.connectRequired')}</AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className="gaming-panel p-6 space-y-6">
      <div>
        <div className="flex items-center gap-3">
          <Film className="h-6 w-6" />
          <div>
            <h3 className="text-lg font-semibold">{t('youtube.upload.uploadToYouTube')}</h3>
            <p className="text-sm text-muted-foreground">{t('youtube.upload.uploadDescription')}</p>
          </div>
        </div>
        {resultId && (
          <div className="mt-3 flex items-center gap-2">
            <Badge variant="secondary" className="bg-blue-500/10 text-blue-400 border-blue-500/20">
              <Sparkles className="w-3 h-3 mr-1" />
              {t('youtube.upload.resultCentered', 'Auto-Edit Result Upload')}
            </Badge>
          </div>
        )}
      </div>

      <div className="space-y-6">
        {/* Quota exceeded error */}
        {uploadErrorType === 'quota' && (
          <Alert variant="destructive">
            <Clock className="h-4 w-4" />
            <AlertTitle>{t('errors.youtubeQuotaExceeded')}</AlertTitle>
            <AlertDescription>{t('errors.youtubeQuotaExceededHint')}</AlertDescription>
          </Alert>
        )}

        {/* Auth expired error */}
        {uploadErrorType === 'auth_expired' && (
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>{t('errors.youtubeAuthExpired')}</AlertTitle>
            <AlertDescription className="space-y-2">
              <p>{t('errors.youtubeAuthExpiredHint')}</p>
            </AlertDescription>
          </Alert>
        )}

        {/* Network error with retry */}
        {uploadErrorType === 'network' && (
          <Alert variant="destructive">
            <WifiOff className="h-4 w-4" />
            <AlertTitle>{t('errors.youtubeNetworkError')}</AlertTitle>
            <AlertDescription className="space-y-3">
              <p>{t('errors.youtubeNetworkErrorHint')}</p>
              <Button
                size="sm"
                variant="outline"
                onClick={handleRetry}
                disabled={isLoading}
                className="mt-2"
              >
                <RefreshCw className="h-3 w-3 mr-1" />
                {t('errorBoundary.retry')}
              </Button>
            </AlertDescription>
          </Alert>
        )}

        {/* Generic upload error with retry */}
        {uploadErrorType === 'generic' && error && (
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>{t('errors.youtubeUploadFailed')}</AlertTitle>
            <AlertDescription className="space-y-3">
              <p>{error}</p>
              <Button
                size="sm"
                variant="outline"
                onClick={handleRetry}
                disabled={isLoading}
                className="mt-2"
              >
                <RefreshCw className="h-3 w-3 mr-1" />
                {t('errorBoundary.retry')}
              </Button>
            </AlertDescription>
          </Alert>
        )}

        {uploadSuccess && (
          <Alert className="bg-green-50 text-green-900 border-green-200">
            <CheckCircle className="h-4 w-4" />
            <AlertDescription>{t('youtube.upload.uploadSuccessful')}</AlertDescription>
          </Alert>
        )}

        {scheduleSuccess && (
          <Alert className="bg-blue-50 text-blue-900 border-blue-200">
            <CheckCircle className="h-4 w-4" />
            <AlertDescription>{t('youtube.schedule.scheduleSuccessful')}</AlertDescription>
          </Alert>
        )}

        {scheduleError && (
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>{t('youtube.schedule.scheduleFailed')}</AlertTitle>
            <AlertDescription>{scheduleError}</AlertDescription>
          </Alert>
        )}

        {queueError && (
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>{t('youtube.schedule.queueError', 'Upload queue unavailable')}</AlertTitle>
            <AlertDescription>{queueError}</AlertDescription>
          </Alert>
        )}

        {uploadProgress && uploadProgress.status !== 'complete' && (
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="font-medium">{t('youtube.upload.uploadProgress')}</span>
              <Badge variant="secondary">{uploadProgress.status}</Badge>
            </div>
            <Progress value={getUploadPercentage()} />
            <p className="text-xs text-muted-foreground text-center">
              {getUploadPercentage()}% ({uploadProgress.bytes_uploaded.toLocaleString()} /{' '}
              {uploadProgress.total_bytes.toLocaleString()} bytes)
            </p>
          </div>
        )}

        <div className="space-y-4">
          {/* Video File Selection */}
          <div className="space-y-2">
            <Label>{t('youtube.upload.videoFile')}</Label>
            <div className="flex gap-2">
              <Input
                value={videoPath}
                placeholder={t('youtube.upload.selectVideoFile')}
                readOnly
                className="flex-1"
              />
              <Button variant="outline" onClick={handleSelectVideo}>
                {t('youtube.upload.browse')}
              </Button>
            </div>
          </div>

          {/* Thumbnail Selection (Optional) */}
          <div className="space-y-2">
            <Label>{t('youtube.upload.customThumbnail')}</Label>
            <div className="flex gap-2">
              <Input
                value={thumbnailPath}
                placeholder={t('youtube.upload.selectThumbnailImage')}
                readOnly
                className="flex-1"
              />
              <Button variant="outline" onClick={handleSelectThumbnail}>
                {t('youtube.upload.browse')}
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">{t('youtube.upload.thumbnailRecommendation')}</p>
          </div>

          {/* Title */}
          <div className="space-y-2">
            <Label>{t('youtube.upload.title')}</Label>
            <Input
              value={metadata.title}
              onChange={(e) => setMetadata((prev) => ({ ...prev, title: e.target.value }))}
              placeholder={t('youtube.upload.titlePlaceholder')}
              maxLength={100}
            />
            {validationResult && getFieldError(validationResult, 'title') && (
              <p className="text-xs text-destructive">{getFieldError(validationResult, 'title')}</p>
            )}
            <p className="text-xs text-muted-foreground">
              {t('youtube.upload.charactersCount', { count: metadata.title.length, max: 100 })}
            </p>
          </div>

          {/* Description */}
          <div className="space-y-2">
            <Label>{t('youtube.upload.description')}</Label>
            <textarea
              value={metadata.description}
              onChange={(e) => setMetadata((prev) => ({ ...prev, description: e.target.value }))}
              placeholder={t('youtube.upload.descriptionPlaceholder')}
              className="w-full min-h-[100px] px-3 py-2 text-sm border rounded-md"
              maxLength={5000}
            />
            {validationResult && getFieldError(validationResult, 'description') && (
              <p className="text-xs text-destructive">{getFieldError(validationResult, 'description')}</p>
            )}
            <p className="text-xs text-muted-foreground">
              {t('youtube.upload.charactersCount', { count: metadata.description.length, max: 5000 })}
            </p>
          </div>

          {/* Tags */}
          <div className="space-y-2">
            <Label>{t('youtube.upload.tags')}</Label>
            <Input
              value={tagsInput}
              onChange={(e) => setTagsInput(e.target.value)}
              placeholder={t('youtube.upload.tagsPlaceholder')}
            />
          </div>

          {/* Privacy Status */}
          <div className="space-y-2">
            <Label>{t('youtube.upload.privacy')}</Label>
            <Select
              value={metadata.privacy_status}
              onValueChange={(value: PrivacyStatus) =>
                setMetadata((prev) => ({ ...prev, privacy_status: value }))
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="public">{t('youtube.upload.privacyOptions.public')}</SelectItem>
                <SelectItem value="unlisted">
                  {t('youtube.upload.privacyOptions.unlisted')}
                </SelectItem>
                <SelectItem value="private">{t('youtube.upload.privacyOptions.private')}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* Schedule datetime picker (only shown in schedule mode) */}
          {scheduleMode === 'schedule' && (
            <div className="space-y-2">
              <Label>
                <Calendar className="h-4 w-4 inline mr-1" />
                {t('youtube.schedule.publishAt')}
              </Label>
              <Input
                type="datetime-local"
                value={scheduledAt}
                onChange={(e) => setScheduledAt(e.target.value)}
                min={new Date().toISOString().slice(0, 16)}
              />
              <p className="text-xs text-muted-foreground">{t('youtube.schedule.publishAtHint')}</p>
              <Alert className="bg-blue-50 text-blue-900 border-blue-200">
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription className="text-xs">
                  {t(
                    'youtube.schedule.appMustBeRunningNotice',
                    'This is not a native YouTube schedule - LoLShorts must be running at the scheduled time for the upload to start.',
                  )}
                </AlertDescription>
              </Alert>
            </div>
          )}

          {/* Upload Now / Schedule buttons */}
          <div className="flex gap-2">
            <Button
              onClick={() => {
                setScheduleMode('now');
                handleUpload();
              }}
              disabled={!videoPath || !metadata.title || isLoading || scheduleMode === 'schedule'}
              className="flex-1"
              variant={scheduleMode === 'now' ? 'default' : 'outline'}
              type="button"
            >
              <Upload className="h-4 w-4 mr-2" />
              {isLoading && scheduleMode === 'now'
                ? t('youtube.upload.uploading')
                : t('youtube.upload.uploadButton')}
            </Button>

            <Button
              onClick={() => {
                if (scheduleMode !== 'schedule') {
                  setScheduleMode('schedule');
                } else {
                  handleSchedule();
                }
              }}
              disabled={!videoPath || !metadata.title || isLoading}
              className="flex-1"
              variant={scheduleMode === 'schedule' ? 'default' : 'outline'}
              type="button"
            >
              <Calendar className="h-4 w-4 mr-2" />
              {scheduleMode === 'schedule'
                ? t('youtube.schedule.confirmSchedule')
                : t('youtube.schedule.scheduleButton')}
            </Button>
          </div>

          <p className="text-xs text-muted-foreground">{t('youtube.upload.requiredFields')}</p>
        </div>
      </div>

       {/* Upload Queue */}
       {uploadQueue && uploadQueue.length > 0 && (
         <div className="space-y-3 border-t pt-4">

          <div className="flex items-center gap-2">
            <ListOrdered className="h-5 w-5" />
            <h4 className="font-semibold text-sm">
              {t('youtube.schedule.queueTitle')} ({uploadQueue.length})
            </h4>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              'youtube.schedule.appMustBeRunningNotice',
              'This is not a native YouTube schedule - LoLShorts must be running at the scheduled time for the upload to start.',
            )}
          </p>
          <div className="space-y-2">
            {uploadQueue.map((item) => (
              <div
                key={item.id}
                className="flex items-center justify-between p-3 border rounded-md text-sm"
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1">
                    <p className="font-medium truncate">{item.title}</p>
                    {getStatusBadge(item.status)}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {item.schedule?.scheduled_at
                      ? `${t('youtube.schedule.scheduledFor')} ${new Date(item.schedule.scheduled_at).toLocaleString()}`
                      : t('youtube.schedule.queued')}
                  </p>
                  {item.error && (
                    <p className="text-xs text-destructive mt-1 flex items-center gap-1">
                      <AlertTriangle className="h-3 w-3" />
                      {item.error}
                    </p>
                  )}
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => handleCancelScheduled(item.id)}
                  disabled={queueLoading}
                  aria-label={t('youtube.schedule.cancelScheduled')}
                >
                  <X className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
