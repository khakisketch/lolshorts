import { ReactElement, useState } from "react";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import {
  BasicSettings,
  detectQualityLevel,
  detectSoundMode,
} from "./BasicSettings";
import {
  EVENT_FILTER_DEFAULTS,
  filtersToPreset,
  HIGHLIGHT_PRESET_FILTERS,
} from "./highlightPreset";
import type { RecordingSettings } from "@/types";

// `t` 는 렌더마다 새로 만들지 않는다 — 소비하는 쪽이 [t] 로 메모이즈할 때
// 무한 루프를 만들기 때문에, 테스트도 실제와 같은 안정된 identity 를 준다.
jest.mock("react-i18next", () => {
  const t = (key: string) => key;
  return { useTranslation: () => ({ t }) };
});

const mockDataDir = jest.fn();
const mockJoin = jest.fn();
jest.mock("@tauri-apps/api/path", () => ({
  dataDir: () => mockDataDir(),
  join: (...parts: string[]) => mockJoin(...parts),
}));

const mockOpenPath = jest.fn();
jest.mock("@tauri-apps/plugin-shell", () => ({
  open: (path: string) => mockOpenPath(path),
}));

const mockToast = jest.fn();
jest.mock("@/components/ui/use-toast", () => ({
  useToast: () => ({ toast: mockToast }),
}));

const baseSettings: RecordingSettings = {
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
    system_audio_device: null,
    system_audio_volume: 100,
    sample_rate: "hz48000",
    bitrate: "kbps192",
  },
  // 백엔드가 실제로 돌려주는 모양(숨은 필드 포함)을 그대로 쓴다.
  event_filter: HIGHLIGHT_PRESET_FILTERS.balanced,
  game_mode: {
    record_ranked_solo: true,
    record_ranked_flex: true,
    record_normal: true,
    record_quick_play: true,
    record_aram: true,
    record_arena: true,
    record_special: false,
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

/** 부모가 상태를 들고 있는 실제 사용 형태 — 기본/고급 양방향 동기화를 이걸로 본다. */
function Harness({
  initial = baseSettings,
  onSettings,
}: {
  initial?: RecordingSettings;
  onSettings?: (settings: RecordingSettings) => void;
}) {
  const [settings, setSettings] = useState<RecordingSettings>(initial);
  return (
    <>
      <BasicSettings
        settings={settings}
        onChange={(next) => {
          setSettings(next);
          onSettings?.(next);
        }}
      />
      {/* 고급 설정에서 개별 토글을 바꾸는 상황을 대신하는 버튼 */}
      <button
        type="button"
        onClick={() =>
          setSettings((current) => ({
            ...current,
            event_filter: { ...current.event_filter, record_deaths: true },
          }))
        }
      >
        advanced-toggle-deaths
      </button>
      <button
        type="button"
        onClick={() =>
          setSettings((current) => ({
            ...current,
            video: { ...current.video, frame_rate: "fps144" },
          }))
        }
      >
        advanced-change-fps
      </button>
      <div data-testid="state">{JSON.stringify(settings)}</div>
    </>
  );
}

const readState = (): RecordingSettings =>
  JSON.parse(screen.getByTestId("state").textContent || "{}");

/** 렌더 직후 저장 경로 조회(마이크로태스크)를 흘려보내 act 경고를 막는다. */
const renderUI = async (ui: ReactElement) => {
  render(ui);
  await act(async () => {});
};

const click = (element: HTMLElement) => {
  act(() => {
    fireEvent.click(element);
  });
};

const typeNumber = (element: HTMLElement, value: string) => {
  act(() => {
    fireEvent.change(element, { target: { value } });
  });
};

describe("BasicSettings", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockDataDir.mockResolvedValue("C:\\Users\\test\\AppData\\Roaming");
    mockJoin.mockImplementation((...parts: string[]) =>
      Promise.resolve(parts.join("\\")),
    );
    mockOpenPath.mockResolvedValue(undefined);
  });

  it("shows exactly the five basic cards", async () => {
    await renderUI(<Harness />);

    expect(screen.getByTestId("basic-highlights")).toBeInTheDocument();
    expect(screen.getByTestId("basic-quality")).toBeInTheDocument();
    expect(screen.getByTestId("basic-sound")).toBeInTheDocument();
    expect(screen.getByTestId("basic-storage")).toBeInTheDocument();
    expect(screen.getByTestId("basic-auto-start")).toBeInTheDocument();

    await waitFor(() => expect(mockDataDir).toHaveBeenCalled());
  });

  /**
   * 프리셋으로 큰 틀을 잡고 세부는 같은 카드에서 바로 바꾼다.
   *
   * 예전에는 이 자리가 읽기 전용 표라, 프레임 하나 바꾸려면 아래 고급 설정까지
   * 스크롤해서 같은 항목을 다시 찾아야 했다.
   */
  it("화질 세부값을 카드 안에서 바로 바꿀 수 있다", async () => {
    await renderUI(<Harness />);
    expect(screen.getByTestId("quality-fps")).toBeInTheDocument();
    expect(screen.getByTestId("quality-bitrate")).toBeInTheDocument();
  });

  /** 녹화 크기는 Windows 캡처가 무시하는 값이라 드롭다운이 되면 안 된다. */
  it("녹화 크기는 고를 수 있는 컨트롤이 아니다", async () => {
    await renderUI(<Harness />);
    expect(
      screen.queryByTestId("quality-capture-size"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("settings.basic.quality.specs.captureSizeValue"),
    ).toBeInTheDocument();
  });

  it("does not expose codec or encoder on the basic screen", async () => {
    await renderUI(<Harness />);

    const basic = screen.getByTestId("basic-settings");
    expect(basic.textContent).not.toMatch(/codec/i);
    expect(basic.textContent).not.toMatch(/encoder/i);
    expect(basic.textContent).not.toMatch(/h26[45]/i);
  });

  describe("what to capture", () => {
    it("marks the current preset as selected", async () => {
      await renderUI(<Harness />);

      expect(
        screen.getByRole("radio", {
          name: "settings.basic.highlights.options.balanced.label",
        }),
      ).toBeChecked();
    });

    it("writes the canonical toggle combination when a preset is picked", async () => {
      await renderUI(<Harness />);

      click(
        screen.getByRole("radio", {
          name: "settings.basic.highlights.options.best_only.label",
        }),
      );

      expect(readState().event_filter).toEqual(
        HIGHLIGHT_PRESET_FILTERS.best_only,
      );
    });

    it('falls back to "직접 설정" when an advanced toggle deviates', async () => {
      await renderUI(<Harness />);

      expect(
        screen.queryByTestId("highlights-custom-hint"),
      ).not.toBeInTheDocument();

      click(screen.getByText("advanced-toggle-deaths"));

      expect(screen.getByTestId("highlights-custom-hint")).toBeInTheDocument();
      expect(
        screen.getByRole("radio", {
          name: "settings.basic.highlights.options.balanced.label",
        }),
      ).not.toBeChecked();
      expect(filtersToPreset(readState().event_filter)).toBe("custom");
    });

    it("re-bundles a custom combination back into a preset", async () => {
      await renderUI(<Harness />);

      click(screen.getByText("advanced-toggle-deaths"));
      click(
        screen.getByRole("radio", {
          name: "settings.basic.highlights.options.balanced.label",
        }),
      );

      expect(readState().event_filter.record_deaths).toBe(false);
      expect(
        screen.queryByTestId("highlights-custom-hint"),
      ).not.toBeInTheDocument();
    });
  });

  describe("quality", () => {
    it("maps a level onto frame rate and bitrate without changing the unused Windows resolution", async () => {
      await renderUI(<Harness />);

      click(
        screen.getByRole("radio", {
          name: "settings.basic.quality.options.high.label",
        }),
      );

      expect(readState().video).toEqual({
        resolution: "r1920x1080",
        frame_rate: "fps60",
        bitrate_preset: "high",
        codec: "h264",
        encoder: "auto",
      });
    });

    it("ignores serialized resolution when judging a Windows quality preset", () => {
      expect(
        detectQualityLevel({
          resolution: "r3840x2160",
          frame_rate: "fps60",
          bitrate_preset: "medium",
        }),
      ).toBe("medium");
    });

    it("shows custom when the advanced video panel deviates", async () => {
      await renderUI(<Harness />);

      click(screen.getByText("advanced-change-fps"));

      expect(
        screen.getByRole("radio", {
          name: "settings.basic.quality.options.medium.label",
        }),
      ).not.toBeChecked();
      expect(detectQualityLevel(readState().video)).toBe("custom");
    });
  });

  describe("sound", () => {
    it("maps the three choices onto the two audio switches", async () => {
      await renderUI(<Harness />);

      click(
        screen.getByRole("radio", {
          name: "settings.basic.sound.options.gameMic.label",
        }),
      );
      expect(readState().audio.record_microphone).toBe(true);
      expect(readState().audio.record_system_audio).toBe(true);

      click(
        screen.getByRole("radio", {
          name: "settings.basic.sound.options.mute.label",
        }),
      );
      expect(readState().audio.record_microphone).toBe(false);
      expect(readState().audio.record_system_audio).toBe(false);
    });

    it("keeps device and volume choices made in the advanced panel", async () => {
      await renderUI(
        <Harness
          initial={{
            ...baseSettings,
            audio: {
              ...baseSettings.audio,
              microphone_device: "Blue Yeti",
              microphone_volume: 60,
            },
          }}
        />,
      );

      click(
        screen.getByRole("radio", {
          name: "settings.basic.sound.options.gameMic.label",
        }),
      );

      expect(readState().audio.microphone_device).toBe("Blue Yeti");
      expect(readState().audio.microphone_volume).toBe(60);
    });

    it("reports a mic-only combination as custom instead of lying", async () => {
      expect(
        detectSoundMode({
          record_system_audio: false,
          record_microphone: true,
        }),
      ).toBe("custom");
    });
  });

  describe("storage", () => {
    it("shows the resolved recordings path and opens it", async () => {
      await renderUI(<Harness />);

      await waitFor(() =>
        expect(screen.getByTestId("storage-path")).toHaveTextContent(
          "C:\\Users\\test\\AppData\\Roaming\\lolshorts\\recordings",
        ),
      );

      click(screen.getByTestId("storage-open-folder"));
      expect(mockOpenPath).toHaveBeenCalledWith(
        "C:\\Users\\test\\AppData\\Roaming\\lolshorts\\recordings",
      );
    });

    it("falls back to a message when the path cannot be resolved", async () => {
      mockDataDir.mockRejectedValue(new Error("not a tauri window"));
      await renderUI(<Harness />);

      await waitFor(() =>
        expect(screen.getByTestId("storage-path")).toHaveTextContent(
          "settings.basic.storage.pathUnknown",
        ),
      );
      expect(screen.getByTestId("storage-open-folder")).toBeDisabled();
    });

    it("keeps the limit inert-looking until automatic cleanup is on", async () => {
      await renderUI(<Harness />);

      const limit = screen.getByLabelText("settings.basic.storage.limitLabel");
      expect(limit).toBeDisabled();
      expect(screen.getByTestId("basic-storage")).toHaveTextContent(
        "settings.basic.storage.limitDisabledHint",
      );

      click(screen.getByLabelText("settings.basic.storage.autoCleanupLabel"));
      expect(readState().storage.auto_delete_enabled).toBe(true);
    });

    it("clamps the limit into the range the backend accepts", async () => {
      await renderUI(
        <Harness
          initial={{
            ...baseSettings,
            storage: { ...baseSettings.storage, auto_delete_enabled: true },
          }}
        />,
      );

      const limit = screen.getByLabelText("settings.basic.storage.limitLabel");
      typeNumber(limit, "0");
      expect(readState().storage.max_storage_gb).toBe(1);

      typeNumber(
        screen.getByLabelText("settings.basic.storage.limitLabel"),
        "99999",
      );
      expect(readState().storage.max_storage_gb).toBe(10000);
    });
  });

  it("toggles launch on Windows startup", async () => {
    await renderUI(<Harness />);

    click(screen.getByLabelText("settings.basic.autoStart.label"));
    expect(readState().launch_on_windows_startup).toBe(false);
  });

  it("disables the controls while a save is in flight", async () => {
    await renderUI(
      <BasicSettings settings={baseSettings} onChange={jest.fn()} disabled />,
    );

    expect(
      screen.getByRole("radio", {
        name: "settings.basic.highlights.options.balanced.label",
      }),
    ).toBeDisabled();
    expect(
      screen.getByLabelText("settings.basic.autoStart.label"),
    ).toBeDisabled();
  });

  it("treats a settings payload without the hidden filter fields as custom", async () => {
    // 미러가 아는 필드가 빠져 있으면 "균형"이라고 단정하지 않는다.
    const partial = { ...EVENT_FILTER_DEFAULTS } as Partial<
      typeof EVENT_FILTER_DEFAULTS
    >;
    delete partial.record_outplay;
    expect(filtersToPreset(partial)).toBe("custom");
  });
});
