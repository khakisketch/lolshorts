import fs from "fs";
import path from "path";
import {
  BITRATE_MBPS,
  enabledScenes,
  megabytesPerMinute,
  qualitySpecs,
  SCENE_FLAGS,
} from "./settingSpecs";

const SRC_TAURI = path.resolve(__dirname, "../../../src-tauri/src");
const MODELS_RS = path.join(SRC_TAURI, "settings/models.rs");
const AUTO_CLIP_RS = path.join(SRC_TAURI, "recording/auto_clip_manager.rs");
const LIVE_CLIENT_RS = path.join(SRC_TAURI, "recording/live_client.rs");

/** 함수 본문의 끝을 찾는 표식 — 줄바꿈 + 들여쓰기 4칸 + 닫는 중괄호. */
const FN_END = ["", "    }"].join("\n");

/**
 * Rust 소스를 읽되 줄바꿈을 LF 로 정규화한다.
 *
 * 이 저장소는 git 이 체크아웃 시 CRLF 로 바꾸므로 워킹 카피의 줄바꿈이 환경마다
 * 다르다. 위 `FN_END` 처럼 개행을 표식으로 쓰는 파서가 그걸 그대로 맞으면 코드는
 * 멀쩡한데 테스트만 죽는다 — 실제로 한 번 그렇게 깨졌다.
 */
function readRustSource(filePath: string): string {
  return fs.readFileSync(filePath, "utf8").replace(/\r\n/g, "\n");
}

/**
 * 이 화면은 한 번 거짓말을 한 전력이 있다 — 저장된 `resolution` 을 캡처 해상도인
 * 것처럼 보여줬는데 Windows 캡처는 그 값을 쓰지 않는다. 카드 아래에 수치를 더
 * 노출하기로 한 이상, 그 수치가 백엔드와 어긋나는 순간을 테스트가 먼저 잡아야
 * 한다. 그래서 여기서는 Rust 소스를 실제로 읽어 대조한다.
 */
describe("settingSpecs (backend mirror)", () => {
  describe("비트레이트 표", () => {
    const source = readRustSource(MODELS_RS);

    it("models.rs 의 to_bitrate_bps 와 Mbps 값이 일치한다", () => {
      const found: Record<string, number> = {};
      const re = /BitratePreset::(\w+)\s*=>\s*([\d_]+)\s*,/g;
      let match: RegExpExecArray | null;
      while ((match = re.exec(source)) !== null) {
        const [, variant, bps] = match;
        if (variant === "Custom") continue;
        found[variant] = Number(bps.replace(/_/g, "")) / 1_000_000;
      }

      expect(found).toEqual({
        Low: BITRATE_MBPS.low,
        Medium: BITRATE_MBPS.medium,
        High: BITRATE_MBPS.high,
        VeryHigh: BITRATE_MBPS.very_high,
      });
    });
  });

  describe("클립 길이 버킷", () => {
    const source = readRustSource(AUTO_CLIP_RS);

    it("이벤트별 설정 키와 권장 길이 fallback 계약을 유지한다", () => {
      // 이 매핑은 예전처럼 wildcard로 모든 이벤트를 kill 버킷에 넣지 않는다.
      // Rust가 exhaustive match로 바뀌어 `_ => None` 문자열을 찾는 검사는 더 이상
      // 구현 계약이 아니다. 대신 UI가 의존하는 실제 설정 키와, 설정이 없을 때의
      // EventTrigger 권장 길이 fallback을 함께 확인한다.
      const start = source.indexOf("fn calculate_clip_window");
      const body = source.slice(start, source.indexOf(FN_END, start));
      expect(body).toContain("let settings_key = match trigger");
      expect(body).toContain('EventTrigger::GameEnd => Some("game_end")');
      expect(body).toContain('EventTrigger::BaronKill => Some("baron")');
      expect(body).toContain(
        'EventTrigger::Outplay1vX(_) | EventTrigger::LowHpOutplay => Some("outplay")',
      );
      expect(body).toContain("EventTrigger::ChampionKill");
      expect(body).toContain('=> Some("kill")');
      expect(body).toContain("trigger.pre_duration()");
      expect(body).toContain("trigger.post_duration()");
    });
  });

  describe("담기는 장면 목록", () => {
    // 후보는 `EventFilterSettings` 가 선언한 필드로 한정한다. 저장소 전체를
    // `record_*` 로 훑으면 게임 모드 플래그(`record_aram`)나 메트릭 이름
    // (`record_success`)까지 딸려온다.
    const declared = (() => {
      const source = readRustSource(MODELS_RS);
      const start = source.indexOf("pub struct EventFilterSettings {");
      expect(start).toBeGreaterThan(-1);
      const end = source.indexOf("\n}", start);
      const block = source.slice(start, end);
      const fields = new Set<string>();
      const re = /pub (record_[a-z_]+):\s*bool/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(block)) !== null) {
        fields.add(m[1]);
      }
      return fields;
    })();

    const consumed = new Set<string>();
    for (const file of [AUTO_CLIP_RS, LIVE_CLIENT_RS]) {
      const source = readRustSource(file);
      for (const flag of declared) {
        if (new RegExp(`\\b${flag}\\b`).test(source)) {
          consumed.add(flag);
        }
      }
    }

    it("후보 목록을 실제 구조체에서 읽어왔다", () => {
      expect(declared.size).toBeGreaterThan(15);
      expect(declared.has("record_kills")).toBe(true);
      expect(declared.has("record_nexus")).toBe(true);
    });

    it("실제로 트리거를 가르는 플래그만 나열한다", () => {
      // 화면이 보여주는 장면은 전부 백엔드가 읽는 플래그여야 한다. 읽히지 않는
      // 플래그를 "담기는 장면" 으로 보여주면 켜도 아무 일이 없다.
      for (const flag of SCENE_FLAGS) {
        expect(consumed.has(flag)).toBe(true);
      }
    });

    it("record_nexus 는 아직 소비처가 없으므로 목록에 없다", () => {
      // 이 단언이 깨지는 방향은 둘 중 하나다:
      //  - 백엔드가 record_nexus 를 쓰기 시작했다 -> SCENE_FLAGS 에 추가할 것
      //  - 설정에서 항목이 사라졌다 -> 이 테스트를 지울 것
      expect(consumed.has("record_nexus")).toBe(false);
      expect(SCENE_FLAGS as readonly string[]).not.toContain("record_nexus");
    });

    it("백엔드가 읽는 플래그 중 화면에서 빠진 것이 없다", () => {
      const shown = new Set<string>(SCENE_FLAGS);
      const missing = [...consumed].filter((flag) => !shown.has(flag));
      expect(missing).toEqual([]);
    });
  });
});

describe("settingSpecs (계산)", () => {
  it("분당 용량은 비트레이트를 8로 나눠 60을 곱한 값이다", () => {
    expect(megabytesPerMinute(20)).toBe(150);
    expect(megabytesPerMinute(10)).toBe(75);
    expect(megabytesPerMinute(40)).toBe(300);
    expect(megabytesPerMinute(80)).toBe(600);
  });

  it("화질 표에 해상도도 코덱도 들어가지 않는다", () => {
    // 캡처 해상도는 게임 창이 정한다 -> 표에 넣는 순간 다시 거짓말이 된다.
    // 코덱은 게이머가 판단할 수 있는 축이 아니다(BasicSettings.test.tsx 가 고정).
    const rows = qualitySpecs({
      frame_rate: "fps60",
      bitrate_preset: "medium",
    });
    expect(rows.map((r) => r.key)).toEqual([
      "frameRate",
      "bitrate",
      "sizePerMinute",
    ]);
    expect(rows.find((r) => r.key === "bitrate")?.value).toBe("20 Mbps");
    expect(rows.find((r) => r.key === "sizePerMinute")?.value).toBe("150 MB");
    expect(JSON.stringify(rows)).not.toMatch(/1920|1080|2560|해상도/);
    expect(JSON.stringify(rows)).not.toMatch(/codec|H\.26/i);
  });

  it("켜진 장면만 화면 순서대로 돌려준다", () => {
    expect(
      enabledScenes({
        record_kills: true,
        record_multikills: true,
        record_deaths: false,
        record_ace: true,
      }),
    ).toEqual(["record_kills", "record_multikills", "record_ace"]);
  });

  it("아무것도 안 켜져 있으면 빈 목록이다", () => {
    expect(enabledScenes({})).toEqual([]);
  });
});
