import { fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom";
import { fireEvent, render, waitFor } from "@testing-library/react";
import { Results } from "./Results";

let mockTab: "clips" | "highlights" | "games" | "replays" | undefined;
const mockNavigate = jest.fn();
jest.mock("@tanstack/react-router", () => ({
  useSearch: () => ({ tab: mockTab }),
  useNavigate: () => mockNavigate,
}));

// Mock i18n — keys are returned verbatim so missing keys are visible.
jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

jest.mock("@/components/auth/ProtectedFeature", () => ({
  ProtectedFeature: ({ children }: { children: React.ReactNode }) => (
    <>{children}</>
  ),
}));

// The three lists are covered by their own suites; here we only care that the
// unified screen can reach all of them.
jest.mock("@/components/results/ResultsViewer", () => ({
  ResultsViewer: () => <div data-testid="highlights-list">highlights</div>,
}));

jest.mock("@/components/results/ClipVault", () => ({
  ClipVault: () => <div data-testid="clips-list">clips</div>,
}));

jest.mock("@/pages/Replays", () => ({
  Replays: () => <div data-testid="replays-list">replays</div>,
}));

describe("Results (unified library)", () => {
  beforeEach(() => {
    mockTab = undefined;
    mockNavigate.mockReset();
  });

  it("shows a list on entry without the user picking anything", () => {
    render(<Results />);

    expect(screen.getByTestId("clips-list")).toBeInTheDocument();
  });

  it("offers game records, finished videos and replays in one screen", () => {
    render(<Results />);

    expect(screen.getByTestId("results-tab-clips")).toBeInTheDocument();
    expect(screen.getByTestId("results-tab-highlights")).toBeInTheDocument();
    expect(screen.getByTestId("results-tab-replays")).toBeInTheDocument();
    expect(screen.queryByTestId("results-tab-games")).not.toBeInTheDocument();
  });

  it("writes finished-video tab changes to the router search state", () => {
    render(<Results />);

    fireEvent.mouseDown(screen.getByTestId("results-tab-highlights"), {
      button: 0,
    });

    expect(mockNavigate).toHaveBeenCalledWith({
      search: { tab: "highlights" },
      replace: false,
    });
  });

  it("writes tab changes to the router search state", () => {
    render(<Results />);

    // Radix tabs activate on mouse down, not on a synthesized click.
    fireEvent.mouseDown(screen.getByTestId("results-tab-replays"), {
      button: 0,
    });

    expect(mockNavigate).toHaveBeenCalledWith({
      search: { tab: "replays" },
      replace: false,
    });
  });

  it("normalizes the legacy games tab to the game records tab", async () => {
    mockTab = "games";

    render(<Results />);

    expect(screen.getByTestId("clips-list")).toBeInTheDocument();
    await waitFor(() =>
      expect(mockNavigate).toHaveBeenCalledWith({
        search: { tab: "clips" },
        replace: true,
      }),
    );
  });

  it("opens the tab named by ?tab= so redirected deep links keep working", () => {
    mockTab = "replays";

    render(<Results />);

    expect(screen.getByTestId("replays-list")).toBeInTheDocument();
  });

  it("restores the active tab when router history changes", () => {
    mockTab = "games";
    const { rerender } = render(<Results />);
    expect(screen.getByTestId("clips-list")).toBeInTheDocument();

    mockTab = "highlights";
    rerender(<Results />);

    expect(screen.getByTestId("highlights-list")).toBeInTheDocument();
  });
});
