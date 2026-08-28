import { fireEvent, render, screen } from "@testing-library/react";
import { AutoEditSettings } from "./AutoEditSettings";

jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}(${JSON.stringify(params)})` : key,
  }),
}));
jest.mock("../canvas/CanvasEditor", () => ({ CanvasEditor: () => null }));
jest.mock("../AudioMixer", () => ({ AudioMixer: () => null }));
jest.mock("../AutoEditQuotaBadge", () => ({ AutoEditQuotaBadge: () => null }));

const baseProps = {
  availableGames: [
    {
      game_id: "game-a",
      champion: "Ahri",
      game_mode: "CLASSIC",
      date: "2026-08-09",
      clip_count: 2,
      selected: true,
    },
    {
      game_id: "game-b",
      champion: "Braum",
      game_mode: "CLASSIC",
      date: "2026-08-08",
      clip_count: 1,
      selected: true,
    },
  ],
  selectedGameIds: ["game-a", "game-b"],
  pinnedClipCount: 3,
  pinnedGameCount: 2,
  pinnedDuration: 75,
  targetDuration: 60 as const,
  enableEventZoom: false,
  enableHookCaptions: true,
  currentTemplate: null,
  backgroundMusic: null,
  audioLevels: { game_audio: 70, background_music: 30 },
  metadata: { title: "", caption: "", tags: [] },
  isLoading: false,
  gamesLoading: false,
  isFreeTier: false,
  onToggleGame: jest.fn(),
  onUseAutomaticSelection: jest.fn(),
  onSetDuration: jest.fn(),
  onToggleEventZoom: jest.fn(),
  onToggleHookCaptions: jest.fn(),
  onTemplateChange: jest.fn(),
  onBackgroundMusicChange: jest.fn(),
  onAudioLevelsChange: jest.fn(),
  onMetadataChange: jest.fn(),
  onGenerate: jest.fn(),
};

describe("AutoEditSettings direct clip selection", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("locks game selection and shows clip, game, duration and overrun summaries", () => {
    render(<AutoEditSettings {...baseProps} />);

    const game = screen.getByTestId("game-card-game-a");
    expect(game).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(game);
    fireEvent.keyDown(game, { key: "Enter" });
    expect(baseProps.onToggleGame).not.toHaveBeenCalled();
    expect(screen.getByTestId("auto-edit-pinned-clips")).toHaveTextContent(
      '"clips":3',
    );
    expect(screen.getByTestId("auto-edit-pinned-clips")).toHaveTextContent(
      '"games":2',
    );
    expect(screen.getByTestId("pinned-duration-warning")).toHaveTextContent(
      "autoEdit.pinnedClips.overTargetWarning",
    );
  });

  it("offers an accessible switch back to automatic selection", () => {
    render(<AutoEditSettings {...baseProps} />);

    fireEvent.click(screen.getByTestId("use-automatic-selection"));
    expect(baseProps.onUseAutomaticSelection).toHaveBeenCalledTimes(1);
  });

  it("describes the free public edition without advertising unavailable checkout", () => {
    render(<AutoEditSettings {...baseProps} isFreeTier />);

    expect(screen.getByText(/autoEdit\.freeEditionNotice/)).toBeInTheDocument();
    expect(screen.queryByText("autoEdit.upgradeToPro")).not.toBeInTheDocument();
  });
});
