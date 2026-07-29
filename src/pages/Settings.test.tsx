import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { Settings } from './Settings';
import { HIGHLIGHT_PRESET_FILTERS } from '@/components/settings/highlightPreset';
import type { RecordingSettings } from '../types';

// Mock i18n. `t` must keep a stable identity: Settings memoises its loaders on
// [t, toast], so a fresh function per render would re-run the load effect on every
// render — an infinite reload loop that also stomps on state changes under test.
jest.mock('react-i18next', () => {
  const t = (key: string) => key;
  return { useTranslation: () => ({ t }) };
});

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

// BasicSettings resolves the storage path through Tauri on mount.
jest.mock('@tauri-apps/api/path', () => ({
  dataDir: jest.fn().mockResolvedValue('C:\\data'),
  join: jest.fn((...parts: string[]) => Promise.resolve(parts.join('\\'))),
}));
jest.mock('@tauri-apps/plugin-shell', () => ({
  open: jest.fn().mockResolvedValue(undefined),
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
// 고급의 이벤트 패널은 "개별 토글 하나를 바꾸는" 동작만 흉내낸다 — 기본 화면이
// 그 변화를 따라오는지(양방향 동기화) 보기 위한 것이다.
jest.mock('@/components/settings/EventFilterSettings', () => ({
  EventFilterSettings: ({
    settings,
    onChange,
  }: {
    settings: Record<string, unknown>;
    onChange: (next: Record<string, unknown>) => void;
  }) => (
    <div>
      <div>Event Filter Settings</div>
      <button
        type="button"
        onClick={() => onChange({ ...settings, record_deaths: true })}
      >
        toggle-record-deaths
      </button>
    </div>
  ),
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
jest.mock('@/components/settings/DiagnosticsSection', () => ({
  DiagnosticsSection: () => <div data-testid="diagnostics-section">Diagnostics Section</div>,
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
    bitrate_preset: 'medium',
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
  // 백엔드가 돌려주는 모양 그대로(숨은 필드 포함) — 프리셋 판정이 성립하려면 필요하다.
  event_filter: HIGHLIGHT_PRESET_FILTERS.balanced,
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

const openAdvanced = async () => {
  const toggle = await screen.findByTestId('advanced-settings-toggle');
  act(() => {
    fireEvent.click(toggle);
  });
};

/** Radix 탭은 mousedown 으로 전환된다. */
const selectTab = (label: string) => {
  act(() => {
    fireEvent.mouseDown(screen.getByText(label));
  });
};

describe('Settings', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockGetRecordingSettings.mockResolvedValue(defaultSettings);
    mockSaveRecordingSettings.mockResolvedValue(undefined);
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

    it('should render the five basic cards without expanding anything', async () => {
      render(<Settings />);

      expect(await screen.findByTestId('basic-highlights')).toBeInTheDocument();
      expect(screen.getByTestId('basic-quality')).toBeInTheDocument();
      expect(screen.getByTestId('basic-sound')).toBeInTheDocument();
      expect(screen.getByTestId('basic-storage')).toBeInTheDocument();
      expect(screen.getByTestId('basic-auto-start')).toBeInTheDocument();
    });

    it('should keep the advanced panels collapsed until asked', async () => {
      render(<Settings />);

      expect(await screen.findByTestId('advanced-settings')).toBeInTheDocument();
      expect(screen.queryByText('Event Filter Settings')).not.toBeInTheDocument();
      expect(screen.queryByText('Video Settings')).not.toBeInTheDocument();
      expect(screen.queryByTestId('diagnostics-section')).not.toBeInTheDocument();
      expect(screen.queryByText('Language Selector')).not.toBeInTheDocument();
    });

    it('should reveal every advanced panel when expanded', async () => {
      render(<Settings />);
      await openAdvanced();

      expect(screen.getByText('settings.recordingConfig.title')).toBeInTheDocument();
      expect(screen.getByText('Language Selector')).toBeInTheDocument();
      expect(screen.getByTestId('diagnostics-section')).toBeInTheDocument();
      // 기본 탭(일반)만 즉시 렌더되고, 나머지는 탭을 눌러야 마운트된다.
      expect(screen.getByText('General Settings')).toBeInTheDocument();

      selectTab('settings.recordingConfig.tabs.events');
      expect(screen.getByText('Event Filter Settings')).toBeInTheDocument();

      selectTab('settings.recordingConfig.tabs.video');
      expect(screen.getByText('Video Settings')).toBeInTheDocument();

      selectTab('settings.recordingConfig.tabs.audio');
      expect(screen.getByText('Audio Settings')).toBeInTheDocument();

      selectTab('settings.recordingConfig.tabs.timing');
      expect(screen.getByText('Clip Timing Settings')).toBeInTheDocument();

      selectTab('settings.recordingConfig.tabs.hotkeys');
      expect(screen.getByText('Hotkey Settings')).toBeInTheDocument();

      selectTab('settings.recordingConfig.tabs.modes');
      expect(screen.getByText('Game Mode Settings')).toBeInTheDocument();
    });
  });

  describe('Basic ↔ advanced sync', () => {
    it('writes the canonical toggle set when a basic preset is picked', async () => {
      render(<Settings />);

      const bestOnly = await screen.findByRole('radio', {
        name: 'settings.basic.highlights.options.best_only.label',
      });
      act(() => {
        fireEvent.click(bestOnly);
      });

      await waitFor(() =>
        expect(
          screen.getByRole('radio', {
            name: 'settings.basic.highlights.options.best_only.label',
          }),
        ).toBeChecked(),
      );
      expect(mockSaveRecordingSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          event_filter: HIGHLIGHT_PRESET_FILTERS.best_only,
        }),
      );
    });

    it('falls back to "직접 설정" when an advanced toggle deviates', async () => {
      render(<Settings />);

      expect(
        await screen.findByRole('radio', {
          name: 'settings.basic.highlights.options.balanced.label',
        }),
      ).toBeChecked();

      await openAdvanced();
      selectTab('settings.recordingConfig.tabs.events');
      act(() => {
        fireEvent.click(screen.getByText('toggle-record-deaths'));
      });

      expect(await screen.findByTestId('highlights-custom-hint')).toBeInTheDocument();
      expect(mockSaveRecordingSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          event_filter: expect.objectContaining({ record_deaths: true }),
        }),
      );
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
      await openAdvanced();

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

    it('should offer a retry when settings fail to load', async () => {
      mockGetRecordingSettings.mockRejectedValue(new Error('nope'));

      render(<Settings />);

      expect(
        await screen.findByText('settings.recordingConfig.loadError'),
      ).toBeInTheDocument();
      expect(screen.queryByTestId('basic-settings')).not.toBeInTheDocument();
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
