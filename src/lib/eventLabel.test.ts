import fs from 'fs';
import path from 'path';
import { clipSeconds, eventLabel } from './eventLabel';
import ko from '@/locales/ko/translation.json';

describe('eventLabel', () => {
  it('단순 변형을 사람 말 키로 바꾼다', () => {
    expect(eventLabel('champion_kill')).toEqual({ key: 'events.championKill' });
    expect(eventLabel('baron_kill')).toEqual({ key: 'events.baronKill' });
    expect(eventLabel('ace')).toEqual({ key: 'events.ace' });
    expect(eventLabel('first_blood')).toEqual({ key: 'events.firstBlood' });
  });

  it('멀티킬은 숫자가 아니라 이름으로 부른다', () => {
    expect(eventLabel({ multikill: 2 })).toEqual({ key: 'events.multikill.double' });
    expect(eventLabel({ multikill: 3 })).toEqual({ key: 'events.multikill.triple' });
    expect(eventLabel({ multikill: 4 })).toEqual({ key: 'events.multikill.quadra' });
    expect(eventLabel({ multikill: 5 })).toEqual({ key: 'events.multikill.penta' });
  });

  it('이름이 없는 멀티킬 수는 숫자로 흘린다', () => {
    expect(eventLabel({ multikill: 6 })).toEqual({
      key: 'events.multikill.other',
      params: { count: 6 },
    });
  });

  it('custom 은 그 이름을 쓴다', () => {
    expect(eventLabel({ custom: '수동 저장' })).toEqual({
      key: 'events.custom',
      params: { name: '수동 저장' },
    });
  });

  it('모르는 값은 코드값을 노출하지 않고 일반 명칭으로 받는다', () => {
    // 백엔드가 새 변형을 추가했을 때 화면에 `void_grub_kill` 이 뜨면 안 된다.
    const label = eventLabel('void_grub_kill' as never);
    expect(label).toEqual({ key: 'events.unknown', unknown: true });
    expect(JSON.stringify(label)).not.toMatch(/void_grub/);
  });

  it('null 과 빈 custom 도 안전하다', () => {
    expect(eventLabel(null).unknown).toBe(true);
    expect(eventLabel(undefined).unknown).toBe(true);
    expect(eventLabel({ custom: '   ' }).unknown).toBe(true);
  });

  it('돌려주는 키가 ko 로케일에 전부 존재한다', () => {
    // 키만 돌려주는 설계라, 로케일에 없으면 화면에 키 문자열이 그대로 뜬다.
    const keys = [
      eventLabel('champion_kill'),
      eventLabel('turret_kill'),
      eventLabel('inhibitor_kill'),
      eventLabel('dragon_kill'),
      eventLabel('baron_kill'),
      eventLabel('ace'),
      eventLabel('first_blood'),
      eventLabel({ multikill: 2 }),
      eventLabel({ multikill: 3 }),
      eventLabel({ multikill: 4 }),
      eventLabel({ multikill: 5 }),
      eventLabel({ multikill: 9 }),
      eventLabel({ custom: 'x' }),
      eventLabel(null),
    ].map((l) => l.key);

    for (const key of keys) {
      const value = key
        .split('.')
        .reduce<unknown>((node, part) => (node as Record<string, unknown>)?.[part], ko);
      expect(typeof value).toBe('string');
    }
  });

  it('ko 와 en 이 같은 키 집합을 가진다', () => {
    const enPath = path.resolve(__dirname, '../locales/en/translation.json');
    const en = JSON.parse(fs.readFileSync(enPath, 'utf8'));
    const flatten = (node: unknown, prefix = ''): string[] =>
      typeof node === 'object' && node !== null
        ? Object.entries(node).flatMap(([k, v]) => flatten(v, prefix ? `${prefix}.${k}` : k))
        : [prefix];
    expect(flatten((en as Record<string, unknown>).events).sort()).toEqual(
      flatten((ko as Record<string, unknown>).events).sort(),
    );
  });
});

describe('clipSeconds', () => {
  it('초를 반올림한다', () => {
    expect(clipSeconds(12.4)).toBe(12);
    expect(clipSeconds(12.6)).toBe(13);
  });

  it('0초 클립은 만들지 않는다 — 아주 짧아도 1초로 보인다', () => {
    expect(clipSeconds(0.3)).toBe(1);
  });

  it('이상한 값은 0 으로 떨어뜨린다', () => {
    expect(clipSeconds(0)).toBe(0);
    expect(clipSeconds(-5)).toBe(0);
    expect(clipSeconds(NaN)).toBe(0);
  });
});
