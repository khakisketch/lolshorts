import { cmd } from './client';
import { RecordingSettings, RecommendedSettings } from '@/types';

export const settingsApi = {
  getRecordingSettings: () => cmd<RecordingSettings>('get_recording_settings'),

  saveRecordingSettings: (settings: RecordingSettings) =>
    cmd<void>('save_recording_settings', { settings }),

  resetToDefault: () => cmd<void>('reset_settings_to_default'),

  // Platform optimization
  detectPlatformConfig: () => cmd<unknown>('detect_platform_config'),
  getRecommendedSettings: () => cmd<RecommendedSettings>('get_recommended_settings'),
};
