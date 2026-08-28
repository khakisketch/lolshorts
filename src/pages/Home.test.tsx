import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Home } from "./Home";
import type { ClipMetadata } from "@/types/storage";

// i18n: 실제 문구가 아니라 키+보간 값을 그대로 뱉게 해 "어떤 문구가 어떤 값으로
// 렌더됐는지" 를 단언한다. 문구 자체는 translation.json 이 SSOT 다.
jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}(${JSON.stringify(params)})` : key,
  }),
}));

jest.mock("@tanstack/react-router", () => ({
  // 라우터 Link 는 렌더만 되면 된다 — span 으로 두어야 a11y 린트가
  // href 없는 앵커를 잡지 않는다(테스트 더블이지 실제 링크가 아니다).
  Link: ({ children }: { children: React.ReactNode }) => (
    <span>{children}</span>
  ),
  useNavigate: () => mockNavigate,
}));

jest.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `asset://${p}`,
}));

jest.mock("@/components/ui/use-toast", () => ({
  useToast: () => ({ toast: jest.fn() }),
}));

jest.mock("@/api/lcu", () => ({
  lcuApi: {
    getUnifiedGameStatus: jest.fn().mockResolvedValue({
      lcu_connected: true,
      in_game: false,
      is_recording: false,
      is_monitoring: true,
    }),
  },
}));

const mockGetStatus = jest.fn();
jest.mock("@/api/recording", () => ({
  recordingApi: {
    startAutoCapture: jest.fn().mockResolvedValue(undefined),
    stopAutoCapture: jest.fn().mockResolvedValue(undefined),
    getStatus: (...args: unknown[]) => mockGetStatus(...args),
  },
}));

jest.mock("@/api/video", () => ({
  videoApi: { generateClipThumbnail: jest.fn().mockResolvedValue(null) },
}));

const mockListClips = jest.fn();
const mockListGames = jest.fn().mockResolvedValue(["game-1"]);
const mockGetGameMetadata = jest.fn().mockResolvedValue(null);
jest.mock("@/api/storage", () => ({
  storageApi: {
    listGames: (...args: unknown[]) => mockListGames(...args),
    listClips: (...args: unknown[]) => mockListClips(...args),
    getGameMetadata: (...args: unknown[]) => mockGetGameMetadata(...args),
  },
}));

const mockNavigate = jest.fn();
const mockSetSelectedGameId = jest.fn();
jest.mock("@/stores/editorStore", () => ({
  useEditorStore: (selector: (s: unknown) => unknown) =>
    selector({ setSelectedGameId: mockSetSelectedGameId }),
}));

const mockSetPinnedClips = jest.fn();
const mockSetSelectedGameIds = jest.fn();
jest.mock("@/stores/autoEditStore", () => ({
  useAutoEditStore: (selector: (s: unknown) => unknown) =>
    selector({
      setPinnedClips: mockSetPinnedClips,
      setSelectedGameIds: mockSetSelectedGameIds,
    }),
}));

function clip(overrides: Partial<ClipMetadata> = {}): ClipMetadata {
  return {
    file_path: "C:/clips/a.mp4",
    thumbnail_path: "C:/clips/a.jpg",
    event_type: { multikill: 3 },
    event_time: 600,
    priority: 3,
    duration: 13,
    created_at: "2026-07-30T00:00:00Z",
    usage_count: 0,
    ...overrides,
  };
}

beforeEach(() => {
  jest.clearAllMocks();
  mockListGames.mockResolvedValue(["game-1"]);
  mockGetGameMetadata.mockResolvedValue(null);
  mockGetStatus.mockResolvedValue({ capture_warning: null });
});

describe("Home — capture privacy warning", () => {
  it("shows a persistent main-app warning when desktop fallback reports one", async () => {
    mockGetStatus.mockResolvedValue({
      capture_warning:
        "Game window could not be found; recording the desktop instead.",
    });
    mockListClips.mockResolvedValue([]);

    render(<Home />);

    expect(await screen.findByTestId("home-capture-warning")).toHaveTextContent(
      "recording the desktop instead",
    );
    expect(screen.getByText("Review capture settings")).toBeInTheDocument();
  });
});

/**
 * 이 앱이 확언할 수 있는 유일한 것은 **그 순간의 게임 상태**다. 경쟁 서비스는
 * 화면 픽셀을 읽어 추정하므로 "체력 8% 였다" 를 확언할 수 없지만 우리는 Live
 * Client Data API 로 직접 받는다 — 그런데 그 값이 저장만 되고 화면에는 한 번도
 * 나오지 않았다. 차별점이 화면에 없으면 없는 것과 같다.
 */
describe("Home — 클립이 왜 뽑혔는지", () => {
  it("점수 이유를 사람 말로 카드에 보여준다", async () => {
    mockListClips.mockResolvedValue([
      clip({ score_reasons: [{ Clutch: 8 }, "Solo"] }),
    ]);

    render(<Home />);

    const line = await screen.findByTestId("home-clip-reasons-C:/clips/a.mp4");
    // 눈에 띄는 것부터 · 로 이어 붙인다.
    expect(line.textContent).toBe(
      'clip.reason.clutch({"percent":8}) · clip.reason.solo',
    );
  });

  it("숫자 점수는 화면에 내보내지 않는다", async () => {
    // "37.5점" 은 게이머에게 아무 뜻이 없다. 정렬에만 쓰고 사람에게는 이유를 준다.
    mockListClips.mockResolvedValue([
      clip({ highlight_score: 37.5, score_reasons: ["Solo"] }),
    ]);

    render(<Home />);

    await screen.findByTestId("home-clip-reasons-C:/clips/a.mp4");
    expect(screen.queryByText(/37\.5/)).not.toBeInTheDocument();
  });

  it("이유가 없는 예전 클립은 그 줄 자체가 없다", async () => {
    // `score_reasons` 가 붙기 전에 저장된 클립. 빈 줄이 남으면 카드 높이가
    // 들쭉날쭉해져 격자가 어긋난다.
    mockListClips.mockResolvedValue([
      clip({ score_reasons: [] }),
      clip({ file_path: "C:/clips/b.mp4" }),
    ]);

    render(<Home />);

    await waitFor(() => {
      expect(
        screen.getByTestId("home-clip-C:/clips/a.mp4"),
      ).toBeInTheDocument();
    });
    expect(
      screen.queryByTestId("home-clip-reasons-C:/clips/a.mp4"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("home-clip-reasons-C:/clips/b.mp4"),
    ).not.toBeInTheDocument();
  });

  it("모르는 이유 모양은 코드값으로 새어 나가지 않는다", async () => {
    // 백엔드가 변형을 늘렸는데 매핑이 없는 경우. 클립 이름이 한국어 UI 에
    // `Shutdown` 으로 나갔던 전력이 있어 같은 경로를 막아 둔다.
    mockListClips.mockResolvedValue([
      clip({
        score_reasons: ["Unheard" as never, "Solo"],
      }),
    ]);

    render(<Home />);

    const line = await screen.findByTestId("home-clip-reasons-C:/clips/a.mp4");
    expect(line.textContent).toBe("clip.reason.solo");
    expect(line.textContent).not.toContain("Unheard");
  });
});

/**
 * 사용자 지적: *"방금 만들어진 클립을 보는 건 무의미하다. 뭐가 진짜 하이라이트인지,
 * 게임이 끝나면 내 하이라이트가 뭐였는지 바로 파악할 수 있어야 한다."*
 *
 * 그래서 이 화면의 정렬 기준은 저장 시각이 아니라 하이라이트 점수다.
 */
describe("Home — 무엇이 진짜 하이라이트인가", () => {
  it("나중에 저장된 클립이라도 점수가 낮으면 아래로 간다", async () => {
    mockListClips.mockResolvedValue([
      // 목록 순서상 먼저지만 점수는 낮다. 예전 정렬(created_at 내림차순)이면
      // 이게 1위였다.
      clip({
        file_path: "C:/clips/late-assist.mp4",
        highlight_score: 12,
        created_at: "2026-07-30T00:30:00Z",
      }),
      clip({
        file_path: "C:/clips/penta.mp4",
        highlight_score: 88,
        created_at: "2026-07-30T00:05:00Z",
      }),
    ]);

    render(<Home />);

    const grid = await screen.findByTestId("home-clip-grid");
    await waitFor(() => expect(grid.children).toHaveLength(2));
    // 격자 DOM 순서가 곧 사용자가 읽는 순서다.
    expect(grid.children[0]).toContainElement(
      screen.getByTestId("home-clip-C:/clips/penta.mp4"),
    );
  });

  it("1위에만 「최고의 순간」 배지가 붙는다", async () => {
    mockListClips.mockResolvedValue([
      clip({ file_path: "C:/clips/penta.mp4", highlight_score: 88 }),
      clip({ file_path: "C:/clips/kill.mp4", highlight_score: 40 }),
    ]);

    render(<Home />);

    await screen.findByTestId("home-clip-grid");
    // 정렬만으로는 1위와 2위가 같은 무게로 읽힌다. 배지는 정확히 하나.
    await waitFor(() =>
      expect(screen.getAllByTestId("home-clip-top")).toHaveLength(1),
    );
    const grid = screen.getByTestId("home-clip-grid");
    expect(grid.children[0]).toContainElement(
      screen.getByTestId("home-clip-top"),
    );
  });

  it("점수가 없는 예전 클립은 우선순위로 같은 눈금에 올린다", async () => {
    // `highlight_score` 가 붙기 전 클립. 점수 0 취급하면 새 클립 뒤로 전부
    // 밀려 예전 판이 통째로 뒤집힌다 — 백엔드와 같은 폴백(priority * 20)을 쓴다.
    mockListClips.mockResolvedValue([
      clip({ file_path: "C:/clips/scored.mp4", highlight_score: 30 }),
      clip({ file_path: "C:/clips/legacy.mp4", priority: 5 }), // 5 * 20 = 100
    ]);

    render(<Home />);

    const grid = await screen.findByTestId("home-clip-grid");
    await waitFor(() => expect(grid.children).toHaveLength(2));
    expect(grid.children[0]).toContainElement(
      screen.getByTestId("home-clip-C:/clips/legacy.mp4"),
    );
  });
});

describe("Home — 어느 판인지", () => {
  it("판 메타데이터가 있으면 챔피언·승패를 머리에 건다", async () => {
    mockGetGameMetadata.mockResolvedValue({
      game_id: "game-1",
      champion: "Tryndamere",
      result: "Win",
      kda: { kills: 12, deaths: 2, assists: 4 },
    });
    mockListClips.mockResolvedValue([clip()]);

    render(<Home />);

    const header = await screen.findByTestId("home-game-summary");
    expect(header.textContent).toContain("Tryndamere");
    expect(header.textContent).toContain("game.result.win");
    expect(header.textContent).toContain("12");
  });

  it("메타데이터를 못 얻어도 클립은 보인다", async () => {
    // 헤더 하나가 화면 전체를 막으면 안 된다.
    mockGetGameMetadata.mockRejectedValue(new Error("boom"));
    mockListClips.mockResolvedValue([clip()]);

    render(<Home />);

    await screen.findByTestId("home-clip-C:/clips/a.mp4");
    expect(screen.queryByTestId("home-game-summary")).not.toBeInTheDocument();
  });

  it("판이 없는 것과 판은 있는데 클립이 0개인 것은 다른 문구다", async () => {
    mockListClips.mockResolvedValue([]);

    const { unmount } = render(<Home />);
    // 판은 있는데 담긴 게 없다 -> 문턱을 낮추라고 안내해야 한다.
    let empty = await screen.findByTestId("home-empty");
    expect(empty.textContent).toContain("home.empty.noClips.title");
    unmount();

    mockListGames.mockResolvedValue([]);
    render(<Home />);
    empty = await screen.findByTestId("home-empty");
    expect(empty.textContent).toContain("home.empty.title");
    expect(empty.textContent).not.toContain("home.empty.noClips");
  });
});

describe("Home — 선택하고 나서 무엇이 일어나는가", () => {
  it("「하이라이트 영상 만들기」가 이 판을 들고 자동편집으로 간다", async () => {
    // 예전에는 navigate({to:"/auto-edit"}) 뿐이라 빈 화면이 열렸다.
    mockListClips.mockResolvedValue([clip()]);

    render(<Home />);

    const button = await screen.findByTestId("home-make-highlight");
    button.click();

    expect(mockSetSelectedGameId).toHaveBeenCalledWith("game-1");
    expect(mockNavigate).toHaveBeenCalledWith({
      to: "/auto-edit",
    });
    expect(mockSetSelectedGameIds).toHaveBeenCalledWith(["game-1"]);
  });

  it("「다듬기」도 같은 판을 들고 편집기로 간다", async () => {
    mockListClips.mockResolvedValue([clip()]);

    render(<Home />);

    const button = await screen.findByTestId("home-trim");
    button.click();

    expect(mockNavigate).toHaveBeenCalledWith({
      to: "/editor",
      search: { gameId: "game-1" },
    });
  });

  it("고른 클립을 화면에 보이는 순서(점수순)로 넘긴다", async () => {
    // 카드의 체크박스가 오랫동안 아무 일도 하지 않는 장식이었다.
    mockListClips.mockResolvedValue([
      clip({ file_path: "C:/clips/kill.mp4", highlight_score: 40 }),
      clip({ file_path: "C:/clips/penta.mp4", highlight_score: 88 }),
    ]);

    render(<Home />);

    // 클릭 순서는 낮은 점수부터 — 넘길 때는 점수순으로 뒤집혀야 한다.
    fireEvent.click(
      await screen.findByTestId("home-clip-select-C:/clips/kill.mp4"),
    );
    fireEvent.click(screen.getByTestId("home-clip-select-C:/clips/penta.mp4"));
    fireEvent.click(screen.getByTestId("home-make-highlight"));

    expect(mockSetPinnedClips).toHaveBeenLastCalledWith({
      groups: [
        {
          gameId: "game-1",
          paths: ["C:/clips/penta.mp4", "C:/clips/kill.mp4"],
        },
      ],
    });
  });

  it("고른 게 없으면 남은 선택을 눕혀 자동 선택으로 되돌린다", async () => {
    // 지난 방문의 선택이 남아 이번 영상을 조용히 제한하면 안 된다.
    mockListClips.mockResolvedValue([clip()]);

    render(<Home />);

    (await screen.findByTestId("home-make-highlight")).click();

    expect(mockSetPinnedClips).toHaveBeenLastCalledWith(null);
  });
});
