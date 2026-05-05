import { useTranslation } from 'react-i18next';
import { useEditorStore } from '@/stores/editorStore';
import { ClipCard } from './ClipCard';
import { Film } from 'lucide-react';

export function ClipLibrary() {
  const { t } = useTranslation();
  const { availableClips } = useEditorStore();

  if (availableClips.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center p-6 text-center">
        <Film className="w-16 h-16 text-muted-foreground mb-4" />
        <h3 className="text-lg font-semibold mb-2">{t('editor.clipLibrary.noClipsAvailable')}</h3>
        <p className="text-sm text-muted-foreground">
          {t('editor.clipLibrary.noClipsDescription')}
        </p>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="p-4 border-b">
        <h3 className="font-semibold">{t('editor.clipLibrary.title')}</h3>
        <p className="text-sm text-muted-foreground">
          {t('editor.clipLibrary.clipsCount', { count: availableClips.length })}
        </p>
      </div>

      {/* Clip Grid */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="grid grid-cols-1 gap-4">
          {availableClips.map((clip) => (
            <ClipCard key={clip.file_path} clip={clip} />
          ))}
        </div>
      </div>
    </div>
  );
}
