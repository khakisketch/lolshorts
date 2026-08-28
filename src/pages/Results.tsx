import { useEffect } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { ResultsViewer } from "@/components/results/ResultsViewer";
import { ClipVault } from "@/components/results/ClipVault";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Replays } from "@/pages/Replays";

type ResultsTab = "clips" | "highlights" | "games" | "replays";

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
  const search = useSearch({ from: "/results" });
  const navigate = useNavigate({ from: "/results" });
  const requestedTab: ResultsTab = search.tab ?? "clips";
  const tab = requestedTab === "games" ? "clips" : requestedTab;

  useEffect(() => {
    if (requestedTab !== "games") return;
    void navigate({
      search: { tab: "clips" },
      replace: true,
    });
  }, [navigate, requestedTab]);

  return (
    <div data-testid="results-page" className="space-y-6">
      <div>
        <h1
          className="text-2xl md:text-3xl font-bold"
          data-autofocus
          tabIndex={-1}
        >
          {t("results.title")}
        </h1>
        <p
          className="text-sm text-muted-foreground mt-1"
          style={{ wordBreak: "keep-all" }}
        >
          {t("results.pageDescription")}
        </p>
      </div>

      <Tabs
        value={tab}
        onValueChange={(value) =>
          void navigate({
            search: { tab: value as ResultsTab },
            replace: false,
          })
        }
      >
        <TabsList className="grid w-full max-w-2xl grid-cols-3 h-auto">
          <TabsTrigger
            value="clips"
            className="min-h-[44px]"
            data-testid="results-tab-clips"
          >
            {t("results.tabs.clips")}
          </TabsTrigger>
          <TabsTrigger
            value="highlights"
            className="min-h-[44px]"
            data-testid="results-tab-highlights"
          >
            {t("results.tabs.highlights")}
          </TabsTrigger>
          <TabsTrigger
            value="replays"
            className="min-h-[44px]"
            data-testid="results-tab-replays"
          >
            {t("results.tabs.replays")}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="clips" className="mt-6">
          <ClipVault />
        </TabsContent>

        <TabsContent value="highlights" className="mt-6">
          <ResultsViewer />
        </TabsContent>

        <TabsContent value="replays" className="mt-6">
          <Replays />
        </TabsContent>
      </Tabs>
    </div>
  );
}
