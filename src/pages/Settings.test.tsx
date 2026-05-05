import { render, screen, waitFor } from '@testing-library/react';
import { Settings } from './Settings';
import type { RecordingSettings } from '../types';

// Mock i18n
jest.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

// Mock auth store
const mockUseAuthStore = jest.fn();

jest.mock('@/lib/auth', () => ({
  useAuthStore: () => mockUseAuthStore(),
}));

// Mock settings API
const mockGetRecordingSettings = jest.fn();
const mockSaveRecordingSettings = jest.fn();
const mockResetToDefault = jest.fn();

jest.mock('@/api/settings', () => ({
  settingsApi: {
    getRecordingSettings: () => mockGetRecordingSettings(),
    saveRecordingSettings: (settings: unknown) => mockSaveRecordingSettings(settings),
    resetToDefault: () => mockResetToDefault(),
  },
}));

// Mock auth API
jest.mock('@/api/auth', () => ({
  authApi: {
    getCurrentEntitlement: jest.fn().mockResolvedValue({
      tier: 'FREE',
      status: 'active',
      expires_at: null,
      source: 'supabase',
      checked_at: '2026-01-01T00:00:00Z',
      payment_available: false,
    }),
  },
}));

// Mock utils
jest.mock('@/lib/utils', () => ({
  cn: (...args: unknown[]) => args.filter(Boolean).join(' '),
  pageStyles: {
    container: 'container',
    title: 'title',
  },
}));

// Mock components
jest.mock('@/components/auth', () => ({
  AuthModal: () => null,
}));
jest.mock('@/components/PaymentModal', () => ({
  PaymentModal: () => null,
}));
jest.mock('@/components/SubscriptionManagement', () => ({
  SubscriptionManagement: () => null,
}));
jest.mock('@/components/settings/EventFilterSettings', () => ({
  EventFilterSettings: () => <div>Event Filter Settings</div>,
}));
jest.mock('@/components/settings/GameModeSettings', () => ({
  GameModeSettings: () => <div>Game Mode Settings</div>,
}));
jest.mock('@/components/settings/VideoSettings', () => ({
  VideoSettings: () => <div>Video Settings</div>,
}));
jest.mock('@/components/settings/AudioSettings', () => ({
  AudioSettings: () => <div>Audio Settings</div>,
}));
jest.mock('@/components/settings/ClipTimingSettings', () => ({
  ClipTimingSettings: () => <div>Clip Timing Settings</div>,
}));
jest.mock('@/components/settings/HotkeySettings', () => ({
  HotkeySettings: () => <div>Hotkey Settings</div>,
}));
jest.mock('@/components/settings/LanguageSelector', () => ({
  LanguageSelector: () => <div>Language Selector</div>,
}));
jest.mock('@/components/settings/GeneralSettings', () => ({
  GeneralSettings: ({ settings }: {
    settings: {
      show_replay_popup: boolean;
      crash_reporting_enabled: boolean;
      overlay_enabled: boolean;
      storage: {
        auto_delete_enabled: boolean;
        auto_delete_days: number;
        max_storage_gb: number;
        delete_exported_clips: boolean;
      };
    };
  }) => (
    <div>
      <div>General Settings</div>
      <div data-testid="general-settings-fixture-shape">
        {JSON.stringify({
          show_replay_popup: settings.show_replay_popup,
          crash_reporting_enabled: settings.crash_reporting_enabled,
          overlay_enabled: settings.overlay_enabled,
          storage: settings.storage,
        })}
      </div>
    </div>
  ),
}));

const defaultSettings: RecordingSettings = {
  video: {
    resolution: 'r1920x1080',
    frame_rate: 'fps60',
    bitrate_preset: 'high',
    codec: 'h264',
    encoder: 'auto',
  },
  audio: {
    record_microphone: false,
    microphone_device: null,
    microphone_volume: 100,
    record_system_audio: true,
    system_audio_device: 'default',
    system_audio_volume: 100,
    sample_rate: 'hz48000',
    bitrate: 'kbps192',
  },
  event_filter: {
    record_kills: true,
    record_multikills: true,
    record_first_blood: true,
    record_deaths: false,
    record_shutdown: true,
    record_assists: false,
    record_dragon: true,
    record_baron: true,
    record_elder: true,
    record_herald: true,
    record_turret: true,
    record_inhibitor: true,
    record_nexus: true,
    record_ace: true,
    record_game_end: true,
    record_steal: true,
    min_priority: 2,
  },
  game_mode: {
    record_ranked_solo: true,
    record_ranked_flex: true,
    record_normal: true,
    record_quick_play: true,
    record_aram: true,
    record_arena: true,
    record_special: true,
    record_custom: false,
    record_practice: false,
  },
  clip_timing: {
    default_pre_duration: 15,
    default_post_duration: 5,
    event_timings: {},
    merge_consecutive_events: true,
    merge_time_threshold: 10,
  },
  hotkeys: {
    toggle_recording: 'F8',
    manual_save_clip: 'F9',
    delete_last_clip: 'F10',
  },
  storage: {
    auto_delete_enabled: false,
    auto_delete_days: 30,
    max_storage_gb: 50,
    delete_exported_clips: false,
  },
  auto_start_with_league: true,
  minimize_to_tray: true,
  show_notifications: true,
  show_replay_popup: true,
  crash_reporting_enabled: false,
  overlay_enabled: true,
};

describe('Settings', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockGetRecordingSettings.mockResolvedValue(defaultSettings);
    mockUseAuthStore.mockReturnValue({
      user: null,
      isAuthenticated: false,
    });
  });

  describe('Basic Rendering', () => {
    it('should render settings page title', async () => {
      render(<Settings />);

      await waitFor(() => {
        expect(screen.getByText('settings.title')).toBeInTheDocument();
      });
    });

    it('should render language selector', async () => {
      render(<Settings />);

      await waitFor(() => {
        expect(screen.getByText('Language Selector')).toBeInTheDocument();
      });
    });

    it('should render recording config section', async () => {
      render(<Settings />);

      await waitFor(() => {
        expect(screen.getByText('settings.recordingConfig.title')).toBeInTheDocument();
      });
    });
  });

  describe('Authentication States', () => {
    it('should show login prompt for license when not authenticated', async () => {
      mockUseAuthStore.mockReturnValue({
        user: null,
        isAuthenticated: false,
      });

      render(<Settings />);

      await waitFor(() => {
        expect(screen.getByText('settings.license.loginRequired')).toBeInTheDocument();
      });
    });

    it('should load license info when authenticated', async () => {
      mockUseAuthStore.mockReturnValue({
        user: { id: 'user1', email: 'test@example.com', tier: 'FREE' },
        isAuthenticated: true,
      });

      render(<Settings />);

      await waitFor(() => {
        expect(screen.getByText('settings.license.title')).toBeInTheDocument();
      });
    });
  });

  describe('Settings Loading', () => {
    it('should load recording settings on mount', async () => {
      render(<Settings />);

      await waitFor(() => {
        expect(mockGetRecordingSettings).toHaveBeenCalled();
      });
    });

    it('should pass the complete general recording fixture shape', async () => {
      render(<Settings />);

      const fixtureShape = await screen.findByTestId('general-settings-fixture-shape');

      expect(fixtureShape).toHaveTextContent('"show_replay_popup":true');
      expect(fixtureShape).toHaveTextContent('"crash_reporting_enabled":false');
      expect(fixtureShape).toHaveTextContent('"overlay_enabled":true');
      expect(fixtureShape).toHaveTextContent('"max_storage_gb":50');
      expect(fixtureShape).toHaveTextContent('"delete_exported_clips":false');
    });

    it('should show loading state while settings are loading', () => {
      mockGetRecordingSettings.mockImplementation(() => new Promise(() => {})); // Never resolves

      render(<Settings />);

      expect(screen.getByText('settings.recordingConfig.loadingSettings')).toBeInTheDocument();
    });
  });

  describe('Account Section', () => {
    it('should display account info when authenticated', async () => {
      mockUseAuthStore.mockReturnValue({
        user: { id: 'user123456', email: 'test@example.com', tier: 'PRO' },
        isAuthenticated: true,
      });

      render(<Settings />);

      await waitFor(() => {
        expect(screen.getByText('settings.accountInfo.title')).toBeInTheDocument();
        expect(screen.getByText('test@example.com')).toBeInTheDocument();
      });
    });
  });
});
