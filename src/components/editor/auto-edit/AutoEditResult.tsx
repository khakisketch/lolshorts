import { useTranslation } from 'react-i18next';
import { useNavigate } from '@tanstack/react-router';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { CheckCircle2, Download, Play, RefreshCw, Upload, LayoutDashboard, AlertCircle, ShieldCheck } from 'lucide-react';
import { formatStorage } from '@/lib/utils';
import { utilsApi } from '@/api/utils';
import { logger } from '@/lib/logger';
import { AutoEditResult as AutoEditResultType, ShortsReadiness } from '@/types/autoEdit';
import { useAutoEditStore } from '@/stores/autoEditStore';

interface AutoEditResultProps {
  result: AutoEditResultType;
  onStartNew: () => void;
  onRegenerate: () => void;
}

function ReadinessCheck({ label, passed, message }: { label: string; passed: boolean; message: string }) {
  return (
    <div className="flex items-center justify-between text-xs py-1">
      <div className="flex items-center gap-2">
        {passed ? (
          <CheckCircle2 className="w-3 h-3 text-green-500" />
        ) : (
          <AlertCircle className="w-3 h-3 text-yellow-500" />
        )}
        <span className={passed ? 'text-foreground' : 'text-muted-foreground'}>{label}</span>
      </div>
      <span className={`text-[10px] ${passed ? 'text-green-600' : 'text-yellow-600'}`}>{message}</span>
    </div>
  );
}

export function AutoEditResult({ result, onStartNew, onRegenerate }: AutoEditResultProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { 
    currentTemplate, 
    backgroundMusic, 
  } = useAutoEditStore();

  const computeReadiness = (): ShortsReadiness => {
    const durationPassed = result.duration >= 15 && result.duration <= 180;
    const clipsPassed = result.clips_used >= 3;
    const templatePassed = !!currentTemplate;
    const musicPassed = !!backgroundMusic;

    return {
      isReady: durationPassed && clipsPassed && templatePassed && musicPassed,
      checks: {
        duration: {
          label: t('autoEdit.readiness.duration', 'Duration'),
          passed: durationPassed,
          message: durationPassed ? t('autoEdit.readiness.durationOk', 'Optimal') : t('autoEdit.readiness.durationWarn', 'Needs review'),
        },
        clipCount: {
          label: t('autoEdit.readiness.clips', 'Clip Count'),
          passed: clipsPassed,
          message: clipsPassed ? t('autoEdit.readiness.clipsOk', 'Sufficient') : t('autoEdit.readiness.clipsWarn', 'Needs review'),
        },
        template: {
          label: t('autoEdit.readiness.template', 'Canvas Template'),
          passed: templatePassed,
          message: templatePassed ? t('autoEdit.readiness.templateOk', 'Applied') : t('autoEdit.readiness.templateWarn', 'None'),
        },
        music: {
          label: t('autoEdit.readiness.music', 'Background Music'),
          passed: musicPassed,
          message: musicPassed ? t('autoEdit.readiness.musicOk', 'Added') : t('autoEdit.readiness.musicWarn', 'None'),
        },
      },
    };
  };

  const readiness = computeReadiness();

  const handleOpenLocation = async () => {
    try {
      await utilsApi.showInFolder(result.output_path);
    } catch (error) {
      logger.error('Failed to show in folder:', error);
      alert(t('errors.failedToOpenFileLocation', { error: String(error) }));
    }
  };

  const handlePlayVideo = async () => {
    try {
      await utilsApi.openFileWithDefaultApp(result.output_path);
    } catch (error) {
      logger.error('Failed to open video:', error);
      alert(t('errors.failedToPlayVideo', { error: String(error) }));
    }
  };

  return (
    <div className="max-w-2xl mx-auto space-y-6" data-testid="result-section">
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold flex items-center gap-2 text-green-600">
            <CheckCircle2 className="w-6 h-6" />
            {t('autoEdit.shortGeneratedSuccessfully')}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t('autoEdit.shortReady')}
          </p>
        </div>
        <div className="space-y-6">
          {/* Result Details */}
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1">
              <div className="text-sm text-muted-foreground">{t('autoEdit.duration')}</div>
              <div className="text-2xl font-bold">{Math.round(result.duration)}s</div>
            </div>
            <div className="space-y-1">
              <div className="text-sm text-muted-foreground">{t('autoEdit.clipsUsed')}</div>
              <div className="text-2xl font-bold">{result.clips_used}</div>
            </div>
            <div className="space-y-1">
              <div className="text-sm text-muted-foreground">{t('autoEdit.fileSize')}</div>
              <div className="text-2xl font-bold">{formatStorage(result.file_size_bytes)}</div>
            </div>
            <div className="space-y-1">
              <div className="text-sm text-muted-foreground">{t('autoEdit.jobId')}</div>
              <div className="text-xs font-mono truncate">{result.job_id}</div>
            </div>
          </div>

          <Separator />

          {/* Shorts Readiness */}
          <div className="p-4 bg-black/40 rounded-lg border border-white/5">
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2 font-medium">
                <ShieldCheck className={`w-4 h-4 ${readiness.isReady ? 'text-green-500' : 'text-yellow-500'}`} />
                {t('autoEdit.shortsReadiness', 'Shorts Planning Check')}
              </div>
              <Badge variant={readiness.isReady ? 'default' : 'secondary'} className={readiness.isReady ? 'bg-green-600' : ''}>
                {readiness.isReady ? t('autoEdit.readiness.ready', 'Looks Good') : t('autoEdit.readiness.notReady', 'Needs Review')}
              </Badge>
            </div>
            <div className="space-y-1">
              <ReadinessCheck {...readiness.checks.duration} />
              <ReadinessCheck {...readiness.checks.clipCount} />
              <ReadinessCheck {...readiness.checks.template} />
              <ReadinessCheck {...readiness.checks.music} />
            </div>
          </div>

          {/* Output Path */}
          <div className="space-y-2">
            <Label className="text-sm font-medium">{t('autoEdit.outputFile')}</Label>
            <div className="p-3 bg-muted rounded-lg">
              <code className="text-xs break-all" data-testid="output-file-path">
                {result.output_path}
              </code>
            </div>
          </div>

           {/* Actions */}
           <div className="flex flex-col gap-3">
             <div className="flex gap-3">
               <Button
                 onClick={handleOpenLocation}
                 className="flex-1"
                 data-testid="open-location-button"
               >
                 <Download className="w-4 h-4 mr-2" />
                 {t('autoEdit.openFileLocation')}
               </Button>
               <Button
                 onClick={handlePlayVideo}
                 variant="outline"
                 className="flex-1"
                 data-testid="play-video-button"
               >
                 <Play className="w-4 h-4 mr-2" />
                 {t('autoEdit.playVideo')}
               </Button>
             </div>
             <div className="flex gap-3">
               <Button
                 onClick={() => navigate({ to: '/results' })}
                 className="flex-1"
                 variant="secondary"
               >
                 <LayoutDashboard className="w-4 h-4 mr-2" />
                 {t('results.title')}
               </Button>
               <Button
                 onClick={() => navigate({ to: '/youtube', search: { path: result.output_path } })}
                 className="flex-1"
                 variant="default"
               >
                 <Upload className="w-4 h-4 mr-2" />
                 {t('youtube.upload.uploadToYouTube')}
               </Button>
             </div>
           </div>

          <div className="flex gap-3">
            <Button onClick={onRegenerate} variant="outline" className="flex-1">
              <RefreshCw className="w-4 h-4 mr-2" />
              {t('autoEdit.regenerate', 'Regenerate with Same Config')}
            </Button>
            <Button onClick={onStartNew} variant="outline" className="flex-1">
              <RefreshCw className="w-4 h-4 mr-2" />
              {t('autoEdit.createAnotherShort')}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
