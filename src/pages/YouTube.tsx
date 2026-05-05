import { YouTubeAuth } from '@/components/youtube/YouTubeAuth';
import { YouTubeUpload } from '@/components/youtube/YouTubeUpload';
import { YouTubeHistory } from '@/components/youtube/YouTubeHistory';
import { QuotaDisplay } from '@/components/youtube/QuotaDisplay';
import { ProtectedFeature } from '@/components/auth/ProtectedFeature';
import { FormErrorBoundary } from '@/components/ErrorBoundary';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Youtube, Film } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { pageStyles } from '@/lib/utils';

function getInitialUploadParams() {
  const params = new URLSearchParams(window.location.search);
  return {
    path: params.get('path') || undefined,
    resultId: params.get('resultId') || undefined,
  };
}

export function YouTube() {
  const { t } = useTranslation();
  const { path: initialUploadPath, resultId: initialResultId } = getInitialUploadParams();

  return (
    <ProtectedFeature requiresPro={true} featureName={t('youtube.title')}>
      <div className="h-full flex flex-col overflow-hidden p-6">
        <div className="flex items-center gap-3 mb-6">
          <Youtube className="h-8 w-8 text-red-500" />
          <div>
            <h1 className={pageStyles.title}>{t('youtube.title')}</h1>
            <p className="text-muted-foreground">
              {t('youtube.subtitle')}
            </p>
          </div>
        </div>

        <Tabs defaultValue="upload" className="flex-1 flex flex-col">
          <TabsList className="grid w-full grid-cols-4 mb-6">
            <TabsTrigger value="upload">{t('youtube.tabs.upload')}</TabsTrigger>
            <TabsTrigger value="history">{t('youtube.tabs.history')}</TabsTrigger>
            <TabsTrigger value="account">{t('youtube.tabs.account')}</TabsTrigger>
            <TabsTrigger value="others">{t('youtube.tabs.others', 'Other Platforms')}</TabsTrigger>
          </TabsList>

          <TabsContent value="upload" className="flex-1 overflow-y-auto space-y-6">
            <div className="grid grid-cols-1 xl:grid-cols-3 gap-6">
              <div className="xl:col-span-2">
                <FormErrorBoundary>
                  <YouTubeUpload initialPath={initialUploadPath} resultId={initialResultId} />
                </FormErrorBoundary>
              </div>
              <div className="space-y-6">
                <QuotaDisplay />
              </div>
            </div>
          </TabsContent>

          <TabsContent value="history" className="flex-1 overflow-y-auto">
            <YouTubeHistory />
          </TabsContent>

          <TabsContent value="account" className="flex-1 overflow-y-auto space-y-6">
            <div className="max-w-2xl">
              <YouTubeAuth />
              <div className="mt-6">
                <QuotaDisplay />
              </div>
            </div>
          </TabsContent>

          <TabsContent value="others" className="flex-1 overflow-y-auto">
            <div className="gaming-panel p-6 space-y-6 max-w-3xl">
              <div className="flex items-center gap-3 mb-4">
                <Film className="h-6 w-6 text-blue-500" />
                <h2 className="text-2xl font-bold">{t('youtube.others.title', 'Publish to Other Platforms')}</h2>
              </div>
              <p className="text-muted-foreground">
                {t('youtube.others.description', 'Direct upload for TikTok and Instagram is currently not supported. Please export your video and upload it manually using the following presets:')}
              </p>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="p-4 rounded-lg border border-white/10 bg-black/20 space-y-3">
                  <h3 className="font-semibold flex items-center gap-2">
                    <div className="w-2 h-2 rounded-full bg-pink-500" />
                    TikTok
                  </h3>
                  <ul className="text-sm text-muted-foreground space-y-2">
                    <li>• Aspect Ratio: 9:16 (Vertical)</li>
                    <li>• Max Duration: 60s (Recommended)</li>
                    <li>• Format: MP4 / MOV</li>
                    <li>• Resolution: 1080 x 1920</li>
                  </ul>
                </div>
                <div className="p-4 rounded-lg border border-white/10 bg-black/20 space-y-3">
                  <h3 className="font-semibold flex items-center gap-2">
                    <div className="w-2 h-2 rounded-full bg-purple-500" />
                    Instagram Reels
                  </h3>
                  <ul className="text-sm text-muted-foreground space-y-2">
                    <li>• Aspect Ratio: 9:16 (Vertical)</li>
                    <li>• Max Duration: 90s</li>
                    <li>• Format: MP4 / MOV</li>
                    <li>• Resolution: 1080 x 1920</li>
                  </ul>
                </div>
              </div>
              <div className="p-4 rounded-lg bg-blue-500/10 border border-blue-500/20 text-sm text-blue-300">
                <strong>Tip:</strong> Use the &quot;Show in Folder&quot; action in the Results library to quickly find your exported files.
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </div>
    </ProtectedFeature>
  );
}
