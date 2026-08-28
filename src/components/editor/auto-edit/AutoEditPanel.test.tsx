import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { useAutoEditStore } from "@/stores/autoEditStore";
import { AutoEditPanel } from "./AutoEditPanel";

jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}(${JSON.stringify(params)})` : key,
  }),
}));
jest.mock("@tanstack/react-router", () => ({ useSearch: () => ({}) }));
jest.mock("@/lib/auth", () => ({
  useAuthStore: () => ({ entitlement: { tier: "PRO", status: "active" } }),
}));

const mockStartAutoEdit = jest.fn();
const mockPlanAutoEdit = jest.fn();
jest.mock("@/hooks/useAutoEdit", () => ({
  useAutoEdit: () => ({
    startAutoEdit: mockStartAutoEdit,
    planAutoEdit: mockPlanAutoEdit,
    cancelAutoEdit: jest.fn(),
    stopProgressPolling: jest.fn(),
    isLoading: false,
  }),
}));

const mockGetAllGames = jest.fn();
jest.mock("@/hooks/useStorage", () => ({
  useStorage: () => ({ getAllGames: mockGetAllGames, isLoading: false }),
}));
jest.mock("@/hooks/useAutoEditQuota", () => ({
  useAutoEditQuota: () => ({ hasQuota: () => true, fetchQuota: jest.fn() }),
}));

const mockListClips = jest.fn();
jest.mock("@/api/storage", () => ({
  storageApi: { listClips: (...args: unknown[]) => mockListClips(...args) },
}));

jest.mock("../AutoEditQuotaBadge", () => ({ AutoEditQuotaBadge: () => null }));
jest.mock("./AutoEditSettings", () => ({ AutoEditSettings: () => null }));
jest.mock("./AutoEditProgress", () => ({ AutoEditProgressView: () => null }));
jest.mock("./AutoEditResult", () => ({ AutoEditResult: () => null }));
jest.mock("./AutoEditError", () => ({ AutoEditErrorView: () => null }));
jest.mock("./AutoEditPreview", () => ({
  AutoEditPreview: ({ onGenerate }: { onGenerate: () => void }) => (
    <button onClick={onGenerate}>confirm generation</button>
  ),
}));

describe("AutoEditPanel manual-selection overrun", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    const store = useAutoEditStore.getState();
    store.resetAll();
    store.setPinnedClips({
      groups: [
        { gameId: "game-a", paths: ["C:/clips/a.mp4"] },
        { gameId: "game-b", paths: ["C:/clips/b.mp4"] },
      ],
    });
    store.setSelectedGameIds(["game-a", "game-b"]);
    store.setCurrentStep("preview");
    mockGetAllGames.mockResolvedValue([
      {
        game_id: "game-a",
        champion: "Ahri",
        game_mode: "CLASSIC",
        start_time: "2026-08-09T00:00:00Z",
      },
      {
        game_id: "game-b",
        champion: "Braum",
        game_mode: "CLASSIC",
        start_time: "2026-08-08T00:00:00Z",
      },
    ]);
    mockListClips.mockImplementation(async (gameId: string) => [
      {
        file_path: `C:/clips/${gameId === "game-a" ? "a" : "b"}.mp4`,
        duration: gameId === "game-a" ? 40 : 35,
      },
    ]);
    mockStartAutoEdit.mockResolvedValue(undefined);
  });

  afterEach(() => {
    act(() => useAutoEditStore.getState().resetAll());
  });

  it("starts with every selected path and leaves over-180 decisions to the storyboard", async () => {
    render(<AutoEditPanel />);
    await waitFor(() => expect(mockListClips).toHaveBeenCalledTimes(2));

    fireEvent.click(screen.getByRole("button", { name: "confirm generation" }));

    await waitFor(() =>
      expect(mockStartAutoEdit).toHaveBeenCalledWith(
        expect.objectContaining({
          game_ids: ["game-a", "game-b"],
          selected_clip_paths: ["C:/clips/a.mp4", "C:/clips/b.mp4"],
        }),
      ),
    );
  });
});
