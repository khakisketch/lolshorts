import { render, screen, waitFor } from '@testing-library/react';
import { YouTube } from './YouTube';

const mockProtectedFeature = jest.fn(
  ({ children }: { children: React.ReactNode; requiresPro?: boolean; featureName?: string }) => <>{children}</>
);

// Mock i18n
jest.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

// Mock utils
jest.mock('@/lib/utils', () => ({
  cn: (...args: unknown[]) => args.filter(Boolean).join(' '),
  pageStyles: {
    container: 'container',
    title: 'title',
  },
}));

// Mock ProtectedFeature to pass through children
jest.mock('@/components/auth/ProtectedFeature', () => ({
  ProtectedFeature: (props: { children: React.ReactNode; requiresPro?: boolean; featureName?: string }) => mockProtectedFeature(props),
}));

// Mock YouTube components
jest.mock('@/components/youtube/YouTubeAuth', () => ({
  YouTubeAuth: () => <div data-testid="youtube-auth">YouTube Auth Component</div>,
}));

jest.mock('@/components/youtube/YouTubeUpload', () => ({
  YouTubeUpload: () => <div data-testid="youtube-upload">YouTube Upload Component</div>,
}));

jest.mock('@/components/youtube/YouTubeHistory', () => ({
  YouTubeHistory: () => <div data-testid="youtube-history">YouTube History Component</div>,
}));

jest.mock('@/components/youtube/QuotaDisplay', () => ({
  QuotaDisplay: () => <div data-testid="quota-display">Quota Display</div>,
}));

describe('YouTube', () => {
  beforeEach(() => {
    mockProtectedFeature.mockClear();
  });

  describe('Basic Rendering', () => {
    it('should render page title', () => {
      render(<YouTube />);

      expect(screen.getByText('youtube.title')).toBeInTheDocument();
    });

    it('should render page subtitle', () => {
      render(<YouTube />);

      expect(screen.getByText('youtube.subtitle')).toBeInTheDocument();
    });

    it('should render YouTube icon', () => {
      const { container } = render(<YouTube />);

      // Check for the YouTube icon by its class
      const icon = container.querySelector('.text-red-500');
      expect(icon).toBeInTheDocument();
    });
  });

  describe('Tabs Structure', () => {
    it('should render all three tabs', () => {
      render(<YouTube />);

      expect(screen.getByText('youtube.tabs.upload')).toBeInTheDocument();
      expect(screen.getByText('youtube.tabs.history')).toBeInTheDocument();
      expect(screen.getByText('youtube.tabs.account')).toBeInTheDocument();
    });

    it('should show upload tab content by default', async () => {
      render(<YouTube />);

      // Upload tab is the default
      await waitFor(() => {
        expect(screen.getByTestId('youtube-upload')).toBeInTheDocument();
      });
    });

    it('should render quota display in upload tab', async () => {
      render(<YouTube />);

      await waitFor(() => {
        expect(screen.getAllByTestId('quota-display').length).toBeGreaterThan(0);
      });
    });
  });

  describe('Components Integration', () => {
    it('should render YouTubeUpload component', async () => {
      render(<YouTube />);

      await waitFor(() => {
        expect(screen.getByTestId('youtube-upload')).toBeInTheDocument();
      });
    });

    it('should render QuotaDisplay component', () => {
      render(<YouTube />);

      // There are 2 QuotaDisplay components - one in upload tab, one in account tab
      expect(screen.getAllByTestId('quota-display').length).toBeGreaterThanOrEqual(1);
    });
  });

  describe('Protected Feature', () => {
    it('wraps content in a PRO-only ProtectedFeature', () => {
      render(<YouTube />);

      expect(mockProtectedFeature).toHaveBeenCalledWith(
        expect.objectContaining({
          requiresPro: true,
          featureName: 'youtube.title',
        })
      );
    });
  });
});
