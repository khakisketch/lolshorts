import { render, screen, waitFor } from '@testing-library/react';
import { Home } from './Home';
import type { ClipMetadata } from '@/types/storage';

// i18n: 실제 문구가 아니라 키+보간 값을 그대로 뱉게 해 "어떤 문구가 어떤 값으로
// 렌더됐는지" 를 단언한다. 문구 자체는 translation.json 이 SSOT 다.
jest.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}(${JSON.stringify(params)})` : key,
  }),
}));

jest.mock('@tanstack/react-router', () => ({
  // 라우터 Link 는 렌더만 되면 된다 — span 으로 두어야 a11y 린트가
  // href 없는 앵커를 잡지 않는다(테스트 더블이지 실제 링크가 아니다).
  Link: ({ children }: { children: React.ReactNode }) => <span>{children}</span>,
  useNavigate: () => jest.fn(),
}));

jest.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (p: string) => `asset://${p}`,
}));

jest.mock('@/components/ui/use-toast', () => ({
  useToast: () => ({ toast: jest.fn() }),
}));

jest.mock('@/api/lcu', () => ({
  lcuApi: {
    getUnifiedGameStatus: jest.fn().mockResolvedValue({
      lcu_connected: true,
      in_game: false,
      is_recording: false,
      is_monitoring: true,
    }),
  },
}));

jest.mock('@/api/recording', () => ({
  recordingApi: {
    startAutoCapture: jest.fn().mockResolvedValue(undefined),
    stopAutoCapture: jest.fn().mockResolvedValue(undefined),
  },
}));

jest.mock('@/api/video', () => ({
  videoApi: { generateClipThumbnail: jest.fn().mockResolvedValue(null) },
}));

const mockListClips = jest.fn();
jest.mock('@/api/storage', () => ({
  storageApi: {
    listGames: jest.fn().mockResolvedValue(['game-1']),
    listClips: (...args: unknown[]) => mockListClips(...args),
  },
}));

function clip(overrides: Partial<ClipMetadata> = {}): ClipMetadata {
  return {
    file_path: 'C:/clips/a.mp4',
    thumbnail_path: 'C:/clips/a.jpg',
    event_type: { multikill: 3 },
    event_time: 600,
    priority: 3,
    duration: 13,
    created_at: '2026-07-30T00:00:00Z',
    usage_count: 0,
    ...overrides,
  };
}

beforeEach(() => {
  jest.clearAllMocks();
});

/**
 * 이 앱이 확언할 수 있는 유일한 것은 **그 순간의 게임 상태**다. 경쟁 서비스는
 * 화면 픽셀을 읽어 추정하므로 "체력 8% 였다" 를 확언할 수 없지만 우리는 Live
 * Client Data API 로 직접 받는다 — 그런데 그 값이 저장만 되고 화면에는 한 번도
 * 나오지 않았다. 차별점이 화면에 없으면 없는 것과 같다.
 */
describe('Home — 클립이 왜 뽑혔는지', () => {
  it('점수 이유를 사람 말로 카드에 보여준다', async () => {
    mockListClips.mockResolvedValue([
      clip({ score_reasons: [{ Clutch: 8 }, 'Solo'] }),
    ]);

    render(<Home />);

    const line = await screen.findByTestId('home-clip-reasons-C:/clips/a.mp4');
    // 눈에 띄는 것부터 · 로 이어 붙인다.
    expect(line.textContent).toBe(
      'clip.reason.clutch({"percent":8}) · clip.reason.solo',
    );
  });

  it('숫자 점수는 화면에 내보내지 않는다', async () => {
    // "37.5점" 은 게이머에게 아무 뜻이 없다. 정렬에만 쓰고 사람에게는 이유를 준다.
    mockListClips.mockResolvedValue([
      clip({ highlight_score: 37.5, score_reasons: ['Solo'] }),
    ]);

    render(<Home />);

    await screen.findByTestId('home-clip-reasons-C:/clips/a.mp4');
    expect(screen.queryByText(/37\.5/)).not.toBeInTheDocument();
  });

  it('이유가 없는 예전 클립은 그 줄 자체가 없다', async () => {
    // `score_reasons` 가 붙기 전에 저장된 클립. 빈 줄이 남으면 카드 높이가
    // 들쭉날쭉해져 격자가 어긋난다.
    mockListClips.mockResolvedValue([clip({ score_reasons: [] }), clip({ file_path: 'C:/clips/b.mp4' })]);

    render(<Home />);

    await waitFor(() => {
      expect(
        screen.getByTestId('home-clip-C:/clips/a.mp4'),
      ).toBeInTheDocument();
    });
    expect(
      screen.queryByTestId('home-clip-reasons-C:/clips/a.mp4'),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId('home-clip-reasons-C:/clips/b.mp4'),
    ).not.toBeInTheDocument();
  });

  it('모르는 이유 모양은 코드값으로 새어 나가지 않는다', async () => {
    // 백엔드가 변형을 늘렸는데 매핑이 없는 경우. 클립 이름이 한국어 UI 에
    // `Shutdown` 으로 나갔던 전력이 있어 같은 경로를 막아 둔다.
    mockListClips.mockResolvedValue([
      clip({
        score_reasons: [
          'Unheard' as never,
          'Solo',
        ],
      }),
    ]);

    render(<Home />);

    const line = await screen.findByTestId('home-clip-reasons-C:/clips/a.mp4');
    expect(line.textContent).toBe('clip.reason.solo');
    expect(line.textContent).not.toContain('Unheard');
  });
});
