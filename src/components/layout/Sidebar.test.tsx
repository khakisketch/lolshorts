import { render, screen } from "@testing-library/react";
import { fireEvent } from "@testing-library/react";
import { Sidebar } from "./Sidebar";
import { useAuthStore } from "@/lib/auth";

let mockPathname = "/";

// Mock i18n with a minimal ko-like dictionary so we can tell "translated"
// output apart from a raw, un-translated hardcoded string.
jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const dict: Record<string, string> = {
        "nav.home": "홈",
        "nav.results": "결과",
        "nav.recording": "녹화",
        "nav.library": "라이브러리",
        "nav.studio": "스튜디오",
        "nav.settings": "설정",
        "nav.quit": "LoLShorts 종료",
        "nav.sidebarLabel": "애플리케이션 사이드바",
        "nav.mainNavigation": "메인 내비게이션",
        "nav.userProfile": "사용자 프로필",
        "auth.loginSignup": "로그인 / 회원가입",
      };
      return dict[key] ?? key;
    },
  }),
}));

jest.mock("@tanstack/react-router", () => ({
  useRouterState: () => mockPathname,
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

const mockInvoke = jest.fn();
jest.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

jest.mock("@/components/auth/AuthModal", () => ({
  AuthModal: () => null,
}));

describe("Sidebar", () => {
  beforeEach(() => {
    mockPathname = "/";
    (useAuthStore as unknown as jest.Mock).mockReturnValue({
      user: null,
      entitlement: null,
      isAuthenticated: false,
      logout: jest.fn(),
    });
  });

  it("uses a compact icon rail on every desktop route and expands explicitly", () => {
    const { rerender } = render(<Sidebar />);
    expect(screen.getByRole("complementary")).toHaveClass("w-16");
    expect(screen.getByText("녹화")).toHaveClass("sr-only");

    mockPathname = "/settings";
    rerender(<Sidebar />);
    expect(screen.getByRole("complementary")).toHaveClass("w-16");
    expect(screen.getByText("녹화")).toHaveClass("sr-only");

    fireEvent.click(screen.getByTestId("sidebar-toggle"));
    expect(screen.getByRole("complementary")).toHaveClass("w-64");
    expect(screen.getByText("녹화")).not.toHaveClass("sr-only");
    expect(screen.getByTestId("sidebar-toggle")).toHaveAttribute(
      "aria-expanded",
      "true",
    );

    fireEvent.click(screen.getByTestId("sidebar-toggle"));
    expect(screen.getByRole("complementary")).toHaveClass("w-16");
  });

  it("shows four top-level destinations: 녹화 / 라이브러리 / 스튜디오 / 설정", () => {
    render(<Sidebar />);

    const nav = screen.getByRole("navigation");
    const links = nav.querySelectorAll("a");

    expect(links).toHaveLength(4);
    expect(screen.getByText("녹화")).toBeInTheDocument();
    expect(screen.getByText("라이브러리")).toBeInTheDocument();
    expect(screen.getByText("스튜디오")).toBeInTheDocument();
    expect(screen.getByText("설정")).toBeInTheDocument();
  });

  it("links the four workflows to recording, library, studio and settings", () => {
    render(<Sidebar />);

    expect(screen.getByTestId("nav-dashboard")).toHaveAttribute("href", "/");
    expect(screen.getByTestId("nav-library")).toHaveAttribute(
      "href",
      "/results",
    );
    expect(screen.getByTestId("nav-studio")).toHaveAttribute(
      "href",
      "/auto-edit",
    );
    expect(screen.getByTestId("nav-settings")).toHaveAttribute(
      "href",
      "/settings",
    );
  });

  it("keeps only the studio as a top-level editing destination", () => {
    render(<Sidebar />);

    for (const testId of [
      "nav-games",
      "nav-editor",
      "nav-auto-edit",
      "nav-youtube",
    ]) {
      expect(screen.queryByTestId(testId)).not.toBeInTheDocument();
    }

    const nav = screen.getByRole("navigation");
    for (const path of ["/games", "/replays", "/editor", "/youtube"]) {
      expect(nav.querySelector(`a[href="${path}"]`)).toBeNull();
    }
  });

  it("requests a real application exit instead of only closing the window", () => {
    mockInvoke.mockResolvedValue(undefined);
    render(<Sidebar />);

    fireEvent.click(screen.getByTestId("sidebar-quit-button"));

    expect(mockInvoke).toHaveBeenCalledWith("quit_app");
  });

  it("renders no PRO badge in navigation — uploading is free now", () => {
    render(<Sidebar />);

    const nav = screen.getByRole("navigation");
    expect(nav.textContent).not.toMatch(/PRO/i);
  });

  it("does not advertise an unavailable paid upgrade to a free account", () => {
    (useAuthStore as unknown as jest.Mock).mockReturnValue({
      user: { email: "free@example.com" },
      entitlement: {
        tier: "FREE",
        status: "active",
        payment_available: false,
      },
      isAuthenticated: true,
      logout: jest.fn(),
    });

    render(<Sidebar expanded />);

    expect(screen.queryByText("auth.upgradeToPro")).not.toBeInTheDocument();
  });
});
