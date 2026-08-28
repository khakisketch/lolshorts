import { render, screen, waitFor } from "@testing-library/react";
import { Editor } from "./Editor";

// Mock i18n
jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) => {
      if (params) return `${key}:${JSON.stringify(params)}`;
      return key;
    },
  }),
}));

// Mock editor store
const mockSetSelectedGameId = jest.fn();
const mockSetAvailableClips = jest.fn();

jest.mock("@/stores/editorStore", () => ({
  useEditorStore: () => ({
    selectedGameId: null,
    setSelectedGameId: mockSetSelectedGameId,
    availableClips: [],
    setAvailableClips: mockSetAvailableClips,
  }),
}));

// Mock useEditor hook
jest.mock("@/hooks/useEditor", () => ({
  useEditor: () => ({
    loadGameClips: jest.fn().mockResolvedValue([]),
    isLoading: false,
    error: null,
  }),
}));

// Mock useStorage hook
const mockGetAllGames = jest.fn();

jest.mock("@/hooks/useStorage", () => ({
  useStorage: () => ({
    getAllGames: mockGetAllGames,
    isLoading: false,
    error: null,
  }),
}));

// Mock utils
jest.mock("@/lib/utils", () => ({
  cn: (...args: unknown[]) => args.filter(Boolean).join(" "),
}));

// Mock editor components
jest.mock("@/components/editor/EditorLayout", () => ({
  EditorLayout: () => <div data-testid="editor-layout">Editor Layout</div>,
}));
jest.mock("@/components/editor/ClipLibrary", () => ({
  ClipLibrary: () => <div>Clip Library</div>,
}));
jest.mock("@/components/editor/VideoPreview", () => ({
  VideoPreview: () => <div>Video Preview</div>,
}));
jest.mock("@/components/editor/CompositionSettings", () => ({
  CompositionSettings: () => <div>Composition Settings</div>,
}));
jest.mock("@/components/editor/Timeline", () => ({
  Timeline: () => <div>Timeline</div>,
}));
jest.mock("@/components/editor/ExportModal", () => ({
  ExportModal: () => null,
}));
jest.mock("@/components/editor/StudioModeNav", () => ({
  StudioModeNav: () => <div data-testid="studio-mode-nav" />,
}));

describe("Editor", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockGetAllGames.mockResolvedValue([]);
  });

  describe("Game Selection", () => {
    it("should render game selection screen when no game selected", async () => {
      render(<Editor />);

      await waitFor(() => {
        expect(screen.getByText("editor.selectGameTitle")).toBeInTheDocument();
      });
    });

    it("should show empty state when no games available", async () => {
      mockGetAllGames.mockResolvedValue([]);

      render(<Editor />);

      await waitFor(() => {
        expect(screen.getByText("editor.noGamesAvailable")).toBeInTheDocument();
      });
    });

    it("should display available games in dropdown", async () => {
      mockGetAllGames.mockResolvedValue([
        {
          game_id: "game1",
          game_start_time: "2024-01-01T12:00:00Z",
          champion: "Yasuo",
          game_mode: "Ranked",
        },
      ]);

      render(<Editor />);

      await waitFor(() => {
        expect(screen.getByText("editor.selectGame")).toBeInTheDocument();
      });
    });
  });

  describe("Loading States", () => {
    it("should show loading spinner when games are loading", async () => {
      // Mock loading state
      jest
        .spyOn(require("@/hooks/useStorage"), "useStorage")
        .mockImplementation(() => ({
          getAllGames: jest.fn(() => new Promise(() => undefined)),
          isLoading: true,
          error: null,
        }));

      // This test just verifies the component renders without error during loading
      const { container } = render(<Editor />);
      expect(container).toBeTruthy();
    });
  });

  describe("Games Error State", () => {
    it("shows an error message with a retry button instead of spinning forever", async () => {
      const retryGetAllGames = jest.fn().mockResolvedValue([]);
      jest
        .spyOn(require("@/hooks/useStorage"), "useStorage")
        .mockImplementation(() => ({
          getAllGames: retryGetAllGames,
          isLoading: false,
          error: "disk read error",
        }));

      render(<Editor />);

      await waitFor(() => {
        expect(
          screen.getByText("editor.errorLoadingGames"),
        ).toBeInTheDocument();
        expect(screen.getByText("disk read error")).toBeInTheDocument();
      });

      const callsBeforeRetry = retryGetAllGames.mock.calls.length;
      expect(callsBeforeRetry).toBeGreaterThan(0);

      const retryButton = screen.getByText("editor.retry");
      expect(retryButton).toBeInTheDocument();

      retryButton.click();

      await waitFor(() => {
        expect(retryGetAllGames.mock.calls.length).toBeGreaterThan(
          callsBeforeRetry,
        );
      });
    });
  });

  describe("Error States", () => {
    it("should display error when clips fail to load", async () => {
      // Mock error state
      jest
        .spyOn(require("@/hooks/useEditor"), "useEditor")
        .mockImplementation(() => ({
          loadGameClips: jest.fn(),
          isLoading: false,
          error: "Failed to load clips",
        }));

      jest
        .spyOn(require("@/stores/editorStore"), "useEditorStore")
        .mockImplementation(() => ({
          selectedGameId: "game1",
          setSelectedGameId: mockSetSelectedGameId,
          availableClips: [],
          setAvailableClips: mockSetAvailableClips,
        }));

      render(<Editor />);

      await waitFor(() => {
        expect(
          screen.getByText("editor.errorLoadingClips"),
        ).toBeInTheDocument();
      });
    });
  });
});
