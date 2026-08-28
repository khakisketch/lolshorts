import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { Settings } from "./Settings";
import { HIGHLIGHT_PRESET_FILTERS } from "@/components/settings/highlightPreset";
import type { RecordingSettings } from "../types";

// Mock i18n. `t` must keep a stable identity: Settings memoises its loaders on
// [t, toast], so a fresh function per render would re-run the load effect on every
// render — an infinite reload loop that also stomps on state changes under test.
jest.mock("react-i18next", () => {
  const t = (key: string) => key;
  return { useTranslation: () => ({ t }) };
});

// Mock auth store
const mockUseAuthStore = jest.fn();

jest.mock("@/lib/auth", () => ({
  useAuthStore: () => mockUseAuthStore(),
}));

// Mock settings API
const mockGetRecordingSettings = jest.fn();
const mockSaveRecordingSettings = jest.fn();
const mockResetToDefault = jest.fn();

jest.mock("@/api/settings", () => ({
  settingsApi: {
    getAutostartStatus: jest.fn(),
    getRecordingSettings: () => mockGetRecordingSettings(),
    saveRecordingSettings: (settings: unknown) =>
      mockSaveRecordingSettings(settings),
    resetToDefault: () => mockResetToDefault(),
  },
}));

// Mock auth API
jest.mock("@/api/auth", () => ({
  authApi: {
    getCurrentEntitlement: jest.fn().mockResolvedValue({
      tier: "FREE",
      status: "active",
      expires_at: null,
      source: "supabase",
      checked_at: "2026-01-01T00:00:00Z",
      payment_available: false,
    }),
  },
}));

// Mock utils
jest.mock("@/lib/utils", () => ({
  cn: (...args: unknown[]) => args.filter(Boolean).join(" "),
  pageStyles: {
    container: "container",
    title: "title",
  },
}));

// BasicSettings resolves the storage path through Tauri on mount.
jest.mock("@tauri-apps/api/path", () => ({
  dataDir: jest.fn(() => new Promise(() => undefined)),
  join: jest.fn((...parts: string[]) => Promise.resolve(parts.join("\\"))),
}));
jest.mock("@tauri-apps/plugin-shell", () => ({
  open: jest.fn().mockResolvedValue(undefined),
}));

// Mock components
jest.mock("@/components/auth", () => ({
  AuthModal: () => null,
}));
jest.mock("@/components/PaymentModal", () => ({
  PaymentModal: () => null,
}));
jest.mock("@/components/SubscriptionManagement", () => ({
  SubscriptionManagement: () => null,
}));
// 고급의 이벤트 패널은 "개별 토글 하나를 바꾸는" 동작만 흉내낸다 — 기본 화면이
// 그 변화를 따라오는지(양방향 동기화) 보기 위한 것이다.
jest.mock("@/components/settings/EventFilterSettings", () => ({
  EventFilterSettings: ({
    settings,
    onChange,
  }: {
    settings: Record<string, unknown>;
    onChange: (next: Record<string, unknown>) => void;
  }) => (
    <div>
      <div>Event Filter Settings</div>
      <button
        type="button"
        onClick={() => onChange({ ...settings, record_deaths: true })}
      >
        toggle-record-deaths
      </button>
    </div>
  ),
}));
jest.mock("@/components/settings/GameModeSettings", () => ({
  GameModeSettings: () => <div>Game Mode Settings</div>,
}));
jest.mock("@/components/settings/VideoSettings", () => ({
  VideoSettings: () => <div>Video Settings</div>,
}));
jest.mock("@/components/settings/AudioSettings", () => ({
  AudioSettings: () => <div>Audio Settings</div>,
}));
jest.mock("@/components/settings/ClipTimingSettings", () => ({
  ClipTimingSettings: () => <div>Clip Timing Settings</div>,
}));
jest.mock("@/components/settings/HotkeySettings", () => ({
  HotkeySettings: () => <div>Hotkey Settings</div>,
}));
jest.mock("@/components/settings/LanguageSelector", () => ({
  LanguageSelector: () => <div>Language Selector</div>,
}));
jest.mock("@/components/settings/DiagnosticsSection", () => ({
  DiagnosticsSection: () => (
    <div data-testid="diagnostics-section">Diagnostics Section</div>
  ),
}));
jest.mock("@/components/settings/GeneralSettings", () => ({
  GeneralSettings: ({
    settings,
  }: {
    settings: {
      show_replay_popup: boolean;
      crash_reporting_enabled: boolean;
      overlay_enabled: boolean;
      minimize_to_tray: boolean;
      show_notifications: boolean;
    };
  }) => (
    <div>
      <div>General Settings</div>
      <div data-testid="general-settings-fixture-shape">
        {JSON.stringify({
          show_replay_popup: settings.show_replay_popup,
          crash_reporting_enabled: settings.crash_reporting_enabled,
          overlay_enabled: settings.overlay_enabled,
          minimize_to_tray: settings.minimize_to_tray,
          show_notifications: settings.show_notifications,
        })}
      </div>
    </div>
  ),
}));

const defaultSettings: RecordingSettings = {
  schema_version: 4,
  video: {
    resolution: "r1920x1080",
    frame_rate: "fps60",
    bitrate_preset: "medium",
    codec: "h264",
    encoder: "auto",
  },
  audio: {
    record_microphone: false,
    microphone_device: null,
    microphone_volume: 100,
    record_system_audio: true,
    system_audio_device: "default",
    system_audio_volume: 100,
    sample_rate: "hz48000",
    bitrate: "kbps192",
  },
  // 백엔드가 돌려주는 모양 그대로(숨은 필드 포함) — 프리셋 판정이 성립하려면 필요하다.
  event_filter: HIGHLIGHT_PRESET_FILTERS.balanced,
  game_mode: {
    record_ranked_solo: true,
    record_ranked_flex: true,
    record_normal: true,
    record_quick_play: true,
    record_aram: true,
    record_arena: true,
    record_special: true,
    record_custom: false,
    record_practice: false,
  },
  clip_timing: {
    default_pre_duration: 15,
    default_post_duration: 5,
    event_timings: {},
    merge_consecutive_events: true,
    merge_time_threshold: 10,
  },
  hotkeys: {
    toggle_recording: "F8",
    manual_save_clip: "F9",
    delete_last_clip: "F10",
  },
  storage: {
    auto_delete_enabled: false,
    auto_delete_days: 30,
    max_storage_gb: 50,
    delete_exported_clips: false,
  },
  launch_on_windows_startup: true,
  minimize_to_tray: true,
  show_notifications: true,
  show_replay_popup: true,
  crash_reporting_enabled: false,
  overlay_enabled: true,
};

/** 왼쪽 카테고리에서 한 칸을 고른다. */
const goTo = async (section: string) => {
  const item = await screen.findByTestId(`settings-nav-${section}`);
  act(() => {
    fireEvent.click(item);
  });
};

describe("Settings", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockGetRecordingSettings.mockResolvedValue(defaultSettings);
    mockSaveRecordingSettings.mockResolvedValue(undefined);
    mockUseAuthStore.mockReturnValue({
      user: null,
      isAuthenticated: false,
    });
  });

  describe("Basic Rendering", () => {
    it("should render settings page title", async () => {
      render(<Settings />);

      await waitFor(() => {
        expect(screen.getByText("settings.title")).toBeInTheDocument();
      });
    });

    /**
     * 설정은 2단이다 — 왼쪽 카테고리, 오른쪽 그 칸의 내용.
     *
     * 예전에는 다섯 카드가 한 화면에 다 쌓이고 그 아래 고급 설정이 접혀 있어서,
     * 1280x800 에서 3화면분이었다. 뭘 찾으려면 무조건 스크롤해야 했다.
     */
    it("처음에는 영상·화질 칸만 보인다", async () => {
      render(<Settings />);

      expect(await screen.findByTestId("basic-quality")).toBeInTheDocument();
      expect(screen.getByText("Video Settings")).toBeInTheDocument();
      // 다른 칸의 내용은 렌더되지 않는다 — 그게 스크롤을 없애는 방법이다.
      expect(screen.queryByTestId("basic-highlights")).not.toBeInTheDocument();
      expect(screen.queryByTestId("basic-storage")).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("diagnostics-section"),
      ).not.toBeInTheDocument();
    });

    it("카테고리를 고르면 그 칸으로 바뀐다", async () => {
      render(<Settings />);

      await goTo("highlights");
      expect(screen.getByTestId("basic-highlights")).toBeInTheDocument();
      expect(screen.getByText("Event Filter Settings")).toBeInTheDocument();
      expect(screen.getByText("Clip Timing Settings")).toBeInTheDocument();
      expect(screen.queryByTestId("basic-quality")).not.toBeInTheDocument();

      await goTo("sound");
      expect(screen.getByTestId("basic-sound")).toBeInTheDocument();
      expect(screen.getByText("Audio Settings")).toBeInTheDocument();

      await goTo("storage");
      expect(screen.getByTestId("basic-storage")).toBeInTheDocument();

      await goTo("app");
      expect(screen.getByTestId("basic-auto-start")).toBeInTheDocument();
      expect(screen.getByText("Language Selector")).toBeInTheDocument();

      await goTo("hotkeys");
      expect(screen.getByText("Hotkey Settings")).toBeInTheDocument();
    });

    /**
     * 홈에서 「수동 리플레이 저장」을 뺐을 때 갈 곳을 만들어 둔 칸이다.
     * 이 칸이 사라지면 F8/F9/F10 을 조절할 화면이 없어진다.
     */
    it("수동 저장·단축키 칸이 존재한다", async () => {
      render(<Settings />);
      expect(
        await screen.findByTestId("settings-nav-hotkeys"),
      ).toBeInTheDocument();
    });

    /**
     * 카테고리를 나눠도 각 칸은 여전히 길었다. 프레임·비트레이트를 위 카드에서
     * 드롭다운으로 고르게 해 놓고, **바로 아래 상세 설정에 같은 항목이 또** 나오기
     * 때문이다. 같은 값이 두 번 보이면 어느 쪽이 진짜인지 알 수 없어 결국 둘 다
     * 스크롤로 훑게 된다 — 대표자가 지적한 "스크롤 스트레스"의 정체다.
     *
     * 지우지 않고 접는다. `open` 이 붙는 순간 이 단언이 깨진다.
     */
    it("고급 설정은 기본으로 접혀 있다", async () => {
      render(<Settings />);

      const video = await screen.findByTestId("advanced-video");
      expect(video).not.toHaveAttribute("open");
      // 지운 게 아니라 접은 것이다 — 안의 항목은 그대로 있다.
      expect(screen.getByText("Video Settings")).toBeInTheDocument();

      await goTo("highlights");
      expect(screen.getByTestId("advanced-highlights")).not.toHaveAttribute(
        "open",
      );

      await goTo("sound");
      expect(screen.getByTestId("advanced-sound")).not.toHaveAttribute("open");
    });

    /** 초기화는 고급 설정이 사라지면서 진단 칸으로 옮겼다 — 잃어버리지 않았는지. */
    it("초기화 버튼이 진단 칸에 남아 있다", async () => {
      render(<Settings />);
      await goTo("diagnostics");
      expect(screen.getByTestId("diagnostics-section")).toBeInTheDocument();
      expect(screen.getByTestId("settings-reset")).toBeInTheDocument();
    });
  });

  describe("Basic ↔ advanced sync", () => {
    it("writes the canonical toggle set when a basic preset is picked", async () => {
      render(<Settings />);
      await goTo("highlights");

      const bestOnly = await screen.findByRole("radio", {
        name: "settings.basic.highlights.options.best_only.label",
      });
      act(() => {
        fireEvent.click(bestOnly);
      });

      await waitFor(() =>
        expect(
          screen.getByRole("radio", {
            name: "settings.basic.highlights.options.best_only.label",
          }),
        ).toBeChecked(),
      );
      expect(mockSaveRecordingSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          event_filter: HIGHLIGHT_PRESET_FILTERS.best_only,
        }),
      );
    });

    it('falls back to "직접 설정" when an advanced toggle deviates', async () => {
      render(<Settings />);
      await goTo("highlights");

      expect(
        await screen.findByRole("radio", {
          name: "settings.basic.highlights.options.balanced.label",
        }),
      ).toBeChecked();

      await goTo("highlights");
      act(() => {
        fireEvent.click(screen.getByText("toggle-record-deaths"));
      });

      expect(
        await screen.findByTestId("highlights-custom-hint"),
      ).toBeInTheDocument();
      expect(mockSaveRecordingSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          event_filter: expect.objectContaining({ record_deaths: true }),
        }),
      );
    });
  });

  describe("Authentication States", () => {
    it("should show login prompt for license when not authenticated", async () => {
      mockUseAuthStore.mockReturnValue({
        user: null,
        isAuthenticated: false,
      });

      render(<Settings />);
      // 구독 패널은 이제 「계정 > 구독·라이선스」 칸 안에 있다. 예전에는 어느
      // 칸을 보고 있든 화면 맨 아래에 딸려 나와서, 녹화 설정을 만지는 내내
      // 결제 안내를 스크롤로 지나쳐야 했다.
      await goTo("license");

      await waitFor(() => {
        expect(
          screen.getByText("settings.license.loginRequired"),
        ).toBeInTheDocument();
      });
    });

    it("should load license info when authenticated", async () => {
      mockUseAuthStore.mockReturnValue({
        user: { id: "user1", email: "test@example.com", tier: "FREE" },
        isAuthenticated: true,
      });

      render(<Settings />);
      await goTo("license");

      await waitFor(() => {
        expect(screen.getByText("settings.license.title")).toBeInTheDocument();
      });
    });
  });

  describe("Settings Loading", () => {
    it("should load recording settings on mount", async () => {
      render(<Settings />);

      await waitFor(() => {
        expect(mockGetRecordingSettings).toHaveBeenCalled();
      });
    });

    it("should pass the complete general recording fixture shape", async () => {
      render(<Settings />);
      await goTo("app");

      const fixtureShape = await screen.findByTestId(
        "general-settings-fixture-shape",
      );

      expect(fixtureShape).toHaveTextContent('"show_replay_popup":true');
      expect(fixtureShape).toHaveTextContent('"crash_reporting_enabled":false');
      expect(fixtureShape).toHaveTextContent('"overlay_enabled":true');
      expect(fixtureShape).toHaveTextContent('"minimize_to_tray":true');
      expect(fixtureShape).toHaveTextContent('"show_notifications":true');
    });

    it("should show loading state while settings are loading", () => {
      mockGetRecordingSettings.mockImplementation(() => new Promise(() => {})); // Never resolves

      render(<Settings />);

      expect(
        screen.getByText("settings.recordingConfig.loadingSettings"),
      ).toBeInTheDocument();
    });

    it("should offer a retry when settings fail to load", async () => {
      mockGetRecordingSettings.mockRejectedValue(new Error("nope"));

      render(<Settings />);

      expect(
        await screen.findByText("settings.recordingConfig.loadError"),
      ).toBeInTheDocument();
      expect(screen.queryByTestId("basic-settings")).not.toBeInTheDocument();
    });
  });

  describe("Account Section", () => {
    it("should display account info when authenticated", async () => {
      mockUseAuthStore.mockReturnValue({
        user: { id: "user123456", email: "test@example.com", tier: "PRO" },
        isAuthenticated: true,
      });

      render(<Settings />);
      await goTo("license");

      await waitFor(() => {
        expect(
          screen.getByText("settings.accountInfo.title"),
        ).toBeInTheDocument();
        // 구독 패널과 계정 패널이 같은 칸에 있으므로 이메일은 두 번 나온다.
        expect(screen.getAllByText("test@example.com").length).toBeGreaterThan(
          0,
        );
      });
    });
  });
});
