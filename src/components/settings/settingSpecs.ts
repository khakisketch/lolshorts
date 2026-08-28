/**
 * 기본 설정 화면이 프리셋 카드 아래에 보여주는 **실제 수치**.
 *
 * 이 파일이 존재하는 이유는 실기기 테스트에서 드러난 한 가지 때문이다:
 * 설정 화면이 보여주던 "해상도 1920x1080" 은 거짓이었다. Windows 캡처 경로는
 * `config.resolution` 을 ffmpeg 에 넘기지 않고 게임 창 크기를 그대로 쓰는데
 * (`src-tauri/src/recording/commands.rs` 의 `recording_quality_resolution_label`
 * 주석), 화면은 저장된 값을 그대로 읽어 보여주고 있었다. 실제 산출물은
 * 2560x1600 이었다.
 *
 * 그래서 여기 있는 모든 값은 **백엔드에 대응하는 근거가 있는 것만** 담는다.
 * 근거가 없으면(예: "한 판당 예상 클립 수") 아예 표시하지 않는다 — 그럴듯한
 * 숫자를 지어내는 것이 값을 안 보여주는 것보다 나쁘다.
 *
 * 백엔드 표와 어긋나면 `settingSpecs.test.ts` 가 Rust 소스를 직접 읽어 깨뜨린다.
 */

import type { BitratePreset, EventFilterSettings, FrameRate } from "@/types";

/**
 * `BitratePreset::to_bitrate_bps()` 미러 (`settings/models.rs`).
 *
 * `Custom(kbps)` 변형은 기본 설정 화면에서 고를 수 없으므로 제외한다 — 고급
 * 설정에서 직접 넣은 경우 화질 프리셋 판정이 이미 "직접 설정" 으로 떨어진다.
 */
export const BITRATE_MBPS: Record<BitratePreset, number> = {
  low: 10,
  medium: 20,
  high: 40,
  very_high: 80,
};

/** `FrameRate` 와이어 값 → 실제 fps. */
export const FRAME_RATE_FPS: Record<FrameRate, number> = {
  fps30: 30,
  fps60: 60,
  fps120: 120,
  fps144: 144,
};

/**
 * 분당 저장 용량(MB). 비트레이트는 초당 비트이므로 8로 나눠 바이트로, 60을 곱해 분당.
 *
 * 컨테이너 오버헤드와 오디오는 무시한다(합쳐서 2% 미만). 실측 클립은 설정값보다
 * 높게 나오는 경우가 있는데(창이 1080p 보다 크면 인코더가 목표 비트레이트를
 * 넘기도) 그건 "대략" 이라는 라벨이 감당할 범위다.
 */
export function megabytesPerMinute(bitrateMbps: number): number {
  return Math.round((bitrateMbps / 8) * 60);
}

/**
 * 화면에 나열할 "담기는 장면" 순서.
 *
 * `record_nexus` 는 **일부러 빠져 있다** — 설정 구조체에는 있지만 이 플래그를
 * 읽는 코드가 백엔드 어디에도 없다(다른 20개는 전부 트리거를 실제로 가른다).
 * 켜도 꺼도 아무 일이 없는 항목을 "담기는 장면" 으로 보여주면 그 순간 이 화면은
 * 다시 거짓말을 시작한다. 플래그가 실제로 연결되면 여기 추가한다.
 */
export const SCENE_FLAGS = [
  "record_kills",
  "record_multikills",
  "record_first_blood",
  "record_shutdown",
  "record_trade_kill",
  "record_outplay",
  "record_low_hp",
  "record_assists",
  "record_deaths",
  "record_first_blood_victim",
  "record_steal",
  "record_baron",
  "record_elder",
  "record_dragon",
  "record_herald",
  "record_voidgrubs",
  "record_atakhan",
  "record_turret",
  "record_inhibitor",
  "record_ace",
  "record_game_end",
] as const;

export type SceneFlag = (typeof SCENE_FLAGS)[number];

/** 지금 켜져 있는 장면 플래그만, 화면 나열 순서대로. */
export function enabledScenes(
  filter: Partial<Record<SceneFlag, boolean>>,
): SceneFlag[] {
  return SCENE_FLAGS.filter((flag) => filter[flag] === true);
}

export interface SpecRow {
  /** i18n 키 뒤에 붙는 항목 이름. */
  key: string;
  /** 이미 사람이 읽을 수 있게 만들어진 값. */
  value: string;
}

/**
 * 화질 카드 아래에 붙는 표.
 *
 * **해상도와 코덱이 여기 없는 것은 누락이 아니라 의도다.**
 *
 * - 해상도: Windows 캡처는 게임 창 크기를 그대로 쓰므로 설정으로 정할 수 있는
 *   값이 아니다. "게임 창 크기 그대로" 라는 사실 보고로만 화면에 남긴다.
 * - 코덱: 사용자가 판단할 수 있는 축이 아니다(h265 를 고르면 편집기 미리보기가
 *   검은 화면이 되는데 원인을 알 수 없다). 기본 화면에서 코덱을 감춘 결정은
 *   `BasicSettings.test.tsx` 가 고정하고 있다 — 고급 설정에는 그대로 있다.
 */
export function qualitySpecs(video: {
  frame_rate: FrameRate;
  bitrate_preset: BitratePreset;
}): SpecRow[] {
  const fps = FRAME_RATE_FPS[video.frame_rate];
  const mbps = BITRATE_MBPS[video.bitrate_preset];

  const rows: SpecRow[] = [];
  if (fps !== undefined) {
    rows.push({ key: "frameRate", value: `${fps}` });
  }
  if (mbps !== undefined) {
    rows.push({ key: "bitrate", value: `${mbps} Mbps` });
    rows.push({
      key: "sizePerMinute",
      value: `${megabytesPerMinute(mbps)} MB`,
    });
  }
  return rows;
}

/*
 * 클립 길이 표(`clipLengthSpecs`)는 **일부러 없앴다.**
 *
 * 기본 화면에 다섯 버킷의 초 단위 숫자를 보여줬는데, 그중 사용자가 바꿀 수 있는
 * 것은 셋뿐이었다(고급 설정의 슬라이더 — 킬·멀티킬·스틸). 죽는 장면과 게임 끝은
 * 숫자만 보이고 어떤 UI로도 바꿀 수 없었다. 조작할 수 없는 숫자 다섯 줄은 정보가
 * 아니라 소음이라, 표를 지우고 길이는 고급 설정에서만 다루기로 했다.
 *
 * 되살리려면 `TRIGGER_DEFAULT_SECONDS` 미러와 그 드리프트 테스트도 함께 되살려야
 * 한다 — 백엔드는 설정에 키가 없는 버킷에서 `EventTrigger` 설계값으로 폴백하므로,
 * 미러 없이 숫자를 적으면 화면이 다시 거짓말을 시작한다(한 번 그렇게 깨졌다:
 * 화면 「게임 끝 13초」, 실제 산출물 40초).
 */

/** 화면이 실제로 읽는 필터 부분집합. */
export type SceneFilter = Pick<EventFilterSettings, never> &
  Partial<Record<SceneFlag, boolean>>;
