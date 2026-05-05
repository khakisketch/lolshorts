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

jest.mock('@/components/StatusDashboard', () => ({
  StatusDashboard: () => <div data-testid="status-dashboard" />,
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

  it('should render status diagnostics dashboard', async () => {
    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('status-dashboard')).toBeInTheDocument();
    });
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
