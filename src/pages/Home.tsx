import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "@tanstack/react-router";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  Film,
  Pause,
  Play,
  Scissors,
  Sparkles,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { VideoModal } from "@/components/video/VideoModal";
import { ClipCard } from "@/components/clips/ClipCard";
import { GameSummary } from "@/components/game/GameSummary";
import { recordingApi } from "@/api/recording";
import { storageApi } from "@/api/storage";
import { videoApi } from "@/api/video";
import { lcuApi, type UnifiedGameStatus } from "@/api/lcu";
import { useToast } from "@/components/ui/use-toast";
import { useEditorStore } from "@/stores/editorStore";
import { useAutoEditStore } from "@/stores/autoEditStore";
import { logger } from "@/lib/logger";
import { getErrorMessage } from "@/lib/utils";
import { clipLabel } from "@/lib/clipLabel";
import { rankClips } from "@/lib/clipRanking";
import type { ClipMetadata, GameMetadata } from "@/types/storage";

/**
 * 홈 — "방금 판에서 나온 재료로 무엇을 만들지" 한 화면.
 *
 * 이전 대시보드는 녹화를 **조작하는** 화면이었다(자동 캡처 시작/정지, 수동 리플레이
 * 슬라이더, 녹화 설정 패널이 세로로 11개). 실측하면 1280x800 에서 1.8 화면분이라
 * 메인 화면인데도 스크롤해야 했고, 무엇보다 자동 파일럿 제품에서 사용자가 홈에서
 * 할 일은 녹화 조작이 아니다 — 앱이 알아서 녹화하므로 홈의 일은 **결과물을 고르는
 * 것**이다. 그래서 조작 계열은 설정으로 보내고, 연동 상태는 한 줄로 줄였다.
 *
 * 홈과 결과의 경계: 홈은 **최근 한 판의 재료**(선택해서 만들기), 결과는 **완성된
 * 영상과 지난 기록 전체**(보관함). 같은 것을 두 번 보여주지 않는다.
 */

/** 한 화면에 스크롤 없이 들어가는 개수. 1280x800 기준 4열 x 2행. */
const MAX_CLIPS_ON_HOME = 8;

export function Home() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { toast } = useToast();
  const setSelectedGameId = useEditorStore((s) => s.setSelectedGameId);
  const setPinnedClips = useAutoEditStore((s) => s.setPinnedClips);
  const setSelectedGameIds = useAutoEditStore((s) => s.setSelectedGameIds);

  const [gameStatus, setGameStatus] = useState<UnifiedGameStatus | null>(null);
  const [captureWarning, setCaptureWarning] = useState<string | null>(null);
  const [isTogglingCapture, setIsTogglingCapture] = useState(false);
  const [clips, setClips] = useState<ClipMetadata[]>([]);
  const [generating, setGenerating] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(true);
  const [videoSrc, setVideoSrc] = useState<string | null>(null);
  const [videoTitle, setVideoTitle] = useState("");

  /**
   * 이 화면이 보여주는 판. 액션(다듬기·만들기)이 **어느 판인지** 알아야 하는데
   * 지금까지 홈은 이 값을 갖고 있지도 않았고 넘기지도 않았다.
   */
  const [gameId, setGameId] = useState<string | null>(null);
  const [game, setGame] = useState<GameMetadata | null>(null);

  const refreshStatus = useCallback(async () => {
    const [gameResult, recordingResult] = await Promise.allSettled([
      lcuApi.getUnifiedGameStatus(),
      recordingApi.getStatus(),
    ]);
    if (gameResult.status === "fulfilled") {
      setGameStatus(gameResult.value);
    } else {
      // 상태 한 줄이 안 읽히는 것으로 화면 전체를 죽이지 않는다.
      logger.error("[Home] Failed to read game status:", gameResult.reason);
    }
    if (recordingResult.status === "fulfilled") {
      setCaptureWarning(recordingResult.value.capture_warning ?? null);
    }
  }, []);

  const loadRecentClips = useCallback(async () => {
    setIsLoading(true);
    try {
      const games = (await storageApi.listGames()) ?? [];
      if (games.length === 0) {
        setGameId(null);
        setGame(null);
        setClips([]);
        return;
      }

      const latest = games[0];
      setGameId(latest);

      // 판 맥락(챔피언·승패·KDA)은 **있으면 좋은 것**이다. 못 얻어도 클립은
      // 보여야 하므로 클립 로딩과 묶지 않고 따로 실패시킨다.
      storageApi
        .getGameMetadata(latest)
        .then((meta) => setGame(meta ?? null))
        .catch((error) => {
          logger.error("[Home] Failed to read game metadata:", error);
          setGame(null);
        });

      const list = (await storageApi.listClips(latest)) ?? [];

      // **점수순**이다. 만들어진 시각순이 아니다.
      //
      // 이 화면이 답하는 질문은 "이번 판에서 내가 뭘 잘했나" 이고, 마지막에
      // 저장된 어시스트가 그 판의 펜타킬 위에 놓이면 그 질문에 답하지 못한다.
      // 규칙은 `clipRanking.ts` — 백엔드와 같은 폴백을 쓰되 재사용 감쇠는 뺀다.
      const shown = rankClips(list).slice(0, MAX_CLIPS_ON_HOME);
      setClips(shown);

      // 자동 클립은 저장 시점에 썸네일이 붙지만(auto_clip_manager), 수동 저장분과
      // 예전 클립은 비어 있다. 없는 것만 만들어 붙인다 — 실패해도 카드는 아이콘으로 남는다.
      for (const clip of shown) {
        if (clip.thumbnail_path) continue;
        setGenerating((prev) => new Set(prev).add(clip.file_path));
        videoApi
          .generateClipThumbnail(clip.file_path)
          .then((thumbnailPath) => {
            if (!thumbnailPath) return;
            setClips((prev) =>
              prev.map((c) =>
                c.file_path === clip.file_path
                  ? { ...c, thumbnail_path: thumbnailPath }
                  : c,
              ),
            );
          })
          .catch((error) => {
            logger.error("[Home] Thumbnail failed for", clip.file_path, error);
          })
          .finally(() => {
            setGenerating((prev) => {
              const next = new Set(prev);
              next.delete(clip.file_path);
              return next;
            });
          });
      }
    } catch (error) {
      logger.error("[Home] Failed to load recent clips:", error);
      setClips([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * 편집기·자동편집으로 넘어갈 때 **어느 판인지, 무엇을 골랐는지** 알려준다.
   *
   * 예전에는 `navigate({to:"/editor"})` 만 했다 — gameId 도 선택도 넘기지 않아서
   * 홈에서 「만들기」를 누르면 빈 화면이 열렸고, 카드의 선택 체크는 아무 일도
   * 하지 않는 장식이었다. `Games.tsx` 는 처음부터 gameId 를 제대로 넘기고
   * 있었으므로 같은 방식을 쓰고, 클립 선택은 store 로 건넨다(경로 목록은 URL 에
   * 실을 것이 아니다).
   */
  const openWithGame = useCallback(
    (to: "/editor" | "/auto-edit") => {
      if (!gameId) return;

      setSelectedGameId(gameId);
      if (to === "/editor") {
        setPinnedClips(null);
        navigate({ to, search: { gameId } });
        return;
      }

      // 고른 게 있으면 그것만 쓰라고 넘긴다. 없으면 null 로 눕혀 자동 선택으로
      // 되돌린다 — 이전 방문의 선택이 남아 조용히 이번 영상을 제한하면 안 된다.
      setPinnedClips(
        selected.size > 0
          ? {
              groups: [
                {
                  gameId,
                  // 화면에 보이는 순서(=점수순)를 그대로 넘긴다.
                  paths: clips
                    .map((c) => c.file_path)
                    .filter((p) => selected.has(p)),
                },
              ],
            }
          : null,
      );

      setSelectedGameIds([gameId]);
      navigate({ to });
    },
    [
      clips,
      gameId,
      navigate,
      selected,
      setPinnedClips,
      setSelectedGameId,
      setSelectedGameIds,
    ],
  );

  useEffect(() => {
    refreshStatus();
    loadRecentClips();
    const timer = setInterval(refreshStatus, 5000);
    return () => clearInterval(timer);
  }, [refreshStatus, loadRecentClips]);

  const statusTone = useMemo(() => {
    if (gameStatus?.is_monitoring === false) return "paused" as const;
    if (gameStatus?.is_recording) return "recording" as const;
    if (gameStatus?.lcu_connected) return "ready" as const;
    return "idle" as const;
  }, [gameStatus]);

  const isMonitoring = gameStatus?.is_monitoring ?? false;

  /**
   * 자동 캡처 켜고 끄기.
   *
   * 이전 대시보드는 이걸 화면 한복판의 큰 버튼으로 두었지만, 자동 파일럿에서는
   * 평소에 누를 일이 없는 스위치다. 그렇다고 없애면 "지금 녹화 안 되게 하고 싶다"
   * (연습 게임, 대리 플레이, 화면에 뜨면 곤란한 것)를 할 수단이 사라지므로,
   * 상태 줄 안에 조용히 남긴다.
   */
  const toggleCapture = async () => {
    const monitoring = gameStatus?.is_monitoring ?? false;
    setIsTogglingCapture(true);
    try {
      if (monitoring) {
        await recordingApi.stopAutoCapture();
      } else {
        await recordingApi.startAutoCapture();
      }
      await refreshStatus();
    } catch (error) {
      logger.error("[Home] Failed to toggle auto capture:", error);
      toast({
        title: t(
          monitoring ? "home.status.stopFailed" : "home.status.startFailed",
        ),
        description: getErrorMessage(error),
        variant: "destructive",
      });
    } finally {
      setIsTogglingCapture(false);
    }
  };

  const toggle = (filePath: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(filePath)) next.delete(filePath);
      else next.add(filePath);
      return next;
    });
  };

  const selectedCount = selected.size;

  return (
    <div data-testid="home" className="flex min-h-full flex-col gap-5 p-6">
      {/* 연동 상태 — 한 줄. 이 화면의 주인공이 아니다. */}
      {captureWarning && (
        <Alert
          className="border-yellow-500/40 bg-yellow-500/10"
          data-testid="home-capture-warning"
        >
          <AlertTriangle className="h-4 w-4 text-yellow-400" />
          <AlertTitle>Capture privacy warning</AlertTitle>
          <AlertDescription className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span>{captureWarning}</span>
            <Link
              to="/settings"
              className="font-medium underline underline-offset-4"
            >
              Review capture settings
            </Link>
          </AlertDescription>
        </Alert>
      )}
      <div
        data-testid="home-status"
        className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border border-white/5 bg-white/[0.02] px-4 py-2.5 text-sm"
      >
        <span
          aria-hidden="true"
          className={[
            "h-2 w-2 shrink-0 rounded-full",
            statusTone === "recording"
              ? "bg-red-500"
              : statusTone === "ready"
                ? "bg-gaming-cyan"
                : "bg-muted-foreground",
          ].join(" ")}
        />
        <span className="font-medium">
          {t(`home.status.${statusTone}.title`)}
        </span>
        <span
          className="text-muted-foreground"
          style={{ wordBreak: "keep-all" }}
        >
          {t(`home.status.${statusTone}.description`)}
        </span>
        <div className="ml-auto flex items-center gap-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={toggleCapture}
            disabled={isTogglingCapture}
            data-testid="home-toggle-capture"
          >
            {isMonitoring ? (
              <Pause className="mr-1.5 h-4 w-4" aria-hidden="true" />
            ) : (
              <Play className="mr-1.5 h-4 w-4" aria-hidden="true" />
            )}
            {t(isMonitoring ? "home.status.pause" : "home.status.resume")}
          </Button>
          <Link
            to="/settings"
            className="text-xs text-muted-foreground underline-offset-4 hover:underline"
          >
            {t("home.status.settingsLink")}
          </Link>
        </div>
      </div>

      {/* 이번 판의 하이라이트 — 점수 높은 순 */}
      <section className="flex min-h-0 flex-1 flex-col gap-3">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          {/*
            어느 판인지부터 말한다. 메타데이터를 못 얻으면 일반 제목으로 떨어지되
            클립은 그대로 보인다 — 헤더 하나 때문에 화면이 비면 안 된다.
          */}
          {game ? (
            <GameSummary game={game} testId="home-game-summary" />
          ) : (
            <h1 className="text-lg font-semibold" data-autofocus tabIndex={-1}>
              {t("home.clips.title")}
            </h1>
          )}
          <Link
            to="/results"
            search={{ tab: "clips" }}
            className="text-sm text-muted-foreground underline-offset-4 hover:underline"
            data-testid="home-see-all"
          >
            {t("home.clips.seeAll")}
          </Link>
        </div>

        {isLoading ? (
          <div className="flex min-h-[320px] flex-1 items-center justify-center">
            <Spinner size="lg" />
          </div>
        ) : clips.length === 0 ? (
          <div
            data-testid="home-empty"
            className="flex min-h-[320px] flex-1 flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-white/10 px-6 py-10 text-center"
          >
            <Film
              className="h-10 w-10 text-muted-foreground"
              aria-hidden="true"
            />
            {/*
              빈 이유가 둘이고 할 일이 다르다.

              - 판 자체가 없다  -> 아직 녹화된 게임이 없다. 기다리거나 설정을 본다
              - 판은 있는데 0개 -> 녹화는 됐는데 담을 만한 게 없었다. 문턱을 낮춰야 한다

              같은 문구로 뭉개면 후자의 사용자는 "녹화가 안 됐나" 하고 엉뚱한 곳을 본다.
            */}
            <p className="text-base font-medium">
              {t(gameId ? "home.empty.noClips.title" : "home.empty.title")}
            </p>
            <p
              className="max-w-md text-sm text-muted-foreground"
              style={{ wordBreak: "keep-all" }}
            >
              {t(
                gameId
                  ? "home.empty.noClips.description"
                  : "home.empty.description",
              )}
            </p>
            <Button
              variant="outline"
              onClick={() => navigate({ to: "/settings" })}
              data-testid="home-empty-settings"
            >
              {t(gameId ? "home.empty.noClips.action" : "home.empty.action")}
            </Button>
          </div>
        ) : (
          <div
            data-testid="home-clip-grid"
            className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4"
          >
            {clips.map((clip, index) => {
              const { title } = clipLabel(clip);
              const text = t(title.key, title.params);
              return (
                <ClipCard
                  key={clip.file_path}
                  clip={clip}
                  // 목록은 점수순이므로 첫 장이 곧 이 판의 최고의 순간이다.
                  top={index === 0}
                  selected={selected.has(clip.file_path)}
                  generatingThumbnail={generating.has(clip.file_path)}
                  onToggle={() => toggle(clip.file_path)}
                  onPlay={() => {
                    setVideoSrc(convertFileSrc(clip.file_path));
                    setVideoTitle(text);
                  }}
                />
              );
            })}
          </div>
        )}
      </section>

      {/* 다음 행동 — 항상 화면 안에. */}
      {clips.length > 0 && (
        <div
          data-testid="home-actions"
          className="flex flex-wrap items-center gap-3 rounded-lg border border-white/5 bg-white/[0.02] px-4 py-3"
        >
          <span className="text-sm text-muted-foreground">
            {selectedCount > 0
              ? t("home.actions.selected", { count: selectedCount })
              : t("home.actions.selectHint")}
          </span>
          {selectedCount > 0 && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setSelected(new Set())}
              data-testid="home-clear-selection"
            >
              {t("home.actions.clear")}
            </Button>
          )}
          <div className="ml-auto flex flex-wrap gap-2">
            <Button
              variant="outline"
              onClick={() => openWithGame("/editor")}
              disabled={!gameId}
              data-testid="home-trim"
            >
              <Scissors className="mr-2 h-4 w-4" aria-hidden="true" />
              {t("home.actions.trim")}
            </Button>
            <Button
              onClick={() => openWithGame("/auto-edit")}
              disabled={!gameId}
              data-testid="home-make-highlight"
            >
              <Sparkles className="mr-2 h-4 w-4" aria-hidden="true" />
              {t("home.actions.makeHighlight")}
            </Button>
          </div>
        </div>
      )}

      <VideoModal
        isOpen={videoSrc !== null}
        onClose={() => setVideoSrc(null)}
        src={videoSrc ?? ""}
        title={videoTitle}
      />
    </div>
  );
}

export default Home;
