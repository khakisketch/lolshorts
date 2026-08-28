import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  applyHighlightPreset,
  SELECTABLE_HIGHLIGHT_PRESETS,
  type SelectableHighlightPreset,
} from "./highlightPreset";

/**
 * 이벤트 필터 — 부모 하나와 그 하위 상황들.
 *
 * # 왜 평면 목록이 아닌가
 *
 * 감지는 킬 하나에 **가장 특별한 이름 하나만** 붙인다(`detect_trigger`). 그래서
 * 셧다운 킬은 "킬"이 아니라 "셧다운"으로 도착하고, 스위치 16개가 위계 없이
 * 늘어서 있던 동안 "킬 켜기 + 셧다운 끄기"는 **셧다운 킬을 통째로 지웠다**.
 * 사용자는 킬을 담겠다고 했는데 가장 값진 킬이 빠졌고, 화면은 그 사실을 말할
 * 방법조차 없었다 — 스위치들이 서로 무관해 보였기 때문이다.
 *
 * 백엔드가 이제 하위 상황을 부모로 강등해 판정하므로(`EventTrigger::parent`),
 * **부모를 켜면 그 계열은 반드시 담긴다.** 그러면 부모가 켜져 있는 동안 하위
 * 스위치는 아무것도 바꾸지 못한다 — 그래서 접는다. 부모를 끈 순간에만
 * "그래도 이건 담을까?"가 의미를 갖고, 그때 펼쳐진다.
 *
 * 이 화면의 위계는 백엔드의 강등 규칙과 같은 것을 말해야 한다. 어긋나면
 * `EventFilterSettings.test.tsx` 가 `live_client.rs` 의 `parent()` 를 읽어 깨뜨린다.
 */

interface EventFilterSettings {
  record_kills: boolean;
  record_multikills: boolean;
  record_first_blood: boolean;
  record_shutdown: boolean;
  record_outplay: boolean;
  record_low_hp: boolean;
  record_deaths: boolean;
  record_trade_kill: boolean;
  record_first_blood_victim: boolean;
  record_assists: boolean;
  record_dragon: boolean;
  record_baron: boolean;
  record_elder: boolean;
  record_herald: boolean;
  record_voidgrubs: boolean;
  record_atakhan: boolean;
  record_turret: boolean;
  record_inhibitor: boolean;
  record_nexus: boolean;
  record_ace: boolean;
  record_game_end: boolean;
  record_steal: boolean;
  min_priority: number;
}

/** 화면이 다루는 토글 이름. `min_priority` 같은 수치 필드는 제외. */
export type EventFlag = {
  [K in keyof EventFilterSettings]: EventFilterSettings[K] extends boolean
    ? K
    : never;
}[keyof EventFilterSettings];

/**
 * 하위 상황 → 그 부모들. 부모가 **전부** 켜져 있으면 화면에서 접힌다.
 *
 * `live_client.rs` 의 `EventTrigger::parent()` 미러이되 두 곳이 다르다.
 *
 * - `record_steal` 은 부모가 둘이다. 백엔드는 원본 이벤트를 보고 드래곤/바론 중
 *   하나를 고르지만, 화면은 어느 쪽이 올지 미리 알 수 없으므로 둘 다 켜져 있을
 *   때만 접는다. 하나라도 꺼져 있으면 "그래도 스틸은 담을까?"가 유효한 질문이다.
 * - `record_first_blood` 는 백엔드에 부모가 없다 — 퍼블은 `ChampionKill` 과 **별개
 *   이벤트**라 킬을 꺼도 독립적으로 잡힌다. 그래도 여기서는 킬의 하위로 둔다:
 *   킬을 켜 두면 그 순간은 어차피 담기므로(퍼블 트리거로든 킬 트리거로든) 스위치가
 *   결과를 바꾸지 못한다는 점이 같기 때문이다.
 */
export const SUB_SITUATION_PARENTS: Readonly<
  Partial<Record<EventFlag, readonly EventFlag[]>>
> = {
  record_multikills: ["record_kills"],
  record_shutdown: ["record_kills"],
  record_outplay: ["record_kills"],
  record_low_hp: ["record_kills"],
  record_first_blood: ["record_kills"],
  record_trade_kill: ["record_deaths"],
  record_first_blood_victim: ["record_deaths"],
  record_elder: ["record_dragon"],
  record_steal: ["record_dragon", "record_baron"],
};

interface EventGroup {
  /** i18n 키이자 `data-testid` 접미사. */
  key: string;
  /** 언제나 보이는 토글. */
  primary: readonly EventFlag[];
  /** 부모가 전부 켜져 있으면 접히는 토글. */
  subs: readonly EventFlag[];
}

/**
 * 화면 배치.
 *
 * `record_nexus` 는 일부러 없다 — 백엔드 어디에도 이 플래그를 읽는 코드가 없어서
 * 켜도 꺼도 아무 일이 일어나지 않는다(`settingSpecs.ts` 의 `SCENE_FLAGS` 참조).
 * 아무 일도 하지 않는 스위치를 놓는 순간 이 화면은 다시 거짓말을 시작한다.
 */
export const EVENT_GROUPS: readonly EventGroup[] = [
  {
    key: "kills",
    primary: ["record_kills"],
    subs: [
      "record_multikills",
      "record_shutdown",
      "record_outplay",
      "record_low_hp",
      "record_first_blood",
    ],
  },
  {
    key: "deaths",
    primary: ["record_deaths"],
    subs: ["record_trade_kill", "record_first_blood_victim"],
  },
  { key: "assists", primary: ["record_assists"], subs: [] },
  {
    key: "objectives",
    primary: [
      "record_dragon",
      "record_baron",
      "record_herald",
      "record_voidgrubs",
      "record_atakhan",
    ],
    subs: ["record_elder", "record_steal"],
  },
  {
    key: "structures",
    primary: ["record_turret", "record_inhibitor"],
    subs: [],
  },
  { key: "special", primary: ["record_ace", "record_game_end"], subs: [] },
];

/** 이 하위 상황을 지금 화면에 보여야 하는가 — 부모 중 하나라도 꺼져 있으면 그렇다. */
export function isSubSituationVisible(
  flag: EventFlag,
  filter: Partial<Record<EventFlag, boolean>>,
): boolean {
  const parents = SUB_SITUATION_PARENTS[flag];
  if (!parents || parents.length === 0) return true;
  return !parents.every((parent) => filter[parent] === true);
}

interface EventFilterSettingsProps {
  settings: EventFilterSettings;
  onChange: (settings: EventFilterSettings) => void;
}

interface ToggleRowProps {
  flag: EventFlag;
  label: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}

/**
 * 라벨과 스위치가 한 줄.
 *
 * 행 **전체**가 `<label>` 이다. 예전에는 바깥 `div` 에만 `min-h-[44px]` 이 있고
 * 라벨은 글자 높이(14px)만 차지해서, 44px 짜리 행처럼 보이는데 실제로 눌리는
 * 곳은 가운데 14px 띠와 20px 스위치뿐이었다 — 위아래 15px 씩은 눌러도 아무 일이
 * 일어나지 않는다. 행을 통째로 라벨로 만들면 어디를 눌러도 토글된다.
 */
function ToggleRow({ flag, label, checked, onCheckedChange }: ToggleRowProps) {
  return (
    <label
      htmlFor={flag}
      className="flex min-h-[44px] cursor-pointer items-center justify-between gap-4"
    >
      <span
        className="min-w-0 flex-1 text-sm"
        style={{ wordBreak: "keep-all" }}
      >
        {label}
      </span>
      <Switch id={flag} checked={checked} onCheckedChange={onCheckedChange} />
    </label>
  );
}

export function EventFilterSettings({
  settings,
  onChange,
}: EventFilterSettingsProps) {
  const { t } = useTranslation();

  const updateSetting = (
    key: keyof EventFilterSettings,
    value: boolean | number,
  ) => {
    onChange({ ...settings, [key]: value });
  };

  // 빠른 프리셋은 기본 화면의 "어떤 장면을 담을까"와 같은 조합을 쓴다.
  // 여기서만 다른 조합을 쓰면 고급에서 프리셋을 눌러도 기본 화면은 "직접 설정"이
  // 되어버려, 같은 앱이 두 가지 언어로 말하게 된다.
  const applyPreset = (preset: SelectableHighlightPreset) => {
    onChange(applyHighlightPreset(preset, settings));
  };

  const getPriorityLabel = (priority: number): string => {
    const labels = {
      1: t("settings.recordingConfig.eventFilter.priorityLabels.allEvents"),
      2: t(
        "settings.recordingConfig.eventFilter.priorityLabels.importantEvents",
      ),
      3: t("settings.recordingConfig.eventFilter.priorityLabels.highPriority"),
      4: t(
        "settings.recordingConfig.eventFilter.priorityLabels.criticalMoments",
      ),
      5: t("settings.recordingConfig.eventFilter.priorityLabels.epicPlaysOnly"),
    };
    return (
      labels[priority as keyof typeof labels] ||
      t("settings.recordingConfig.eventFilter.priorityLabels.custom")
    );
  };

  /** 장면 이름은 기본 화면과 같은 표를 쓴다 — 같은 것을 두 이름으로 부르지 않는다. */
  const sceneLabel = (flag: EventFlag) =>
    t(`settings.basic.highlights.scenes.${flag}`);

  return (
    <div className="space-y-6">
      {/* Presets */}
      <div>
        <h3 className="text-sm font-semibold mb-3">
          {t("settings.recordingConfig.eventFilter.quickPresets")}
        </h3>
        <div className="flex gap-2 flex-wrap">
          {SELECTABLE_HIGHLIGHT_PRESETS.map((preset) => (
            <Button
              key={preset}
              variant="outline"
              size="sm"
              onClick={() => applyPreset(preset)}
              data-testid={`preset-${preset}`}
            >
              {t(`settings.basic.highlights.options.${preset}.label`)}
            </Button>
          ))}
        </div>
      </div>

      {/* Priority Filter */}
      <div className="gaming-panel p-6">
        <div className="mb-4">
          <h3 className="text-lg font-semibold">
            {t("settings.recordingConfig.eventFilter.priorityFilter")}
          </h3>
          <p
            className="text-sm text-muted-foreground"
            style={{ wordBreak: "keep-all" }}
          >
            {t(
              "settings.recordingConfig.eventFilter.priorityFilterDescription",
            )}
          </p>
        </div>
        <div className="space-y-4">
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label>
                {t("settings.recordingConfig.eventFilter.minimumPriority")}
              </Label>
              <Badge variant="secondary">
                {getPriorityLabel(settings.min_priority)}
              </Badge>
            </div>
            <Slider
              value={[settings.min_priority]}
              onValueChange={([value]) => updateSetting("min_priority", value)}
              min={1}
              max={5}
              step={1}
              className="w-full"
              data-testid="priority-filter-slider"
            />
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>
                {t("settings.recordingConfig.eventFilter.priorityScale.all")}
              </span>
              <span>
                {t(
                  "settings.recordingConfig.eventFilter.priorityScale.important",
                )}
              </span>
              <span>
                {t("settings.recordingConfig.eventFilter.priorityScale.high")}
              </span>
              <span>
                {t(
                  "settings.recordingConfig.eventFilter.priorityScale.critical",
                )}
              </span>
              <span>
                {t("settings.recordingConfig.eventFilter.priorityScale.epic")}
              </span>
            </div>
          </div>
          {/* 문턱은 토글보다 먼저 걸리고, 강등으로 우회되지도 않는다. 켜 둔 것이
              조용히 버려지는 상태를 만들 수 있는 유일한 노브라 그 사실을 밝힌다. */}
          <p
            className="text-xs text-muted-foreground"
            style={{ wordBreak: "keep-all" }}
          >
            {t("settings.recordingConfig.eventFilter.priorityOverridesHint")}
          </p>
        </div>
      </div>

      {EVENT_GROUPS.map((group) => {
        const visibleSubs = group.subs.filter((flag) =>
          isSubSituationVisible(flag, settings),
        );
        // 접힌 하위 상황 = 부모가 켜져 있어 자동으로 함께 담기는 것들.
        // 그냥 감추면 "멀티킬은 어디 갔지?"가 되므로, 무엇이 포함됐는지 말한다.
        const includedSubs = group.subs.filter(
          (flag) => !isSubSituationVisible(flag, settings),
        );

        // 토글이 하나뿐인 그룹은 제목이 곧 그 토글의 이름이다. 줄을 따로 두면
        // "킬 / 킬" 처럼 같은 말이 연달아 나와, 둘이 다른 것인가 하고 멈추게 된다.
        const singleToggle =
          group.primary.length === 1 ? group.primary[0] : null;

        return (
          <div
            key={group.key}
            className="gaming-panel p-6"
            data-testid={`event-group-${group.key}`}
          >
            <div className="mb-4 flex items-start justify-between gap-4">
              <div className="min-w-0">
                <h3 className="text-lg font-semibold">
                  {t(
                    `settings.recordingConfig.eventFilter.groups.${group.key}.title`,
                  )}
                </h3>
                <p
                  className="text-sm text-muted-foreground"
                  style={{ wordBreak: "keep-all" }}
                >
                  {t(
                    `settings.recordingConfig.eventFilter.groups.${group.key}.description`,
                  )}
                </p>
              </div>
              {singleToggle && (
                // `Switch` 자체는 20x36px 이라 터치타깃에 한참 못 미친다.
                // `ToggleRow` 는 라벨이 행 전체를 덮어 실질 타깃이 44px 이지만,
                // 제목 줄에서는 옆에 눌릴 라벨이 없어 스위치가 유일한 과녁이다.
                // 빈 `label` 로 감싸 과녁만 넓힌다 — 접근성 이름은 `aria-label`
                // 이 그대로 제공한다(빈 라벨보다 우선한다).
                <label
                  htmlFor={singleToggle}
                  className="flex min-h-[44px] min-w-[44px] shrink-0 cursor-pointer items-center justify-end"
                >
                  <Switch
                    id={singleToggle}
                    aria-label={sceneLabel(singleToggle)}
                    checked={settings[singleToggle]}
                    onCheckedChange={(checked: boolean) =>
                      updateSetting(singleToggle, checked)
                    }
                  />
                </label>
              )}
            </div>

            {!singleToggle && (
              <div className="space-y-4">
                {group.primary.map((flag) => (
                  <ToggleRow
                    key={flag}
                    flag={flag}
                    label={sceneLabel(flag)}
                    checked={settings[flag]}
                    onCheckedChange={(checked) => updateSetting(flag, checked)}
                  />
                ))}
              </div>
            )}

            {includedSubs.length > 0 && (
              <p
                data-testid={`event-group-${group.key}-included`}
                className="mt-3 text-xs text-muted-foreground"
                style={{ wordBreak: "keep-all" }}
              >
                {t("settings.recordingConfig.eventFilter.includedHint", {
                  scenes: includedSubs.map(sceneLabel).join(" · "),
                })}
              </p>
            )}

            {visibleSubs.length > 0 && (
              <div
                data-testid={`event-group-${group.key}-exceptions`}
                className="mt-4 space-y-4 border-l-2 border-gaming-cyan/30 pl-4"
              >
                <p
                  className="text-sm font-medium"
                  style={{ wordBreak: "keep-all" }}
                >
                  {t("settings.recordingConfig.eventFilter.exceptionsLabel")}
                </p>
                {visibleSubs.map((flag) => (
                  <ToggleRow
                    key={flag}
                    flag={flag}
                    label={sceneLabel(flag)}
                    checked={settings[flag]}
                    onCheckedChange={(checked) => updateSetting(flag, checked)}
                  />
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
