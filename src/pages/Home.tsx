import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "@tanstack/react-router";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Check, Film, Pause, Play, Scissors, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { VideoModal } from "@/components/video/VideoModal";
import { recordingApi } from "@/api/recording";
import { storageApi } from "@/api/storage";
import { videoApi } from "@/api/video";
import { lcuApi, type UnifiedGameStatus } from "@/api/lcu";
import { useToast } from "@/components/ui/use-toast";
import { logger } from "@/lib/logger";
import { getErrorMessage } from "@/lib/utils";
import { clipSeconds, eventLabel } from "@/lib/eventLabel";
import type { ClipMetadata } from "@/types/storage";

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

interface ClipCardProps {
  clip: ClipMetadata;
  selected: boolean;
  generatingThumbnail: boolean;
  onToggle: () => void;
  onPlay: () => void;
  label: string;
}

function ClipCard({
  clip,
  selected,
  generatingThumbnail,
  onToggle,
  onPlay,
  label,
}: ClipCardProps) {
  const { t } = useTranslation();
  const seconds = clipSeconds(clip.duration);
  const thumbnailSrc = clip.thumbnail_path
    ? convertFileSrc(clip.thumbnail_path)
    : undefined;

  return (
    <div className="group relative">
      <button
        type="button"
        onClick={onToggle}
        aria-pressed={selected}
        aria-label={t("home.clips.toggleLabel", { event: label, seconds })}
        data-testid={`home-clip-${clip.file_path}`}
        className={[
          "block w-full overflow-hidden rounded-lg border text-left transition-colors",
          selected
            ? "border-gaming-cyan bg-gaming-cyan/10"
            : "border-white/5 bg-white/[0.02] hover:border-gaming-cyan/40",
        ].join(" ")}
      >
        <span className="relative block aspect-video w-full bg-black/40">
          {thumbnailSrc && (
            <img
              src={thumbnailSrc}
              alt=""
              className="absolute inset-0 h-full w-full object-cover"
              onError={(e) => {
                e.currentTarget.style.visibility = "hidden";
              }}
            />
          )}
          {generatingThumbnail && !thumbnailSrc && (
            <span className="absolute inset-0 flex items-center justify-center">
              <Spinner size="sm" />
            </span>
          )}
          {!generatingThumbnail && !thumbnailSrc && (
            <span className="absolute inset-0 flex items-center justify-center">
              <Film className="h-8 w-8 text-muted-foreground" aria-hidden="true" />
            </span>
          )}
          <span className="absolute bottom-1.5 right-1.5 rounded bg-black/75 px-1.5 py-0.5 text-xs tabular-nums text-white">
            {t("home.clips.seconds", { count: seconds })}
          </span>
          {selected && (
            <span className="absolute left-1.5 top-1.5 flex h-6 w-6 items-center justify-center rounded-full bg-gaming-cyan text-black">
              <Check className="h-4 w-4" aria-hidden="true" />
            </span>
          )}
        </span>
        <span
          className="block truncate px-3 py-2 text-sm font-medium"
          style={{ wordBreak: "keep-all" }}
        >
          {label}
        </span>
      </button>

      {/* 재생은 선택과 다른 동작이므로 카드 버튼 안에 중첩하지 않는다. */}
      <button
        type="button"
        onClick={onPlay}
        aria-label={t("home.clips.playLabel", { event: label })}
        data-testid={`home-clip-play-${clip.file_path}`}
        className="absolute right-1.5 top-1.5 flex h-9 w-9 items-center justify-center rounded-full bg-black/70 text-white opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100"
      >
        <Play className="h-4 w-4" aria-hidden="true" />
      </button>
    </div>
  );
}

export function Home() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { toast } = useToast();

  const [gameStatus, setGameStatus] = useState<UnifiedGameStatus | null>(null);
  const [isTogglingCapture, setIsTogglingCapture] = useState(false);
  const [clips, setClips] = useState<ClipMetadata[]>([]);
  const [generating, setGenerating] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(true);
  const [videoSrc, setVideoSrc] = useState<string | null>(null);
  const [videoTitle, setVideoTitle] = useState("");

  const refreshStatus = useCallback(async () => {
    try {
      setGameStatus(await lcuApi.getUnifiedGameStatus());
    } catch (error) {
      // 상태 한 줄이 안 읽히는 것으로 화면 전체를 죽이지 않는다.
      logger.error("[Home] Failed to read game status:", error);
    }
  }, []);

  const loadRecentClips = useCallback(async () => {
    setIsLoading(true);
    try {
      const games = (await storageApi.listGames()) ?? [];
      if (games.length === 0) {
        setClips([]);
        return;
      }

      const latest = games[0];
      const list = (await storageApi.listClips(latest)) ?? [];
      // 새로 만들어진 것이 앞에. 백엔드 정렬에 기대지 않는다.
      const ordered = [...list].sort((a, b) =>
        String(b.created_at).localeCompare(String(a.created_at)),
      );
      const shown = ordered.slice(0, MAX_CLIPS_ON_HOME);
      setClips(shown);

      // 자동 클립은 `thumbnail_path: None` 으로 저장되므로(auto_clip_manager)
      // 여기서 만들어 붙인다. 실패해도 카드는 아이콘으로 남는다.
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
        <span className="font-medium">{t(`home.status.${statusTone}.title`)}</span>
        <span className="text-muted-foreground" style={{ wordBreak: "keep-all" }}>
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

      {/* 재료 */}
      <section className="flex min-h-0 flex-1 flex-col gap-3">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <h1 className="text-lg font-semibold">{t("home.clips.title")}</h1>
          <Link
            to="/results"
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
            <Film className="h-10 w-10 text-muted-foreground" aria-hidden="true" />
            <p className="text-base font-medium">{t("home.empty.title")}</p>
            <p
              className="max-w-md text-sm text-muted-foreground"
              style={{ wordBreak: "keep-all" }}
            >
              {t("home.empty.description")}
            </p>
            <Button
              variant="outline"
              onClick={() => navigate({ to: "/settings" })}
              data-testid="home-empty-settings"
            >
              {t("home.empty.action")}
            </Button>
          </div>
        ) : (
          <div
            data-testid="home-clip-grid"
            className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4"
          >
            {clips.map((clip) => {
              const label = eventLabel(clip.event_type);
              const text = t(label.key, label.params ?? {});
              return (
                <ClipCard
                  key={clip.file_path}
                  clip={clip}
                  label={text}
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
              onClick={() => navigate({ to: "/editor" })}
              data-testid="home-trim"
            >
              <Scissors className="mr-2 h-4 w-4" aria-hidden="true" />
              {t("home.actions.trim")}
            </Button>
            <Button
              onClick={() => navigate({ to: "/auto-edit" })}
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
