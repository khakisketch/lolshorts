import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { Results } from './Results';

// Mock i18n — keys are returned verbatim so missing keys are visible.
jest.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

jest.mock('@/components/auth/ProtectedFeature', () => ({
  ProtectedFeature: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

// The three lists are covered by their own suites; here we only care that the
// unified screen can reach all of them.
jest.mock('@/components/results/ResultsViewer', () => ({
  ResultsViewer: () => <div data-testid="highlights-list">highlights</div>,
}));

jest.mock('@/pages/Games', () => ({
  Games: () => <div data-testid="games-list">games</div>,
}));

jest.mock('@/pages/Replays', () => ({
  Replays: () => <div data-testid="replays-list">replays</div>,
}));

function setSearch(search: string) {
  window.history.replaceState({}, '', `/results${search}`);
}

describe('Results (unified library)', () => {
  afterEach(() => {
    setSearch('');
  });

  it('shows a list on entry without the user picking anything', () => {
    setSearch('');

    render(<Results />);

    expect(screen.getByTestId('highlights-list')).toBeInTheDocument();
  });

  it('offers highlights, recorded games and replays in one screen', () => {
    render(<Results />);

    expect(screen.getByTestId('results-tab-highlights')).toBeInTheDocument();
    expect(screen.getByTestId('results-tab-games')).toBeInTheDocument();
    expect(screen.getByTestId('results-tab-replays')).toBeInTheDocument();
  });

  it('switches to the recorded games list', () => {
    render(<Results />);

    // Radix tabs activate on mouse down, not on a synthesized click.
    fireEvent.mouseDown(screen.getByTestId('results-tab-games'), { button: 0 });

    expect(screen.getByTestId('games-list')).toBeInTheDocument();
  });

  it('opens the tab named by ?tab= so redirected deep links keep working', () => {
    setSearch('?tab=replays');

    render(<Results />);

    expect(screen.getByTestId('replays-list')).toBeInTheDocument();
  });

  it('falls back to highlights for an unknown ?tab= value', () => {
    setSearch('?tab=nonsense');

    render(<Results />);

    expect(screen.getByTestId('highlights-list')).toBeInTheDocument();
  });
});
