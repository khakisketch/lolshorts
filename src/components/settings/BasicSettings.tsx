import { ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { dataDir, join } from "@tauri-apps/api/path";
import { open as openPath } from "@tauri-apps/plugin-shell";
import {
  Film,
  Gauge,
  Gem,
  HardDrive,
  Layers,
  Mic,
  Monitor,
  Scale,
  Sparkles,
  Volume2,
  VolumeX,
  Zap,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useToast } from "@/components/ui/use-toast";
import { logger } from "@/lib/logger";
import type {
  AudioSettings,
  BitratePreset,
  FrameRate,
  RecordingSettings,
  VideoSettings,
} from "@/types";
import {
  applyHighlightPreset,
  DEFAULT_HIGHLIGHT_PRESET,
  filtersToPreset,
  SELECTABLE_HIGHLIGHT_PRESETS,
  type SelectableHighlightPreset,
} from "./highlightPreset";
import {
  BITRATE_MBPS,
  clipLengthSpecs,
  enabledScenes,
  FRAME_RATE_FPS,
  megabytesPerMinute,
  type SceneFlag,
} from "./settingSpecs";
import { evaluateCoverage } from "./captureCoverage";

/** 프리셋별 아이콘. 카드가 셋 나란히 놓이면 글자만으로는 구분이 느리다. */
const ICON_CLASS = "h-6 w-6";

const HIGHLIGHT_ICONS: Record<SelectableHighlightPreset, ReactNode> = {
  everything: <Layers className={ICON_CLASS} />,
  balanced: <Scale className={ICON_CLASS} />,
  best_only: <Gem className={ICON_CLASS} />,
};

const QUALITY_ICONS: Record<QualityLevel, ReactNode> = {
  low: <Gauge className={ICON_CLASS} />,
  medium: <Scale className={ICON_CLASS} />,
  high: <Sparkles className={ICON_CLASS} />,
};

const SOUND_ICONS: Record<SoundMode, ReactNode> = {
  game: <Volume2 className={ICON_CLASS} />,
  gameMic: <Mic className={ICON_CLASS} />,
  mute: <VolumeX className={ICON_CLASS} />,
};

/**
 * 기본 설정 — 사용자가 실제로 판단할 수 있는 5가지만 남긴 화면.
 *
 * 73개 항목이 위계 없이 늘어서 있으면 전부 똑같이 중요해 보이고, 그 중 하나만
 * 틀려도 앱은 아무 말 없이 빈손으로 끝난다(실기기 테스트에서 3건). 그래서 여기서는
 * "무엇을 담을까 / 얼마나 좋은 화질로 / 어떤 소리로 / 어디에 얼마나 / 언제 켜질까"
 * 다섯 질문만 묻고, 나머지 개별 항목은 고급 설정으로 접는다.
 *
 * 개별 항목을 없애지 않았으므로 두 화면은 같은 상태를 본다. 기본에서 고른 묶음은
 * 고급의 개별 값으로 즉시 풀리고, 고급에서 개별 값을 건드리면 기본 화면의 묶음
 * 표시가 "직접 설정"으로 바뀐다.
 */

/** 이 화면이 그릴 수 있는 카드들. */
export type BasicSection =
  | "highlights"
  | "quality"
  | "sound"
  | "storage"
  | "autoStart";

const ALL_SECTIONS: readonly BasicSection[] = [
  "highlights",
  "quality",
  "sound",
  "storage",
  "autoStart",
];

interface BasicSettingsProps {
  settings: RecordingSettings;
  onChange: (settings: RecordingSettings) => void;
  disabled?: boolean;
  /**
   * 그릴 카드를 고른다. 생략하면 전부.
   *
   * 설정 화면이 2단(카테고리 + 내용)으로 나뉘면서, 같은 카드가 서로 다른
   * 카테고리에 흩어져 들어간다 — 화질은 「영상·화질」에, 장면은 「하이라이트」에.
   * 카드를 따로 떼어 쓰기 위한 것이지 새 화면을 만드는 것이 아니다.
   */
  sections?: readonly BasicSection[];
}

type QualityLevel = "high" | "medium" | "low";
type VideoQualityFields = Pick<
  VideoSettings,
  "resolution" | "frame_rate" | "bitrate_preset"
>;

/**
 * 화질 3단. 코덱·인코더는 일부러 묶지 않는다 — 사용자가 판단할 수 있는 축이 아니고
 * (h265 를 고르면 편집기 미리보기가 검은 화면이 되는데 원인을 알 수 없다),
 * 기본값 관리는 백엔드 몫이다. 고급 설정에는 그대로 남아 있다.
 */
const QUALITY_PRESETS: Record<QualityLevel, VideoQualityFields> = {
  high: {
    resolution: "r2560x1440",
    frame_rate: "fps60",
    bitrate_preset: "high",
  },
  medium: {
    resolution: "r1920x1080",
    frame_rate: "fps60",
    bitrate_preset: "medium",
  },
  low: {
    resolution: "r1920x1080",
    frame_rate: "fps30",
    bitrate_preset: "low",
  },
};

const QUALITY_LEVELS: readonly QualityLevel[] = ["high", "medium", "low"];

/**
 * 카드에 놓이는 순서 — 가벼운 것에서 무거운 것으로.
 *
 * `QUALITY_LEVELS`(판정용)와 분리해 둔다. 판정은 순서와 무관하고, 화면 순서는
 * "성능을 지킬수록 왼쪽" 이라는 사용자 멘탈 모델을 따라야 한다.
 */
const QUALITY_DISPLAY_ORDER: readonly QualityLevel[] = ["low", "medium", "high"];

/** 드롭다운에 노출할 프레임. 120/144 는 롤에서 흔치 않지만 고사양 사용자가 있다. */
const FRAME_RATE_OPTIONS: readonly FrameRate[] = ["fps30", "fps60", "fps120", "fps144"];

/** 화질(비트레이트). 숫자를 그대로 보여준다 — "높음" 보다 "40 Mbps" 가 판단에 낫다. */
const BITRATE_OPTIONS: readonly BitratePreset[] = ["low", "medium", "high", "very_high"];

/** `VideoSettings::default()` 가 만드는 조합(60fps + medium)에 대응. */
const RECOMMENDED_QUALITY: QualityLevel = "medium";

type SoundMode = "game" | "gameMic" | "mute";
type SoundFields = Pick<
  AudioSettings,
  "record_system_audio" | "record_microphone"
>;

const SOUND_MODES: Record<SoundMode, SoundFields> = {
  game: { record_system_audio: true, record_microphone: false },
  gameMic: { record_system_audio: true, record_microphone: true },
  mute: { record_system_audio: false, record_microphone: false },
};

const SOUND_ORDER: readonly SoundMode[] = ["game", "gameMic", "mute"];

/** 마이크는 기본 꺼짐(`AudioSettings::default()`), 게임 소리만이 기본이자 추천. */
const RECOMMENDED_SOUND: SoundMode = "game";

/** 백엔드 `dirs::data_dir()/lolshorts/recordings` (src-tauri/src/main.rs) 와 같은 경로. */
const RECORDINGS_DIR_SEGMENTS = ["lolshorts", "recordings"] as const;

export function detectQualityLevel(
  video: VideoQualityFields,
): QualityLevel | "custom" {
  const match = QUALITY_LEVELS.find((level) => {
    const preset = QUALITY_PRESETS[level];
    return (
      preset.resolution === video.resolution &&
      preset.frame_rate === video.frame_rate &&
      preset.bitrate_preset === video.bitrate_preset
    );
  });
  return match ?? "custom";
}

export function detectSoundMode(audio: SoundFields): SoundMode | "custom" {
  const match = SOUND_ORDER.find((mode) => {
    const preset = SOUND_MODES[mode];
    return (
      preset.record_system_audio === audio.record_system_audio &&
      preset.record_microphone === audio.record_microphone
    );
  });
  return match ?? "custom";
}

interface PresetOption {
  value: string;
  label: string;
  description: string;
  icon?: ReactNode;
  /** 한 묶음에 하나만. 카드 위에 "추천" 배지가 붙는다. */
  recommended?: boolean;
}

interface OptionCardsProps {
  name: string;
  value: string;
  options: readonly PresetOption[];
  onValueChange: (value: string) => void;
  disabled?: boolean;
  recommendedLabel: string;
}

/**
 * 프리셋 카드 묶음 — 셋을 나란히 놓고 고르게 한다.
 *
 * 세로 라디오 목록이었을 때의 문제는 "고르는 것"과 "그 결과 무엇이 달라지는지"가
 * 이어지지 않는다는 점이었다. 카드로 나란히 놓으면 선택지가 서로 비교되고, 카드
 * 바로 아래 `SpecTable` 이 고른 값의 실제 수치를 즉시 보여준다.
 *
 * 라디오 자체는 화면에서 숨기되 접근성 트리에는 남긴다(`sr-only`) — 선택 상태는
 * 카드 테두리와 체크 표시로 보이고, 키보드 포커스는 `peer-focus-visible` 로 카드에
 * 옮겨 그린다. 터치 타깃은 카드 전체(≥44px)다.
 */
function OptionCards({
  name,
  value,
  options,
  onValueChange,
  disabled,
  recommendedLabel,
}: OptionCardsProps) {
  return (
    <RadioGroup
      value={value}
      onValueChange={onValueChange}
      disabled={disabled}
      className="grid grid-cols-1 gap-2 sm:grid-cols-3"
      data-testid={`${name}-options`}
    >
      {options.map((option) => {
        const id = `${name}-${option.value}`;
        const selected = value === option.value;
        return (
          <label
            key={option.value}
            htmlFor={id}
            data-selected={selected ? "true" : undefined}
            className={[
              "relative flex min-h-[44px] cursor-pointer flex-col items-center gap-1.5 rounded-lg border p-4 text-center transition-colors",
              selected
                ? "border-gaming-cyan bg-gaming-cyan/10"
                : "border-white/5 bg-white/[0.02] hover:border-gaming-cyan/40",
              disabled ? "cursor-not-allowed opacity-60" : "",
            ].join(" ")}
          >
            <RadioGroupItem
              value={option.value}
              id={id}
              aria-label={option.label}
              aria-describedby={`${id}-description`}
              className="peer sr-only"
            />
            {option.recommended && (
              <span className="absolute -top-2 rounded-full bg-gaming-cyan px-2 py-0.5 text-[10px] font-semibold text-black">
                {recommendedLabel}
              </span>
            )}
            {option.icon && (
              <span
                aria-hidden="true"
                className={selected ? "text-gaming-cyan" : "text-muted-foreground"}
              >
                {option.icon}
              </span>
            )}
            <span className="text-sm font-medium">{option.label}</span>
            <span
              id={`${id}-description`}
              className="text-xs text-muted-foreground"
              style={{ wordBreak: "keep-all" }}
            >
              {option.description}
            </span>
            <span className="pointer-events-none absolute inset-0 rounded-lg ring-gaming-cyan peer-focus-visible:ring-2" />
          </label>
        );
      })}
    </RadioGroup>
  );
}

interface SettingRowProps {
  label: string;
  hint?: string;
  value: string;
  options: readonly { value: string; label: string }[];
  onChange: (value: string) => void;
  disabled?: boolean;
  testId: string;
}

/**
 * 값 하나를 바로 바꾸는 줄. 라벨은 왼쪽, 드롭다운은 오른쪽.
 *
 * 예전에는 이 자리가 **읽기 전용 표**였고, 실제로 바꾸려면 아래 고급 설정까지
 * 스크롤해서 같은 항목을 다시 찾아야 했다. 프리셋으로 큰 틀을 잡고 여기서 바로
 * 손보는 게 자연스럽다 — 바꾸는 순간 위 프리셋 배지가 "직접 설정" 으로 넘어가므로
 * 사용자는 자기가 기본에서 벗어났다는 걸 그 자리에서 안다.
 */
function SettingRow({
  label,
  hint,
  value,
  options,
  onChange,
  disabled,
  testId,
}: SettingRowProps) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/5 py-2.5 last:border-b-0">
      <div className="min-w-0">
        <p className="text-sm">{label}</p>
        {hint && (
          <p className="text-xs text-muted-foreground" style={{ wordBreak: "keep-all" }}>
            {hint}
          </p>
        )}
      </div>
      <Select value={value} onValueChange={onChange} disabled={disabled}>
        <SelectTrigger
          className="h-11 w-[11.5rem] shrink-0"
          data-testid={testId}
          aria-label={label}
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((o) => (
            <SelectItem key={o.value} value={o.value}>
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

interface SpecTableProps {
  testId: string;
  rows: readonly { label: string; value: string }[];
}

/**
 * 고른 프리셋이 실제로 어떤 값이 되는지 보여주는 표.
 *
 * 여기 들어가는 값은 전부 백엔드에 대응하는 근거가 있어야 한다 — 근거 없는 값을
 * 넣지 않는 이유는 `settingSpecs.ts` 상단에 적어 두었다(해상도 사건).
 */
function SpecTable({ testId, rows }: SpecTableProps) {
  if (rows.length === 0) return null;
  return (
    <dl
      data-testid={testId}
      className="mt-4 divide-y divide-white/5 border-t border-white/5 text-sm"
    >
      {rows.map((row) => (
        <div
          key={row.label}
          className="flex items-baseline justify-between gap-4 py-2"
        >
          <dt className="text-muted-foreground" style={{ wordBreak: "keep-all" }}>
            {row.label}
          </dt>
          <dd className="shrink-0 font-medium tabular-nums">{row.value}</dd>
        </div>
      ))}
    </dl>
  );
}

interface BasicCardProps {
  testId: string;
  icon: ReactNode;
  title: string;
  description: string;
  badge?: string;
  children: ReactNode;
}

function BasicCard({
  testId,
  icon,
  title,
  description,
  badge,
  children,
}: BasicCardProps) {
  return (
    <section data-testid={testId} className="gaming-panel p-6">
      <div className="mb-4 flex items-start justify-between gap-3">
        <div className="flex items-start gap-2">
          <span className="mt-0.5 text-gaming-cyan">{icon}</span>
          <div>
            <h3 className="text-base font-semibold">{title}</h3>
            <p
              className="mt-1 text-sm text-muted-foreground"
              style={{ wordBreak: "keep-all" }}
            >
              {description}
            </p>
          </div>
        </div>
        {badge && (
          <Badge variant="secondary" className="shrink-0">
            {badge}
          </Badge>
        )}
      </div>
      {children}
    </section>
  );
}

export function BasicSettings({
  settings,
  onChange,
  disabled,
  sections = ALL_SECTIONS,
}: BasicSettingsProps) {
  const shows = (section: BasicSection) => sections.includes(section);
  const { t } = useTranslation();
  const { toast } = useToast();
  const [recordingsPath, setRecordingsPath] = useState<string | null>(null);

  // 저장 위치는 백엔드가 IPC 로 돌려주지 않는다(고정 경로). 같은 규칙으로
  // 프론트에서 조립해 보여주되, 실패하면 경로 대신 안내 문구를 보여준다.
  useEffect(() => {
    let alive = true;

    const resolvePath = async () => {
      try {
        const base = await dataDir();
        const dir = await join(base, ...RECORDINGS_DIR_SEGMENTS);
        if (alive) setRecordingsPath(dir);
      } catch (error) {
        logger.error("[BasicSettings] Failed to resolve recordings path:", error);
      }
    };

    resolvePath();
    return () => {
      alive = false;
    };
  }, []);

  const highlightPreset = filtersToPreset(settings.event_filter);
  const qualityLevel = detectQualityLevel(settings.video);
  const soundMode = detectSoundMode(settings.audio);
  const scenes = enabledScenes(
    settings.event_filter as Partial<Record<SceneFlag, boolean>>,
  );
  // 이 설정으로 실제로 뭐가 담기는지. 오늘 두 번, 값 하나 때문에 한 판 분량의
  // 클립을 통째로 잃었는데 화면은 아무 말도 하지 않았다.
  const coverage = evaluateCoverage(
    settings.event_filter as unknown as Record<string, boolean | number>,
  );

  const handlePresetChange = (value: string) => {
    onChange({
      ...settings,
      event_filter: applyHighlightPreset(
        value as SelectableHighlightPreset,
        settings.event_filter,
      ),
    });
  };

  const handleQualityChange = (value: string) => {
    // 코덱·인코더·모니터 인덱스는 건드리지 않는다.
    onChange({
      ...settings,
      video: { ...settings.video, ...QUALITY_PRESETS[value as QualityLevel] },
    });
  };

  const handleSoundChange = (value: string) => {
    onChange({
      ...settings,
      audio: { ...settings.audio, ...SOUND_MODES[value as SoundMode] },
    });
  };

  const handleStorageChange = (
    key: "auto_delete_enabled" | "max_storage_gb",
    value: boolean | number,
  ) => {
    onChange({
      ...settings,
      storage: { ...settings.storage, [key]: value },
    });
  };

  const handleOpenFolder = async () => {
    if (!recordingsPath) return;
    try {
      await openPath(recordingsPath);
    } catch (error) {
      logger.error("[BasicSettings] Failed to open recordings folder:", error);
      toast({
        title: t("settings.basic.storage.openFailed"),
        variant: "destructive",
      });
    }
  };

  return (
    <div data-testid="basic-settings" className="space-y-4">
      {/* 1. 어떤 장면을 담을까 */}
      {shows("highlights") && (
      <BasicCard
        testId="basic-highlights"
        icon={<Film className="h-5 w-5" aria-hidden="true" />}
        title={t("settings.basic.highlights.title")}
        description={t("settings.basic.highlights.description")}
        badge={
          highlightPreset === "custom"
            ? t("settings.basic.customLabel")
            : undefined
        }
      >
        {highlightPreset === "custom" && (
          <p
            data-testid="highlights-custom-hint"
            className="mb-3 text-xs text-muted-foreground"
            style={{ wordBreak: "keep-all" }}
          >
            {t("settings.basic.highlights.customHint")}
          </p>
        )}
        <OptionCards
          name="highlight-preset"
          value={highlightPreset === "custom" ? "" : highlightPreset}
          onValueChange={handlePresetChange}
          disabled={disabled}
          recommendedLabel={t("settings.basic.recommendedBadge")}
          options={SELECTABLE_HIGHLIGHT_PRESETS.map((preset) => ({
            value: preset,
            label: t(`settings.basic.highlights.options.${preset}.label`),
            description: t(
              `settings.basic.highlights.options.${preset}.description`,
            ),
            icon: HIGHLIGHT_ICONS[preset],
            recommended: preset === DEFAULT_HIGHLIGHT_PRESET,
          }))}
        />

        {/* 고른 묶음이 실제로 무엇을 담는지. 프리셋 이름만으로는 "확실한 것만" 이
            무엇을 버리는지 알 수 없다. */}
        <div className="mt-4 border-t border-white/5 pt-3">
          <p className="text-sm text-muted-foreground">
            {t("settings.basic.highlights.scenesLabel")}
          </p>
          <p
            data-testid="highlights-scenes"
            className="mt-1 text-sm"
            style={{ wordBreak: "keep-all" }}
          >
            {scenes.length > 0
              ? scenes
                  .map((flag) => t(`settings.basic.highlights.scenes.${flag}`))
                  .join(" · ")
              : t("settings.basic.highlights.scenesEmpty")}
          </p>
        </div>

        {/* 아무것도 안 담기거나 아주 좁으면 반드시 말한다.
            녹화는 정상으로 보이는데 결과물이 0개인 상태를 화면이 구분해 주지
            못해서, 실기기에서 두 판을 통째로 잃었다. */}
        {coverage.level !== "normal" && (
          <div
            data-testid="highlights-coverage-warning"
            className={[
              "mt-3 rounded-lg border p-3 text-sm",
              coverage.level === "none"
                ? "border-red-500/40 bg-red-500/10"
                : "border-amber-500/40 bg-amber-500/10",
            ].join(" ")}
            role="status"
          >
            <p className="font-medium" style={{ wordBreak: "keep-all" }}>
              {t(`settings.basic.highlights.coverage.${coverage.level}.title`)}
            </p>
            <p
              className="mt-1 text-xs text-muted-foreground"
              style={{ wordBreak: "keep-all" }}
            >
              {t(
                `settings.basic.highlights.coverage.${coverage.level}.description`,
              )}
            </p>
            {coverage.blockedByPriority.length > 0 && (
              <p
                data-testid="highlights-coverage-blocked"
                className="mt-2 text-xs"
                style={{ wordBreak: "keep-all" }}
              >
                {t("settings.basic.highlights.coverage.blocked", {
                  scenes: coverage.blockedByPriority
                    .slice(0, 5)
                    .map((s) =>
                      t(`settings.basic.highlights.scenes.${s.labelKey}`),
                    )
                    .join(" · "),
                })}
              </p>
            )}
          </div>
        )}

        <SpecTable
          testId="highlights-specs"
          rows={clipLengthSpecs(settings.clip_timing).map((row) => ({
            label: t(`settings.basic.highlights.clipLength.${row.key}`),
            value: t("settings.basic.seconds", { count: Number(row.value) }),
          }))}
        />
      </BasicCard>)}

      {/* 2. 화질 */}
      {shows("quality") && (
      <BasicCard
        testId="basic-quality"
        icon={<Monitor className="h-5 w-5" aria-hidden="true" />}
        title={t("settings.basic.quality.title")}
        description={t("settings.basic.quality.description")}
        badge={
          qualityLevel === "custom" ? t("settings.basic.customLabel") : undefined
        }
      >
        <OptionCards
          name="quality-level"
          value={qualityLevel === "custom" ? "" : qualityLevel}
          onValueChange={handleQualityChange}
          disabled={disabled}
          recommendedLabel={t("settings.basic.recommendedBadge")}
          options={QUALITY_DISPLAY_ORDER.map((level) => ({
            value: level,
            label: t(`settings.basic.quality.options.${level}.label`),
            description: t(`settings.basic.quality.options.${level}.description`),
            icon: QUALITY_ICONS[level],
            recommended: level === RECOMMENDED_QUALITY,
          }))}
        />

        {/* 프리셋으로 큰 틀을 잡고, 세부는 여기서 바로. 하나라도 바꾸면 위
            배지가 "직접 설정" 이 된다(`detectQualityLevel`). */}
        <div className="mt-4 border-t border-white/5">
          <SettingRow
            testId="quality-fps"
            label={t("settings.basic.quality.specs.frameRate")}
            value={settings.video.frame_rate}
            options={FRAME_RATE_OPTIONS.map((v) => ({
              value: v,
              label: t("settings.basic.quality.specs.frameRateValue", {
                fps: FRAME_RATE_FPS[v],
              }),
            }))}
            onChange={(frame_rate) =>
              onChange({
                ...settings,
                video: { ...settings.video, frame_rate: frame_rate as FrameRate },
              })
            }
            disabled={disabled}
          />
          <SettingRow
            testId="quality-bitrate"
            label={t("settings.basic.quality.specs.bitrate")}
            hint={t("settings.basic.quality.specs.sizePerMinuteHint", {
              size: megabytesPerMinute(BITRATE_MBPS[settings.video.bitrate_preset]),
            })}
            value={settings.video.bitrate_preset}
            options={BITRATE_OPTIONS.map((v) => ({
              value: v,
              label: `${BITRATE_MBPS[v]} Mbps`,
            }))}
            onChange={(bitrate_preset) =>
              onChange({
                ...settings,
                video: {
                  ...settings.video,
                  bitrate_preset: bitrate_preset as BitratePreset,
                },
              })
            }
            disabled={disabled}
          />
          {/* 녹화 크기는 고를 수 있는 값이 아니다 — Windows 캡처는 게임 창 크기를
              그대로 쓴다(`commands.rs` 의 `recording_quality_resolution_label`).
              드롭다운으로 만들면 눌러도 아무 일이 없는 가짜 컨트롤이 된다. */}
          <div className="flex items-center justify-between gap-3 py-2.5 text-sm">
            <span>{t("settings.basic.quality.specs.captureSize")}</span>
            <span className="text-muted-foreground">
              {t("settings.basic.quality.specs.captureSizeValue")}
            </span>
          </div>
        </div>
        <p
          className="mt-3 text-xs text-muted-foreground"
          style={{ wordBreak: "keep-all" }}
        >
          {t("settings.basic.quality.note")}
        </p>
      </BasicCard>)}

      {/* 3. 소리 */}
      {shows("sound") && (
      <BasicCard
        testId="basic-sound"
        icon={<Volume2 className="h-5 w-5" aria-hidden="true" />}
        title={t("settings.basic.sound.title")}
        description={t("settings.basic.sound.description")}
        badge={
          soundMode === "custom" ? t("settings.basic.customLabel") : undefined
        }
      >
        <OptionCards
          name="sound-mode"
          value={soundMode === "custom" ? "" : soundMode}
          onValueChange={handleSoundChange}
          disabled={disabled}
          recommendedLabel={t("settings.basic.recommendedBadge")}
          options={SOUND_ORDER.map((mode) => ({
            value: mode,
            label: t(`settings.basic.sound.options.${mode}.label`),
            description: t(`settings.basic.sound.options.${mode}.description`),
            icon: SOUND_ICONS[mode],
            recommended: mode === RECOMMENDED_SOUND,
          }))}
        />
        {/* 시스템 소리 전체가 섞여 들어간다는 사실은 업로드 사고로 이어지므로
            (디스코드 통화·음악이 그대로 클립에 남는다) 고른 순간 밝힌다. */}
        {settings.audio.record_system_audio && (
          <p
            data-testid="sound-system-warning"
            className="mt-3 text-xs text-muted-foreground"
            style={{ wordBreak: "keep-all" }}
          >
            {t("settings.basic.sound.systemAudioNote")}
          </p>
        )}
      </BasicCard>)}

      {/* 4. 저장 위치 */}
      {shows("storage") && (
      <BasicCard
        testId="basic-storage"
        icon={<HardDrive className="h-5 w-5" aria-hidden="true" />}
        title={t("settings.basic.storage.title")}
        description={t("settings.basic.storage.description")}
      >
        <div className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-white/5 bg-white/[0.02] p-3">
            <code
              data-testid="storage-path"
              className="min-w-0 flex-1 break-all text-xs text-muted-foreground"
            >
              {recordingsPath ?? t("settings.basic.storage.pathUnknown")}
            </code>
            <Button
              variant="outline"
              size="sm"
              className="min-h-[44px] shrink-0"
              onClick={handleOpenFolder}
              disabled={!recordingsPath}
              data-testid="storage-open-folder"
            >
              {t("settings.basic.storage.openFolder")}
            </Button>
          </div>

          <div className="flex items-center justify-between gap-4">
            <div className="space-y-0.5">
              <Label htmlFor="basic-auto-cleanup">
                {t("settings.basic.storage.autoCleanupLabel")}
              </Label>
              <p
                className="text-sm text-muted-foreground"
                style={{ wordBreak: "keep-all" }}
              >
                {t("settings.basic.storage.autoCleanupDescription")}
              </p>
            </div>
            <Switch
              id="basic-auto-cleanup"
              checked={settings.storage.auto_delete_enabled}
              disabled={disabled}
              onCheckedChange={(checked: boolean) =>
                handleStorageChange("auto_delete_enabled", checked)
              }
            />
          </div>

          <div className="flex items-center justify-between gap-4">
            <div className="space-y-0.5">
              <Label htmlFor="basic-max-storage-gb">
                {t("settings.basic.storage.limitLabel")}
              </Label>
              <p
                className="text-sm text-muted-foreground"
                style={{ wordBreak: "keep-all" }}
              >
                {settings.storage.auto_delete_enabled
                  ? t("settings.basic.storage.limitDescription")
                  : t("settings.basic.storage.limitDisabledHint")}
              </p>
            </div>
            <Input
              id="basic-max-storage-gb"
              type="number"
              min={1}
              max={10000}
              className="w-24 text-right"
              value={settings.storage.max_storage_gb}
              disabled={disabled || !settings.storage.auto_delete_enabled}
              onChange={(event) =>
                handleStorageChange(
                  "max_storage_gb",
                  clampStorageGb(event.target.value),
                )
              }
            />
          </div>
        </div>
      </BasicCard>)}

      {/* 5. 리그 켜면 자동 실행 */}
      {shows("autoStart") && (
      <BasicCard
        testId="basic-auto-start"
        icon={<Zap className="h-5 w-5" aria-hidden="true" />}
        title={t("settings.basic.autoStart.title")}
        description={t("settings.basic.autoStart.description")}
      >
        <div className="flex items-center justify-between gap-4">
          <Label htmlFor="basic-auto-start-switch" className="flex-1 cursor-pointer">
            {t("settings.basic.autoStart.label")}
          </Label>
          <Switch
            id="basic-auto-start-switch"
            checked={settings.auto_start_with_league}
            disabled={disabled}
            onCheckedChange={(checked: boolean) =>
              onChange({ ...settings, auto_start_with_league: checked })
            }
          />
        </div>
      </BasicCard>)}
    </div>
  );
}

/** 백엔드 검증 범위(1-10000)를 벗어난 값으로 저장이 실패하지 않게 미리 자른다. */
function clampStorageGb(raw: string): number {
  const parsed = parseInt(raw, 10);
  if (Number.isNaN(parsed)) return 1;
  return Math.min(10000, Math.max(1, parsed));
}
