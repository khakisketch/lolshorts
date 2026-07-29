import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ResultsViewer } from '@/components/results/ResultsViewer';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Games } from '@/pages/Games';
import { Replays } from '@/pages/Replays';

const RESULTS_TABS = ['highlights', 'games', 'replays'] as const;
type ResultsTab = (typeof RESULTS_TABS)[number];

function isResultsTab(value: string | null): value is ResultsTab {
  return value !== null && (RESULTS_TABS as readonly string[]).includes(value);
}

/**
 * Deep links and the redirects from the retired /games, /replays, /auto-edit and
 * /youtube routes carry `?tab=`. Anything else (including a bare /results) lands
 * on the highlights list, so the screen always opens with something to look at.
 */
function getInitialTab(): ResultsTab {
  try {
    const tab = new URLSearchParams(window.location.search).get('tab');
    return isResultsTab(tab) ? tab : 'highlights';
  } catch {
    return 'highlights';
  }
}

/**
 * "결과" — everything the user owns in one screen: the shorts made for them,
 * the games that were recorded, and the replays available from the client.
 * Editing and sharing are entered from an item here, not from the sidebar.
 *
 * **로그인을 요구하지 않는다.** 여기 있는 것은 전부 이미 사용자 PC 에 있는
 * 사용자의 파일이다. 녹화가 로그인 없이 되는데 그 결과를 보려는 순간 로그인
 * 창이 뜨면, 게임을 한 판 다 마친 사람이 "내 영상이 사라졌나" 하고 앱을 지운다.
 * 인증은 엔진이 실제로 도는 순간(자동편집·내보내기·업로드)에만 요구한다 —
 * `auth::command_policy` 의 `the_engine_never_runs_for_a_logged_out_user` 참조.
 */
export function Results() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<ResultsTab>(getInitialTab);

  return (
    <div data-testid="results-page" className="space-y-6">
        <div>
          <h1
            className="text-2xl md:text-3xl font-bold"
            data-autofocus
            tabIndex={-1}
          >
            {t('results.title')}
          </h1>
          <p
            className="text-sm text-muted-foreground mt-1"
            style={{ wordBreak: 'keep-all' }}
          >
            {t('results.pageDescription')}
          </p>
        </div>

        <Tabs value={tab} onValueChange={(value) => setTab(value as ResultsTab)}>
          <TabsList className="grid w-full max-w-lg grid-cols-3 h-auto">
            <TabsTrigger
              value="highlights"
              className="min-h-[44px]"
              data-testid="results-tab-highlights"
            >
              {t('results.tabs.highlights')}
            </TabsTrigger>
            <TabsTrigger
              value="games"
              className="min-h-[44px]"
              data-testid="results-tab-games"
            >
              {t('results.tabs.games')}
            </TabsTrigger>
            <TabsTrigger
              value="replays"
              className="min-h-[44px]"
              data-testid="results-tab-replays"
            >
              {t('results.tabs.replays')}
            </TabsTrigger>
          </TabsList>

          <TabsContent value="highlights" className="mt-6">
            <ResultsViewer />
          </TabsContent>

          <TabsContent value="games" className="mt-6">
            <Games />
          </TabsContent>

          <TabsContent value="replays" className="mt-6">
            <Replays />
          </TabsContent>
        </Tabs>
    </div>
  );
}
