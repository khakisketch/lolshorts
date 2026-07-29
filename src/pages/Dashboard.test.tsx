import { render, screen, waitFor } from '@testing-library/react';
import { Dashboard } from './Dashboard';

// Mock i18n
jest.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

// Mock auth store
jest.mock('@/lib/auth', () => ({
  useAuthStore: () => ({
    checkAuth: jest.fn().mockResolvedValue(undefined),
  }),
}));

// Mock recording store
const mockRecordingStoreState = {
  status: {
    state: 'idle',
    isRecording: false,
  },
  readiness: null as unknown,
  error: null as string | null,
};

jest.mock('@/stores/recordingStore', () => ({
  useRecordingStore: () => ({
    ...mockRecordingStoreState,
    startRecording: jest.fn().mockResolvedValue(undefined),
    stopRecording: jest.fn().mockResolvedValue(undefined),
  }),
}));

// Mock LCU API
jest.mock('@/api/lcu', () => ({
  lcuApi: {
    connect: jest.fn().mockResolvedValue(undefined),
    getUnifiedGameStatus: jest.fn().mockResolvedValue({
      lcu_connected: true,
      in_game: false,
      summoner_name: null,
      champion_name: null,
      game_time: null,
      is_recording: false,
    }),
  },
}));

// Mock utils API
jest.mock('@/api/utils', () => ({
  utilsApi: {
    getDashboardStats: jest.fn().mockResolvedValue({
      total_games: 5,
      total_clips: 25,
      total_size_bytes: 1073741824,
    }),
  },
}));

// Mock settings API (Dashboard reads hotkeys for the shortcut card; the
// nested audio/video shape is also read by the mounted RecordingControls).
jest.mock('@/api/settings', () => ({
  settingsApi: {
    getRecordingSettings: jest.fn().mockResolvedValue({
      hotkeys: {
        toggle_recording: 'F8',
        manual_save_clip: 'F9',
        delete_last_clip: 'F10',
      },
      audio: {
        record_system_audio: true,
        record_microphone: false,
        system_audio_device: 'default',
      },
      video: {
        encoder: 'auto',
      },
    }),
    saveRecordingSettings: jest.fn().mockResolvedValue(undefined),
  },
}));

// Mock recording API (used by the mounted RecordingControls for
// start/stop/save-replay actions).
jest.mock('@/api/recording', () => ({
  recordingApi: {
    startAutoCapture: jest.fn().mockResolvedValue(undefined),
    stopAutoCapture: jest.fn().mockResolvedValue(undefined),
    saveReplay: jest.fn().mockResolvedValue('/path/to/replay.mp4'),
  },
}));

// Mock toast (used by the mounted RecordingControls)
jest.mock('@/components/ui/use-toast', () => ({
  toast: jest.fn(),
}));

// Mock YouTube API (resilient probe, not shown on dashboard)
jest.mock('@/api/youtube', () => ({
  youtubeApi: {
    getAuthStatus: jest.fn().mockResolvedValue({
      authenticated: false,
      expires_at: null,
      has_refresh_token: false,
    }),
  },
}));

// Mock utils
jest.mock('@/lib/utils', () => ({
  cn: (...args: unknown[]) => args.filter(Boolean).join(' '),
  formatStorage: jest.fn((bytes: number) => `${bytes} bytes`),
  pageStyles: {
    container: 'container',
    title: 'title',
  },
}));

// Mock AuthModal
jest.mock('@/components/auth', () => ({
  AuthModal: () => null,
}));

// Import mocked modules after jest.mock calls
import { lcuApi } from '../api/lcu';
import { utilsApi } from '../api/utils';

const mockLcuApi = lcuApi as jest.Mocked<typeof lcuApi>;
const mockUtilsApi = utilsApi as jest.Mocked<typeof utilsApi>;

describe('Dashboard', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockRecordingStoreState.error = null;
    mockRecordingStoreState.status.state = 'idle';
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it('should render dashboard title', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText('dashboard.title')).toBeInTheDocument();
    });
  });

  it('should connect to LCU on mount', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(mockLcuApi.connect).toHaveBeenCalled();
    });
  });

  it('should fetch dashboard stats on mount', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(mockUtilsApi.getDashboardStats).toHaveBeenCalled();
    });
  });

  it('should show loading state initially', () => {
    render(<Dashboard />);
    // Dashboard uses Skeleton components for loading state, not text
    const skeletons = document.querySelectorAll('.bg-muted');
    expect(skeletons.length).toBeGreaterThan(0);
  });

  it('should poll game status periodically', async () => {
    render(<Dashboard />);

    // Wait for initial call
    await waitFor(() => {
      expect(mockLcuApi.getUnifiedGameStatus).toHaveBeenCalled();
    });

    // Polling test is timing-sensitive, verify the initial call happens
    expect(mockLcuApi.getUnifiedGameStatus).toHaveBeenCalledTimes(1);
  });

  it('should display error when initialization fails', async () => {
    mockUtilsApi.getDashboardStats.mockRejectedValueOnce(new Error('Network error'));

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText('dashboard.error.title')).toBeInTheDocument();
    });
  });

  it('should show last update time', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText(/dashboard.lastUpdate/)).toBeInTheDocument();
    });
  });

  it('should not render the diagnostics status dashboard (moved to Settings)', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText('dashboard.title')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('status-dashboard')).not.toBeInTheDocument();
  });

  it('should mount the recording controls so start/stop is reachable from the dashboard', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('recording-controls')).toBeInTheDocument();
    });
    expect(
      screen.getByText('recordingControls.autoCapture.title'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('start-auto-capture'),
    ).toBeInTheDocument();
    // Only the start button is visible until recording begins.
    expect(
      screen.queryByTestId('stop-auto-capture'),
    ).not.toBeInTheDocument();
  });

  it('should render the one-line readiness summary', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(
        screen.getByText('dashboard.readiness.summaryChecking'),
      ).toBeInTheDocument();
    });
  });

  it('should render the hotkey reference from settings', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText('dashboard.hotkeys.title')).toBeInTheDocument();
    });
    expect(screen.getByText('F8')).toBeInTheDocument();
    expect(screen.getByText('F9')).toBeInTheDocument();
    expect(screen.getByText('F10')).toBeInTheDocument();
    // F10 is the delete-last-clip action, not a 30s replay save
    expect(screen.getByText('dashboard.hotkeys.deleteLast')).toBeInTheDocument();
  });

  it('should hide the getting-started guide once games exist', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText('dashboard.hotkeys.title')).toBeInTheDocument();
    });
    expect(
      screen.queryByText('dashboard.gettingStarted.title'),
    ).not.toBeInTheDocument();
  });

  it('should show the getting-started guide on first run (0 games)', async () => {
    mockUtilsApi.getDashboardStats.mockResolvedValueOnce({
      total_games: 0,
      total_clips: 0,
      total_size_bytes: 0,
    });

    render(<Dashboard />);

    await waitFor(() => {
      expect(
        screen.getByText('dashboard.gettingStarted.title'),
      ).toBeInTheDocument();
    });
  });

  it('should show only the legacy storageUsed label when new size fields are absent (backward compat)', async () => {
    // Default mock resolves without recordings_dir_size_bytes / exports_dir_size_bytes /
    // total_disk_usage_bytes, simulating an older backend response shape.
    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText('dashboard.stats.storageUsed')).toBeInTheDocument();
    });
    expect(screen.queryByText('dashboard.stats.clipLibrary')).not.toBeInTheDocument();
    expect(screen.queryByText('dashboard.stats.totalDiskUsage')).not.toBeInTheDocument();
  });

  it('should show clip library and total disk usage when the backend reports full size breakdown', async () => {
    mockUtilsApi.getDashboardStats.mockResolvedValueOnce({
      total_games: 5,
      total_clips: 25,
      total_size_bytes: 1073741824,
      recordings_dir_size_bytes: 2147483648,
      exports_dir_size_bytes: 536870912,
      total_disk_usage_bytes: 2684354560,
    });

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText('dashboard.stats.clipLibrary')).toBeInTheDocument();
    });
    expect(screen.getByText('dashboard.stats.totalDiskUsage')).toBeInTheDocument();
    expect(screen.getByText('2684354560 bytes')).toBeInTheDocument();
    expect(screen.queryByText('dashboard.stats.storageUsed')).not.toBeInTheDocument();
  });

  it('should cleanup interval on unmount', async () => {
    const clearIntervalSpy = jest.spyOn(global, 'clearInterval');

    const { unmount } = render(<Dashboard />);

    await waitFor(() => {
      expect(mockLcuApi.connect).toHaveBeenCalled();
    });

    unmount();

    expect(clearIntervalSpy).toHaveBeenCalled();

    clearIntervalSpy.mockRestore();
  });

  describe('error handling', () => {
    it('should handle LCU connection failure gracefully', async () => {
      mockLcuApi.connect.mockRejectedValueOnce(new Error('LCU not available'));

      render(<Dashboard />);

      await waitFor(() => {
        expect(screen.getByText('dashboard.title')).toBeInTheDocument();
      });
    });

    it('should display recording status sync failures', async () => {
      mockRecordingStoreState.status.state = 'error';
      mockRecordingStoreState.error = 'Desktop runtime unavailable';

      render(<Dashboard />);

      await waitFor(() => {
        expect(screen.getByText('dashboard.recordingError.title')).toBeInTheDocument();
        expect(screen.getByText('Desktop runtime unavailable')).toBeInTheDocument();
      });
    });
  });
});
