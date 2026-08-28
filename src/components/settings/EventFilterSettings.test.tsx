import fs from "fs";
import path from "path";
import { fireEvent, render, screen } from "@testing-library/react";
import {
  EVENT_GROUPS,
  EventFilterSettings,
  isSubSituationVisible,
  SUB_SITUATION_PARENTS,
  type EventFlag,
} from "./EventFilterSettings";
import { EVENT_FILTER_DEFAULTS } from "./highlightPreset";

jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}));

const LIVE_CLIENT_RS = path.resolve(
  __dirname,
  "../../../src-tauri/src/recording/live_client.rs",
);
const MODELS_RS = path.resolve(
  __dirname,
  "../../../src-tauri/src/settings/models.rs",
);

type Filter = Parameters<typeof EventFilterSettings>[0]["settings"];

const baseFilter = (overrides: Partial<Filter> = {}): Filter =>
  ({ ...EVENT_FILTER_DEFAULTS, ...overrides }) as unknown as Filter;

/**
 * 이 화면의 위계는 백엔드 강등 사슬과 같은 것을 말해야 한다.
 *
 * 어긋나면 증상이 조용하다 — 화면은 "부모를 켜 두었으니 아래는 자동 포함"이라며
 * 스위치를 감추는데, 백엔드가 그 트리거를 부모로 강등해 주지 않으면 그 순간은
 * 그냥 버려진다. 사용자는 담기로 켜 둔 장면을 잃고도 화면에서 이유를 알 수 없다.
 */
describe("EventFilterSettings 위계 (백엔드 미러)", () => {
  const liveClient = fs.readFileSync(LIVE_CLIENT_RS, "utf8");
  const models = fs.readFileSync(MODELS_RS, "utf8");

  /** `EventTrigger::parent()` 본문에서 (자식 변형 → 부모 변형) 관계를 읽는다. */
  const backendParents = (() => {
    const start = liveClient.indexOf("pub fn parent(&self, event: &GameEvent)");
    expect(start).toBeGreaterThan(-1);
    const end = liveClient.indexOf("\n    }", start);
    const block = liveClient.slice(start, end);

    // `=> Some(EventTrigger::Parent)` 마다, 그 **앞쪽 텍스트**에 적힌 자식 변형들을
    // 모은다. 왼쪽이 여러 줄에 걸친 `A\n | B\n | C` 형태라 한 정규식으로 잡기보다
    // 화살표를 기준으로 구간을 잘라 읽는 편이 튼튼하다.
    //
    // 스틸은 안쪽에 또 `match` 가 있어서 `_ if ...` / `_` arm 의 왼쪽에는 변형
    // 이름이 없다. 그런 구간은 **직전에 읽은 자식**에 그대로 속하므로 이어받는다.
    const arms: Record<string, string[]> = {};
    // 가드가 붙은 arm 은 본문이 블록이라 `=> { Some(...) }` 로 적힌다. 중괄호를
    // 허용하지 않으면 그 arm 이 통째로 안 잡혀 부모 하나를 조용히 놓친다.
    const arrow = /=>\s*\{?\s*Some\(EventTrigger::(\w+)\)/g;
    let cursor = 0;
    let current: string[] = [];
    let match: RegExpExecArray | null;
    while ((match = arrow.exec(block)) !== null) {
      const left = block.slice(cursor, match.index);
      const found = [...left.matchAll(/EventTrigger::(\w+)/g)].map((m) => m[1]);
      if (found.length > 0) current = found;
      for (const child of current) {
        arms[child] = arms[child] ?? [];
        arms[child].push(match[1]);
      }
      cursor = arrow.lastIndex;
    }
    return arms;
  })();

  it("백엔드 parent() 를 실제로 읽어왔다", () => {
    expect(Object.keys(backendParents).length).toBeGreaterThan(4);
    expect(backendParents.Shutdown).toEqual(["ChampionKill"]);
  });

  /** 화면 플래그 → 백엔드 트리거 변형. */
  const FLAG_TO_TRIGGER: Partial<Record<EventFlag, string>> = {
    record_multikills: "Multikill",
    record_shutdown: "Shutdown",
    record_outplay: "Outplay1vX",
    record_low_hp: "LowHpOutplay",
    record_trade_kill: "TradeKill",
    record_first_blood_victim: "FirstBloodVictim",
    record_elder: "ElderDragonKill",
    record_steal: "Steal",
  };

  const TRIGGER_TO_FLAG: Record<string, EventFlag> = {
    ChampionKill: "record_kills",
    Death: "record_deaths",
    DragonKill: "record_dragon",
    BaronKill: "record_baron",
    ElderDragonKill: "record_elder",
  };

  it("모든 하위 상황의 부모가 백엔드 강등 사슬과 같다", () => {
    for (const [flag, trigger] of Object.entries(FLAG_TO_TRIGGER)) {
      const parents = SUB_SITUATION_PARENTS[flag as EventFlag];
      expect(parents).toBeDefined();

      const backend = backendParents[trigger];
      expect(backend).toBeDefined();

      // 백엔드가 내려보내는 부모를 화면 플래그로 옮긴다. 장로는 그 자체가
      // 드래곤의 하위라 한 단계 더 내려가므로(스틸 -> 장로 -> 드래곤) 펼친다.
      const expanded = new Set<EventFlag>();
      for (const variant of backend) {
        const asFlag = TRIGGER_TO_FLAG[variant];
        expect(asFlag).toBeDefined();
        if (asFlag === "record_elder") {
          expanded.add("record_dragon");
        } else {
          expanded.add(asFlag);
        }
      }

      expect(new Set(parents)).toEqual(expanded);
    }
  });

  it("백엔드가 강등하는 트리거를 화면이 빠뜨리지 않았다", () => {
    const covered = new Set(Object.values(FLAG_TO_TRIGGER));
    for (const child of Object.keys(backendParents)) {
      expect(covered.has(child)).toBe(true);
    }
  });

  it("마이그레이션 표(reconcile_hierarchy)가 같은 부모를 본다", () => {
    const start = models.indexOf("pub fn reconcile_hierarchy(&mut self)");
    expect(start).toBeGreaterThan(-1);
    const block = models.slice(start, models.indexOf("\n    }\n}", start));

    // `if self.record_x { self.record_y = true; ... }` 블록에서 (부모, 자식들)을 읽는다.
    const found: Record<string, Set<string>> = {};
    const armPattern = /if ([^{]+)\{([^}]*)\}/g;
    let match: RegExpExecArray | null;
    while ((match = armPattern.exec(block)) !== null) {
      const parents = [...match[1].matchAll(/self\.(record_\w+)/g)].map(
        (m) => m[1],
      );
      const children = [...match[2].matchAll(/self\.(record_\w+) = true/g)].map(
        (m) => m[1],
      );
      for (const child of children) {
        found[child] = new Set(parents);
      }
    }

    for (const [flag, parents] of Object.entries(SUB_SITUATION_PARENTS)) {
      expect({ flag, parents: found[flag] }).toEqual({
        flag,
        parents: new Set(parents),
      });
    }
  });
});

describe("isSubSituationVisible", () => {
  it("부모가 켜져 있으면 감춘다 — 그 스위치는 결과를 바꾸지 못한다", () => {
    const filter = { record_kills: true } as Partial<
      Record<EventFlag, boolean>
    >;
    expect(isSubSituationVisible("record_shutdown", filter)).toBe(false);
  });

  it("부모가 꺼지면 예외로 물어본다", () => {
    const filter = { record_kills: false } as Partial<
      Record<EventFlag, boolean>
    >;
    expect(isSubSituationVisible("record_shutdown", filter)).toBe(true);
  });

  it("부모가 둘인 스틸은 하나만 꺼져도 물어본다", () => {
    expect(
      isSubSituationVisible("record_steal", {
        record_dragon: true,
        record_baron: false,
      }),
    ).toBe(true);
    expect(
      isSubSituationVisible("record_steal", {
        record_dragon: true,
        record_baron: true,
      }),
    ).toBe(false);
  });

  it("부모가 없는 항목은 언제나 보인다", () => {
    expect(isSubSituationVisible("record_kills", {})).toBe(true);
  });
});

describe("EventFilterSettings 렌더", () => {
  it("부모가 켜져 있으면 하위 스위치 대신 무엇이 포함되는지 말한다", () => {
    render(
      <EventFilterSettings
        settings={baseFilter({ record_kills: true })}
        onChange={jest.fn()}
      />,
    );

    expect(screen.queryByTestId("event-group-kills-exceptions")).toBeNull();
    expect(
      screen.getByTestId("event-group-kills-included"),
    ).toBeInTheDocument();
  });

  it("부모를 끄면 예외를 고를 수 있게 펼친다", () => {
    render(
      <EventFilterSettings
        settings={baseFilter({ record_deaths: false })}
        onChange={jest.fn()}
      />,
    );

    const exceptions = screen.getByTestId("event-group-deaths-exceptions");
    expect(exceptions).toBeInTheDocument();
    // "죽는 장면은 됐고 퍼블 당한 것만" — 사용자가 원한 조합이 여기서 가능해진다.
    expect(
      screen.getByLabelText(/record_first_blood_victim/),
    ).toBeInTheDocument();
  });

  it("예외를 켜면 그 플래그만 바뀐다", () => {
    const onChange = jest.fn();
    const settings = baseFilter({
      record_deaths: false,
      record_first_blood_victim: false,
    });

    render(<EventFilterSettings settings={settings} onChange={onChange} />);
    fireEvent.click(screen.getByLabelText(/record_first_blood_victim/));

    expect(onChange).toHaveBeenCalledWith({
      ...settings,
      record_first_blood_victim: true,
    });
    // 행 전체가 `<label>` 이고 그 안에 스위치가 들어 있다. 클릭이 두 번 전달되면
    // 켰다가 곧바로 꺼지므로, 호출이 정확히 한 번인지가 곧 그 회귀 검사다.
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("아무 일도 하지 않는 record_nexus 스위치는 놓지 않는다", () => {
    render(
      <EventFilterSettings settings={baseFilter()} onChange={jest.fn()} />,
    );
    expect(screen.queryByLabelText(/record_nexus/)).toBeNull();
  });

  it("감지되는 플래그를 어느 그룹에도 넣지 않고 빠뜨리지 않았다", () => {
    const placed = new Set(
      EVENT_GROUPS.flatMap((group) => [...group.primary, ...group.subs]),
    );
    const expected: EventFlag[] = [
      "record_kills",
      "record_multikills",
      "record_first_blood",
      "record_shutdown",
      "record_outplay",
      "record_low_hp",
      "record_deaths",
      "record_trade_kill",
      "record_first_blood_victim",
      "record_assists",
      "record_dragon",
      "record_baron",
      "record_elder",
      "record_herald",
      "record_voidgrubs",
      "record_atakhan",
      "record_turret",
      "record_inhibitor",
      "record_ace",
      "record_game_end",
      "record_steal",
    ];
    for (const flag of expected) {
      expect({ flag, placed: placed.has(flag) }).toEqual({
        flag,
        placed: true,
      });
    }
  });
});
