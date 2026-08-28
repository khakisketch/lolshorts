import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type {
  ClipMetadata,
  ClipVaultGameGroup,
  ClipVaultPage,
} from "@/types/storage";
import { ClipVault } from "./ClipVault";

jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}(${JSON.stringify(params)})` : key,
    i18n: { language: "en" },
  }),
}));

jest.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://${path}`,
}));

const mockNavigate = jest.fn();
jest.mock("@tanstack/react-router", () => ({
  useNavigate: () => mockNavigate,
}));

jest.mock("@/components/video/VideoPlayer", () => ({
  VideoPlayer: ({ title, src }: { title?: string; src: string }) => (
    <div data-testid="clip-workspace-player" data-src={src}>
      {title}
    </div>
  ),
}));

const mockListPage = jest.fn();
const mockEnsureThumbnail = jest.fn();
const mockGetStorageStats = jest.fn();
const mockDeleteGame = jest.fn();
jest.mock("@/api/storage", () => ({
  storageApi: {
    listClipVaultPage: (...args: unknown[]) => mockListPage(...args),
    ensureClipThumbnail: (...args: unknown[]) => mockEnsureThumbnail(...args),
    getStorageStats: (...args: unknown[]) => mockGetStorageStats(...args),
    deleteGame: (...args: unknown[]) => mockDeleteGame(...args),
  },
}));

const mockSetPinnedClips = jest.fn();
const mockSetSelectedGameIds = jest.fn();
jest.mock("@/stores/autoEditStore", () => ({
  useAutoEditStore: (selector: (state: unknown) => unknown) =>
    selector({
      setPinnedClips: mockSetPinnedClips,
      setSelectedGameIds: mockSetSelectedGameIds,
      targetDuration: 60,
    }),
}));

function clip(overrides: Partial<ClipMetadata> = {}): ClipMetadata {
  return {
    file_path: "C:/clips/default.mp4",
    thumbnail_path: "C:/clips/default.jpg",
    event_type: "champion_kill",
    event_time: 60,
    priority: 1,
    duration: 12,
    created_at: "2026-08-01T00:00:00Z",
    ...overrides,
  };
}

function group(
  gameId: string,
  clips: ClipMetadata[],
  champion = gameId,
): ClipVaultGameGroup {
  return {
    game_id: gameId,
    game: {
      game_id: gameId,
      champion,
      game_mode: "CLASSIC",
      result: "Win",
      start_time: "2026-08-01T00:00:00Z",
      end_time: "2026-08-01T00:30:00Z",
      kda: { kills: 5, deaths: 2, assists: 7 },
    },
    clips,
    clip_count: clips.length,
  };
}

function page(
  groups: ClipVaultGameGroup[],
  nextCursor: string | null = null,
  skipped = 0,
): ClipVaultPage {
  return {
    groups,
    next_cursor: nextCursor,
    skipped_item_count: skipped,
  };
}

beforeEach(() => {
  jest.clearAllMocks();
  mockEnsureThumbnail.mockResolvedValue("C:/clips/generated.jpg");
  mockGetStorageStats.mockResolvedValue({
    total_games: 1,
    total_clips: 1,
    total_size_bytes: 1024,
  });
  mockDeleteGame.mockResolvedValue(undefined);
  mockListPage.mockResolvedValue(page([group("game-a", [clip()], "Ahri")]));
});

describe("ClipVault", () => {
  it("loads one grouped page and labels ranks as local to the game", async () => {
    mockListPage.mockResolvedValue(
      page([
        group(
          "game-a",
          [
            clip({ file_path: "C:/clips/one.mp4" }),
            clip({ file_path: "C:/clips/two.mp4" }),
            clip({ file_path: "C:/clips/three.mp4" }),
          ],
          "Ahri",
        ),
      ]),
    );

    render(<ClipVault />);

    expect(await screen.findByText("Ahri")).toBeInTheDocument();
    expect(
      screen.queryByTestId("clip-vault-card-C:/clips/one.mp4"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("clip-vault-disclosure-game-a"));
    expect(mockListPage).toHaveBeenCalledWith({
      sort: "best",
      cursor: null,
      game_limit: 6,
    });
    expect(screen.getAllByText(/results\.clips\.gameRank/)).toHaveLength(3);
  });

  it("opens and closes one game group without affecting another", async () => {
    mockListPage.mockResolvedValue(
      page([
        group("game-a", [clip({ file_path: "C:/clips/a.mp4" })]),
        group("game-b", [clip({ file_path: "C:/clips/b.mp4" })]),
      ]),
    );

    render(<ClipVault />);

    const first = await screen.findByTestId("clip-vault-disclosure-game-a");
    const second = screen.getByTestId("clip-vault-disclosure-game-b");
    expect(first).toHaveAttribute("aria-expanded", "false");
    expect(second).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(first);
    expect(first).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByTestId("clip-vault-card-C:/clips/a.mp4"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("clip-vault-card-C:/clips/b.mp4"),
    ).not.toBeInTheDocument();

    fireEvent.click(first);
    expect(first).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByTestId("clip-vault-card-C:/clips/a.mp4"),
    ).not.toBeInTheDocument();
  });

  it("expands and collapses all groups without rendering folded cards", async () => {
    mockListPage.mockResolvedValue(
      page([
        group("game-a", [clip({ file_path: "C:/clips/a.mp4" })]),
        group("game-b", [clip({ file_path: "C:/clips/b.mp4" })]),
      ]),
    );

    render(<ClipVault />);
    await screen.findByTestId("clip-vault-disclosure-game-a");

    fireEvent.click(screen.getByTestId("clip-vault-expand-all"));
    expect(screen.getByTestId("clip-vault-grid-game-a")).toHaveAttribute(
      "role",
      "region",
    );
    expect(
      screen.getByTestId("clip-vault-card-C:/clips/a.mp4"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("clip-vault-card-C:/clips/b.mp4"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("clip-vault-collapse-all"));
    expect(
      screen.queryByTestId("clip-vault-card-C:/clips/a.mp4"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("clip-vault-card-C:/clips/b.mp4"),
    ).not.toBeInTheDocument();
  });

  it("loads the opaque next page without dropping the first page", async () => {
    mockListPage
      .mockResolvedValueOnce(
        page(
          [group("game-a", [clip({ file_path: "C:/clips/a.mp4" })])],
          "opaque",
        ),
      )
      .mockResolvedValueOnce(
        page([group("game-b", [clip({ file_path: "C:/clips/b.mp4" })])]),
      );

    render(<ClipVault />);
    await screen.findByTestId("clip-vault-game-game-a");
    fireEvent.click(
      screen.getByRole("button", { name: "results.clips.loadMore" }),
    );

    expect(
      await screen.findByTestId("clip-vault-game-game-b"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("clip-vault-game-game-a")).toBeInTheDocument();
    expect(mockListPage).toHaveBeenLastCalledWith({
      sort: "best",
      cursor: "opaque",
      game_limit: 6,
    });
  });

  it("keeps selections through sorting and later page loads", async () => {
    mockListPage.mockImplementation(
      async ({ sort, cursor }: { sort: string; cursor: string | null }) => {
        if (cursor) {
          return page([
            group("game-b", [clip({ file_path: "C:/clips/b.mp4" })]),
          ]);
        }
        return page(
          [group("game-a", [clip({ file_path: "C:/clips/a.mp4" })])],
          sort === "newest" ? "more" : null,
        );
      },
    );

    render(<ClipVault />);
    fireEvent.click(await screen.findByTestId("clip-vault-disclosure-game-a"));
    fireEvent.click(
      await screen.findByRole("checkbox", { name: /results\.clips\.select/ }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "results.clips.sortNewest" }),
    );
    await waitFor(() =>
      expect(mockListPage).toHaveBeenLastCalledWith(
        expect.objectContaining({ sort: "newest" }),
      ),
    );
    expect(screen.getByTestId("clip-vault-action-bar")).toHaveTextContent(
      '"clips":1',
    );

    fireEvent.click(
      screen.getByRole("button", { name: "results.clips.loadMore" }),
    );
    expect(
      await screen.findByTestId("clip-vault-game-game-b"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("clip-vault-action-bar")).toHaveTextContent(
      '"clips":1',
    );
  });

  it("sends the debounced search query to the storage page request", async () => {
    render(<ClipVault />);
    await screen.findByTestId("clip-vault-game-game-a");

    fireEvent.change(screen.getByTestId("clip-vault-search"), {
      target: { value: "Ahri" },
    });

    await waitFor(() =>
      expect(mockListPage).toHaveBeenLastCalledWith(
        expect.objectContaining({
          cursor: null,
          query: "Ahri",
          sort: "best",
        }),
      ),
    );
  });

  it("selects and clears a whole game group", async () => {
    mockListPage.mockResolvedValue(
      page([
        group("game-a", [
          clip({ file_path: "C:/clips/a.mp4" }),
          clip({ file_path: "C:/clips/b.mp4" }),
        ]),
      ]),
    );
    render(<ClipVault />);

    fireEvent.click(
      await screen.findByRole("button", { name: "results.clips.selectGame" }),
    );
    expect(screen.getByTestId("clip-vault-action-bar")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("clip-vault-disclosure-game-a"));
    expect(screen.getAllByRole("checkbox")).toEqual([
      expect.objectContaining({ checked: true }),
      expect.objectContaining({ checked: true }),
    ]);
    fireEvent.click(
      screen.getByRole("button", { name: "results.clips.clearGameSelection" }),
    );
    expect(
      screen.queryByTestId("clip-vault-action-bar"),
    ).not.toBeInTheDocument();
  });

  it("keeps in-page playback separate from selection", async () => {
    const onSelectionChange = jest.fn();
    render(<ClipVault onSelectionChange={onSelectionChange} />);

    fireEvent.click(await screen.findByTestId("clip-vault-disclosure-game-a"));
    await screen.findByTestId("clip-vault-card-C:/clips/default.mp4");
    fireEvent.click(
      screen.getByRole("button", { name: /results\.clips\.play/ }),
    );
    expect(screen.getByTestId("clip-workspace-player")).toBeInTheDocument();
    expect(onSelectionChange).toHaveBeenLastCalledWith([]);
  });

  it("can collapse the contextual event list without closing the selected player", async () => {
    render(<ClipVault />);

    fireEvent.click(await screen.findByTestId("clip-vault-disclosure-game-a"));
    await screen.findByTestId("clip-vault-card-C:/clips/default.mp4");
    fireEvent.click(
      screen.getByRole("button", { name: /results\.clips\.play/ }),
    );
    fireEvent.click(screen.getByRole("button", { name: "common.close" }));

    expect(screen.getByTestId("clip-workspace-player")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "results.clips.title" }),
    ).toHaveAttribute("aria-expanded", "false");
  });

  it("summarizes multi-game selections and pins them before montage navigation", async () => {
    const onCreateMontage = jest.fn();
    mockListPage.mockResolvedValue(
      page([
        group("game-a", [clip({ file_path: "C:/clips/a.mp4", duration: 40 })]),
        group("game-b", [clip({ file_path: "C:/clips/b.mp4", duration: 35 })]),
      ]),
    );
    render(<ClipVault onCreateMontage={onCreateMontage} />);

    fireEvent.click(await screen.findByTestId("clip-vault-disclosure-game-a"));
    fireEvent.click(await screen.findByTestId("clip-vault-disclosure-game-b"));
    const checkboxes = await screen.findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    expect(screen.getByTestId("clip-vault-action-bar")).toHaveTextContent(
      '"games":2',
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "results.clips.overTargetWarning",
    );
    fireEvent.click(screen.getByTestId("create-montage-button"));

    const groups = [
      { gameId: "game-a", paths: ["C:/clips/a.mp4"] },
      { gameId: "game-b", paths: ["C:/clips/b.mp4"] },
    ];
    expect(mockSetPinnedClips).toHaveBeenCalledWith({ groups });
    expect(mockSetSelectedGameIds).toHaveBeenCalledWith(["game-a", "game-b"]);
    expect(onCreateMontage).toHaveBeenCalledWith(groups);
  });

  it("shows a non-blocking warning for skipped metadata", async () => {
    mockListPage.mockResolvedValue(page([group("game-a", [clip()])], null, 2));
    render(<ClipVault />);

    expect(
      await screen.findByTestId("clip-vault-skipped-warning"),
    ).toHaveTextContent('"count":2');
    expect(screen.getByTestId("clip-vault-game-game-a")).toBeInTheDocument();
  });

  it("does not generate a missing thumbnail before its card becomes visible", async () => {
    let reveal: ((entries: Array<{ isIntersecting: boolean }>) => void) | null =
      null;
    const original = global.IntersectionObserver;
    global.IntersectionObserver = class {
      constructor(
        callback: (entries: Array<{ isIntersecting: boolean }>) => void,
      ) {
        reveal = callback;
      }
      observe() {}
      disconnect() {}
      unobserve() {}
      takeRecords() {
        return [];
      }
      root = null;
      rootMargin = "";
      thresholds = [];
    } as unknown as typeof IntersectionObserver;
    mockListPage.mockResolvedValue(
      page([
        group("game-visible", [
          clip({
            file_path: "C:/clips/visible-only.mp4",
            thumbnail_path: null,
          }),
        ]),
      ]),
    );

    render(<ClipVault />);
    fireEvent.click(
      await screen.findByTestId("clip-vault-disclosure-game-visible"),
    );
    await screen.findByTestId("clip-vault-card-C:/clips/visible-only.mp4");
    expect(mockEnsureThumbnail).not.toHaveBeenCalled();
    act(() => reveal?.([{ isIntersecting: true }]));
    await waitFor(() =>
      expect(mockEnsureThumbnail).toHaveBeenCalledWith(
        "game-visible",
        "C:/clips/visible-only.mp4",
      ),
    );
    global.IntersectionObserver = original;
  });
});
