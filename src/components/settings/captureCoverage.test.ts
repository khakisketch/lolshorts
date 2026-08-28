import fs from "fs";
import path from "path";
import { CAPTURE_SCENES, evaluateCoverage } from "./captureCoverage";

/**
 * Rust 소스를 읽되 줄바꿈을 LF 로 정규화한다.
 *
 * 이 저장소는 git 이 체크아웃 시 CRLF 로 바꾸므로 워킹 카피의 줄바꿈은 환경마다
 * 다르다. 개행 표식으로 블록을 잘라내는 파서가 그걸 그대로 맞으면, 코드는
 * 멀쩡한데 테스트만 "블록의 끝을 찾지 못했습니다" 로 죽는다 — 실제로 한 번
 * 그렇게 깨졌다.
 */
function readRustSource(filePath: string): string {
  return fs.readFileSync(filePath, "utf8").replace(/\r\n/g, "\n");
}

const LIVE_CLIENT_RS = path.resolve(
  __dirname,
  "../../../src-tauri/src/recording/live_client.rs",
);

/**
 * 이 판정이 백엔드와 어긋나면 화면은 "담긴다"고 말하는데 실제로는 버려진다 —
 * 오늘 두 번 그 일이 났고, 두 번 다 한 판 분량의 클립을 잃었다.
 */
describe("captureCoverage (백엔드 미러)", () => {
  const source = readRustSource(LIVE_CLIENT_RS, "utf8");

  /** `EventTrigger::priority()` 본문에서 `Variant => N` 을 뽑는다. */
  const backendPriorities = (() => {
    const start = source.indexOf("pub fn priority(&self) -> u8 {");
    expect(start).toBeGreaterThan(-1);
    const end = source.indexOf("\n    }", start);
    const block = source.slice(start, end);
    const out: Record<string, number> = {};
    const keep = (name: string, value: number) => {
      // 같은 변형이 여러 번 나오면(멀티킬 단계별) **가장 낮은 값**을 남긴다 —
      // 토글이 하나뿐이라 화면은 "가장 먼저 빠지는 단계"를 기준으로 말해야 한다.
      out[name] = out[name] === undefined ? value : Math.min(out[name], value);
    };

    // `Variant => 3,` 형태
    const simple = /EventTrigger::(\w+)(?:\([^)]*\))?\s*=>\s*(\d+)\s*,/g;
    let m: RegExpExecArray | null;
    while ((m = simple.exec(block)) !== null) {
      keep(m[1], Number(m[2]));
    }

    // `Variant(n) => { if *n >= 3 { 5 } else { 4 } }` 형태 — 인원수로 갈리는 것.
    // if/else 가 중첩돼 있어 정규식으로는 블록 끝을 못 찾으므로 중괄호를 센다.
    // 블록 안 정수 리터럴 중 우선순위 범위(1~5)의 최솟값을 그 변형 값으로 본다.
    const arrowBlock = /EventTrigger::(\w+)\([^)]*\)\s*=>\s*\{/g;
    while ((m = arrowBlock.exec(block)) !== null) {
      let depth = 1;
      let i = arrowBlock.lastIndex;
      while (i < block.length && depth > 0) {
        if (block[i] === "{") depth += 1;
        else if (block[i] === "}") depth -= 1;
        i += 1;
      }
      const body = block.slice(arrowBlock.lastIndex, i - 1);
      // 분기 **결과값**만 센다. 본문에는 `*n >= 3` 처럼 조건에 쓰인 숫자도 있어서
      // 아무 정수나 주우면 그 threshold 가 우선순위로 둔갑한다(실제로 그랬다).
      // 결과값은 언제나 중괄호 안에 홀로 놓인 정수다: `{ 5 }` / `{ 4 }`.
      const nums = [...body.matchAll(/\{\s*(\d+)\s*\}/g)].map((x) =>
        Number(x[1]),
      );
      if (nums.length) keep(m[1], Math.min(...nums));
    }

    return out;
  })();

  it("우선순위 표를 백엔드에서 실제로 읽어왔다", () => {
    expect(Object.keys(backendPriorities).length).toBeGreaterThan(15);
    expect(backendPriorities.ChampionKill).toBe(1);
    expect(backendPriorities.Ace).toBe(4);
  });

  it("화면이 아는 우선순위가 백엔드와 같다", () => {
    const MAP: Record<string, string> = {
      record_kills: "ChampionKill",
      record_deaths: "Death",
      record_first_blood_victim: "FirstBloodVictim",
      record_assists: "Assist",
      record_turret: "TurretKill",
      record_dragon: "DragonKill",
      record_herald: "HeraldKill",
      record_inhibitor: "InhibitorKill",
      record_voidgrubs: "VoidgrubsKill",
      record_trade_kill: "TradeKill",
      record_first_blood: "FirstBlood",
      record_baron: "BaronKill",
      record_atakhan: "AtakhanKill",
      record_shutdown: "Shutdown",
      record_game_end: "GameEnd",
      record_ace: "Ace",
      record_steal: "Steal",
      record_elder: "ElderDragonKill",
      record_low_hp: "LowHpOutplay",
      record_multikills: "Multikill",
      record_outplay: "Outplay1vX",
    };

    for (const scene of CAPTURE_SCENES) {
      const variant = MAP[scene.flag];
      expect(variant).toBeDefined();
      expect(backendPriorities[variant]).toBe(scene.priority);
    }
  });
});

describe("captureCoverage (판정)", () => {
  const allOn = () =>
    Object.fromEntries(CAPTURE_SCENES.map((s) => [s.flag, true])) as Record<
      string,
      boolean
    >;

  it("전부 켜고 문턱이 낮으면 정상이다", () => {
    const v = evaluateCoverage({ ...allOn(), min_priority: 1 });
    expect(v.level).toBe("normal");
    expect(v.blockedByPriority).toEqual([]);
  });

  it("min_priority 5 는 실제로 담기는 게 거의 없다 — 실기기에서 한 판을 통째로 잃었다", () => {
    const v = evaluateCoverage({ ...allOn(), min_priority: 5 });
    // 5 이상은 펜타킬과 1v3 이상 아웃플레이뿐인데, 둘 다 토글이 더 낮은 단계와
    // 공유돼 있어 표에는 각각 2/4 로 잡힌다 -> 담기는 것이 하나도 없다.
    expect(v.level).toBe("none");
    expect(v.captured).toEqual([]);
    expect(v.blockedByPriority.length).toBeGreaterThan(15);
  });

  it("켜 뒀는데 문턱에 막히는 것을 따로 알려준다", () => {
    const v = evaluateCoverage({ ...allOn(), min_priority: 3 });
    const blocked = v.blockedByPriority.map((s) => s.flag);
    // 일반 킬은 우선순위 1 이라 문턱 3 에 막힌다 — 사용자는 "킬을 담겠다"고
    // 켜 뒀는데 조용히 버려지는 상태다.
    expect(blocked).toContain("record_kills");
    expect(blocked).toContain("record_deaths");
    expect(v.captured.map((s) => s.flag)).toContain("record_ace");
  });

  it("토글을 다 끄면 아무것도 안 담긴다", () => {
    const off = Object.fromEntries(CAPTURE_SCENES.map((s) => [s.flag, false]));
    const v = evaluateCoverage({ ...off, min_priority: 1 });
    expect(v.level).toBe("none");
    expect(v.blockedByPriority).toEqual([]);
  });

  it("몇 개만 켜면 좁다고 말한다", () => {
    const v = evaluateCoverage({
      record_ace: true,
      record_steal: true,
      min_priority: 1,
    });
    expect(v.level).toBe("narrow");
    expect(v.captured).toHaveLength(2);
  });

  it("min_priority 가 없으면 1 로 본다", () => {
    const v = evaluateCoverage({ record_kills: true });
    expect(v.captured.map((s) => s.flag)).toEqual(["record_kills"]);
  });

  it("알 수 없는 키가 섞여 있어도 무시한다", () => {
    const v = evaluateCoverage({
      record_kills: true,
      record_nexus: true, // 백엔드에 소비처가 없는 플래그
      min_priority: 1,
    });
    expect(v.captured.map((s) => s.flag)).toEqual(["record_kills"]);
  });
});
