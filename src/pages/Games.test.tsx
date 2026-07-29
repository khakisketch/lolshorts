import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';
import { Games } from './Games';
import { useEditorStore } from '@/stores/editorStore';

// Mock i18n
jest.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

// Mock router
const mockNavigate = jest.fn();

jest.mock('@tanstack/react-router', () => ({
  useNavigate: () => mockNavigate,
}));

// Mock storage hook
const mockListGames = jest.fn();
const mockGetGameMetadata = jest.fn();
const mockDeleteGame = jest.fn();
const mockGetStorageStats = jest.fn();

jest.mock('@/hooks/useStorage', () => ({
  useStorage: () => ({
    listGames: mockListGames,
    getGameMetadata: mockGetGameMetadata,
    deleteGame: mockDeleteGame,
    getStorageStats: mockGetStorageStats,
    isLoading: false,
    error: null,
  }),
}));

// Mock confirm dialog
jest.mock('@/components/ui/confirm-dialog', () => ({
  useConfirmDialog: () => ({
    confirm: jest.fn().mockResolvedValue(true),
    ConfirmDialog: () => null,
  }),
}));

// Mock utils
jest.mock('@/lib/utils', () => ({
  cn: (...args: unknown[]) => args.filter(Boolean).join(' '),
  formatDuration: (seconds: number) => `${Math.floor(seconds / 60)}:${seconds % 60}`,
  formatStorage: (bytes: number) => `${Math.round(bytes / 1024 / 1024)} MB`,
  pageStyles: {
    container: 'container',
    title: 'title',
  },
}));

describe('Games', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockListGames.mockResolvedValue([]);
    mockGetStorageStats.mockResolvedValue({
      total_games: 0,
      total_clips: 0,
      total_size_bytes: 0,
    });
  });

  describe('Basic Rendering', () => {
    it('should render games page title', async () => {
      render(<Games />);

      await waitFor(() => {
        expect(screen.getByText('games.recordedGames')).toBeInTheDocument();
      });
    });

    it('should render refresh button', async () => {
      render(<Games />);

      await waitFor(() => {
        expect(screen.getByText('games.refresh')).toBeInTheDocument();
      });
    });
  });

  describe('Statistics Display', () => {
    it('should display storage stats', async () => {
      mockGetStorageStats.mockResolvedValue({
        total_games: 5,
        total_clips: 25,
        total_size_bytes: 1073741824, // 1GB
      });

      render(<Games />);

      await waitFor(() => {
        expect(screen.getByText('5')).toBeInTheDocument();
        expect(screen.getByText('25')).toBeInTheDocument();
        expect(screen.getByText('games.stats.totalGames')).toBeInTheDocument();
        expect(screen.getByText('games.stats.totalClips')).toBeInTheDocument();
      });
    });

    it('should load stats on mount', async () => {
      render(<Games />);

      await waitFor(() => {
        expect(mockGetStorageStats).toHaveBeenCalled();
      });
    });
  });

  describe('Empty State', () => {
    it('should show empty state when no games', async () => {
      mockListGames.mockResolvedValue([]);

      render(<Games />);

      await waitFor(() => {
        expect(screen.getByText('games.noGamesRecorded')).toBeInTheDocument();
      });
    });

    it('should offer a way back home in empty state', async () => {
      mockListGames.mockResolvedValue([]);

      render(<Games />);

      await waitFor(() => {
        expect(screen.getByText('games.goHome')).toBeInTheDocument();
      });
    });
  });

  describe('Games List', () => {
    it('should load games on mount', async () => {
      render(<Games />);

      await waitFor(() => {
        expect(mockListGames).toHaveBeenCalled();
      });
    });

    it('should display game cards when games exist', async () => {
      mockListGames.mockResolvedValue(['game1', 'game2']);

      mockGetGameMetadata.mockImplementation((gameId: string) =>
        Promise.resolve({
          game_id: gameId,
          champion: 'Yasuo',
          game_mode: 'Ranked',
          start_time: '2024-01-01T12:00:00Z',
          end_time: '2024-01-01T12:30:00Z',
          result: 'Win',
          kda: { kills: 10, deaths: 3, assists: 7 },
        })
      );

      render(<Games />);

      await waitFor(() => {
        expect(screen.getAllByText(/Yasuo - Ranked/)).toHaveLength(2);
      });
    });

    it('should display KDA for games', async () => {
      mockListGames.mockResolvedValue(['game1']);
      mockGetGameMetadata.mockResolvedValue({
        game_id: 'game1',
        champion: 'Lux',
        game_mode: 'ARAM',
        start_time: '2024-01-01T12:00:00Z',
        end_time: '2024-01-01T12:20:00Z',
        result: 'Win',
        kda: { kills: 15, deaths: 2, assists: 20 },
      });

      render(<Games />);

      await waitFor(() => {
        expect(screen.getByText('15 / 2 / 20')).toBeInTheDocument();
      });
    });

    it('opens a recorded game in the editor via "다듬기"', async () => {
      mockListGames.mockResolvedValue(['game1']);
      mockGetGameMetadata.mockResolvedValue({
        game_id: 'game1',
        champion: 'Lux',
        game_mode: 'ARAM',
        start_time: '2024-01-01T12:00:00Z',
        end_time: '2024-01-01T12:20:00Z',
        result: 'Win',
        kda: { kills: 15, deaths: 2, assists: 20 },
      });

      render(<Games />);

      const polishButton = await screen.findByTestId('game-polish-game1');

      expect(polishButton).toBeEnabled();

      fireEvent.click(polishButton);

      // The editor works off the selected game, so the selection has to be set
      // as well — navigating alone would open an empty editor.
      expect(useEditorStore.getState().selectedGameId).toBe('game1');
      expect(mockNavigate).toHaveBeenCalledWith({
        to: '/editor',
        search: { gameId: 'game1' },
      });
    });

    it('no longer offers a manual auto-edit entry point', async () => {
      mockListGames.mockResolvedValue(['game1']);
      mockGetGameMetadata.mockResolvedValue({
        game_id: 'game1',
        champion: 'Lux',
        game_mode: 'ARAM',
        start_time: '2024-01-01T12:00:00Z',
        end_time: '2024-01-01T12:20:00Z',
        result: 'Win',
        kda: { kills: 15, deaths: 2, assists: 20 },
      });

      render(<Games />);

      await screen.findByTestId('game-polish-game1');

      expect(
        screen.queryByRole('button', { name: 'games.game.autoEdit' })
      ).not.toBeInTheDocument();
    });
  });

  describe('Error Handling', () => {
    it('should handle games list error gracefully', async () => {
      mockListGames.mockRejectedValue(new Error('Network error'));

      render(<Games />);

      // Component should render without crashing
      await waitFor(() => {
        expect(screen.getByText('games.recordedGames')).toBeInTheDocument();
      });
    });
  });
});
