import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Film, Play } from "lucide-react";
import { Spinner } from "@/components/ui/spinner";
import { clipSeconds } from "@/lib/eventLabel";
import { clipLabel } from "@/lib/clipLabel";
import type { ClipMetadata } from "@/types/storage";

interface ClipCardProps {
  clip: ClipMetadata;
  selected: boolean;
  generatingThumbnail: boolean;
  onToggle: () => void;
  onPlay: () => void;
  /**
   * 이 판에서 가장 볼 만한 순간인가 — 목록 1위에만 준다.
   *
   * "게임이 끝나면 내 하이라이트가 뭐였는지 바로 파악" 이 이 화면의 일이다.
   * 정렬만으로는 1위와 2위가 같은 무게로 읽히므로 배지 하나로 눈을 끈다.
   */
  top?: boolean;
  testIdPrefix?: string;
}

/**
 * 클립 한 장.
 *
 * `Home.tsx` 안에 갇혀 있던 것을 꺼냈다 — 결과 화면에도 클립 목록이 필요해질
 * 참인데(지금 결과 3개 탭 어디에도 개별 클립이 없어서 홈 상위 8개를 넘어가면
 * 앱에서 볼 방법이 없다) 파일 안에 있으면 재사용할 수 없다.
 *
 * 라벨은 `clipLabel()` 이 제목과 이유를 **함께** 만든다 — 따로 만들면 「1대3
 * 아웃플레이」 제목에 「1대4」 이유가 붙는 조합이 나온다(서로 다른 것을 세는
 * 두 값이다).
 */
export function ClipCard({
  clip,
  selected,
  generatingThumbnail,
  onToggle,
  onPlay,
  top = false,
  testIdPrefix = "home-clip",
}: ClipCardProps) {
  const { t } = useTranslation();
  const seconds = clipSeconds(clip.duration);
  const thumbnailSrc = clip.thumbnail_path
    ? convertFileSrc(clip.thumbnail_path)
    : undefined;

  // 이 클립이 왜 뽑혔는지. 앱이 확언할 수 있는 유일한 것이고(Live Client API 로
  // 그 순간의 체력·생존 인원을 직접 받는다) 오랫동안 저장만 되고 화면에는
  // 나오지 않았다.
  const { title, reasons } = clipLabel(clip);
  const label = t(title.key, title.params);

  return (
    <div className="group relative h-full">
      <div
        data-testid={`${testIdPrefix}-${clip.file_path}`}
        className={[
          // `h-full`: 이유 줄이 없는 카드(상황을 못 찍은 클립)가 한 줄만큼
          // 짧아져 격자 아래 모서리가 들쭉날쭉해졌다.
          "flex h-full w-full flex-col overflow-hidden rounded-lg border text-left transition-colors",
          selected
            ? "border-gaming-cyan bg-gaming-cyan/10"
            : top
              ? "border-gaming-cyan/40 bg-white/[0.02] hover:border-gaming-cyan/60"
              : "border-white/5 bg-white/[0.02] hover:border-gaming-cyan/40",
        ].join(" ")}
      >
        {/* 재생과 선택은 서로 다른 행동이다. 썸네일은 언제나 재생되고,
            선택은 명시적인 체크박스로만 바뀌게 해 실수로 선택되지 않게 한다. */}
        <button
          type="button"
          onClick={onPlay}
          aria-label={t("home.clips.playLabel", { event: label })}
          data-testid={`${testIdPrefix}-play-${clip.file_path}`}
          className="relative block aspect-video w-full shrink-0 bg-black text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gaming-cyan focus-visible:ring-inset"
        >
          {thumbnailSrc && (
            <img
              src={thumbnailSrc}
              alt=""
              className="absolute inset-0 h-full w-full object-contain"
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
              <Film
                className="h-8 w-8 text-muted-foreground"
                aria-hidden="true"
              />
            </span>
          )}
          <span className="absolute right-2 top-2 flex h-9 w-9 items-center justify-center rounded-full bg-black/75 text-white transition-colors group-hover:bg-gaming-cyan group-hover:text-black">
            <Play className="h-4 w-4" aria-hidden="true" />
          </span>
          <span className="absolute bottom-1.5 right-1.5 rounded bg-black/75 px-1.5 py-0.5 text-xs tabular-nums text-white">
            {t("home.clips.seconds", { count: seconds })}
          </span>
        </button>

        <label className="absolute left-2 top-2 z-10 flex h-9 w-9 cursor-pointer items-center justify-center rounded-full bg-black/75 text-white shadow-sm transition-colors hover:bg-gaming-cyan hover:text-black">
          <input
            type="checkbox"
            checked={selected}
            onChange={onToggle}
            aria-label={t("home.clips.toggleLabel", { event: label, seconds })}
            data-testid={`${testIdPrefix}-select-${clip.file_path}`}
            className="h-5 w-5 cursor-pointer accent-cyan-400"
          />
        </label>

        <span className="block px-3 py-2">
          <span className="flex items-baseline justify-between gap-2">
            <span
              className="min-w-0 truncate text-sm font-medium"
              style={{ wordBreak: "keep-all" }}
            >
              {label}
            </span>
            {top && (
              <span
                className="shrink-0 text-[10px] font-bold uppercase tracking-wider text-gaming-cyan"
                data-testid="home-clip-top"
              >
                {t("home.clips.topMoment")}
              </span>
            )}
          </span>
          {reasons.length > 0 && (
            <span
              className="mt-0.5 block truncate text-xs text-gaming-cyan/80"
              style={{ wordBreak: "keep-all" }}
              data-testid={`${testIdPrefix}-reasons-${clip.file_path}`}
            >
              {/* 가운뎃점으로 잇는다 — 배지 여러 개는 카드 폭을 넘고, 이 줄은
                  읽히는 것이 목적이지 클릭 대상이 아니다. */}
              {reasons.map((r) => t(r.key, r.params)).join(" · ")}
            </span>
          )}
        </span>
      </div>
    </div>
  );
}
