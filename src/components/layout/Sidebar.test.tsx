import { render, screen } from '@testing-library/react';
import { Sidebar } from './Sidebar';

// Mock i18n with a minimal ko-like dictionary so we can tell "translated"
// output apart from a raw, un-translated hardcoded string.
jest.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const dict: Record<string, string> = {
        'nav.home': '홈',
        'nav.results': '결과',
        'nav.settings': '설정',
        'nav.sidebarLabel': '애플리케이션 사이드바',
        'nav.mainNavigation': '메인 내비게이션',
        'nav.userProfile': '사용자 프로필',
        'auth.loginSignup': '로그인 / 회원가입',
      };
      return dict[key] ?? key;
    },
  }),
}));

jest.mock('@tanstack/react-router', () => ({
  Link: ({
    children,
    to,
    activeProps: _activeProps,
    ...rest
  }: {
    children: React.ReactNode;
    to: string;
    activeProps?: unknown;
  } & Record<string, unknown>) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
}));

jest.mock('@/components/auth/AuthModal', () => ({
  AuthModal: () => null,
}));

describe('Sidebar', () => {
  it('shows exactly three top-level destinations: 홈 / 결과 / 설정', () => {
    render(<Sidebar />);

    const nav = screen.getByRole('navigation');
    const links = nav.querySelectorAll('a');

    expect(links).toHaveLength(3);
    expect(screen.getByText('홈')).toBeInTheDocument();
    expect(screen.getByText('결과')).toBeInTheDocument();
    expect(screen.getByText('설정')).toBeInTheDocument();
  });

  it('links the three items to /, /results and /settings', () => {
    render(<Sidebar />);

    expect(screen.getByTestId('nav-dashboard')).toHaveAttribute('href', '/');
    expect(screen.getByTestId('nav-results')).toHaveAttribute(
      'href',
      '/results',
    );
    expect(screen.getByTestId('nav-settings')).toHaveAttribute(
      'href',
      '/settings',
    );
  });

  it('no longer exposes games, replays, editor, auto-edit or youtube', () => {
    render(<Sidebar />);

    for (const testId of [
      'nav-games',
      'nav-library',
      'nav-editor',
      'nav-auto-edit',
      'nav-youtube',
    ]) {
      expect(screen.queryByTestId(testId)).not.toBeInTheDocument();
    }

    const nav = screen.getByRole('navigation');
    for (const path of ['/games', '/replays', '/editor', '/auto-edit', '/youtube']) {
      expect(nav.querySelector(`a[href="${path}"]`)).toBeNull();
    }
  });

  it('renders no PRO badge in navigation — uploading is free now', () => {
    render(<Sidebar />);

    const nav = screen.getByRole('navigation');
    expect(nav.textContent).not.toMatch(/PRO/i);
  });
});
