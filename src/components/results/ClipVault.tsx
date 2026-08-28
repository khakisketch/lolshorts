import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
import { useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  Calendar,
  ChevronDown,
  ChevronRight,
  Clock,
  Filter,
  Film,
  PanelLeftClose,
  PanelLeftOpen,
  Play,
  Search,
  Scissors,
  Sparkles,
  Trash2,
} from "lucide-react";
import { storageApi } from "@/api/storage";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useConfirmDialog } from "@/components/ui/confirm-dialog";
import { SpinnerCenter } from "@/components/ui/spinner";
import { VideoPlayer } from "@/components/video/VideoPlayer";
import { clipLabel } from "@/lib/clipLabel";
import { createClipThumbnailQueue } from "@/lib/clipThumbnailQueue";
import { clipSeconds } from "@/lib/eventLabel";
import { formatDuration, formatStorage } from "@/lib/utils";
import { type PinnedClipGroup, useAutoEditStore } from "@/stores/autoEditStore";
import { useEditorStore } from "@/stores/editorStore";
import type {
  ClipMetadata,
  ClipVaultGameGroup,
  ClipVaultSort,
  StorageStats,
} from "@/types/storage";

const PAGE_SIZE = 6;
const thumbnailQueue = createClipThumbnailQueue(
  ({ gameId, clipFilePath }) =>
    storageApi.ensureClipThumbnail(gameId, clipFilePath),
  2,
);

const selectionKey = (gameId: string, path: string) => `${gameId}\u0000${path}`;

interface SelectedClip {
  gameId: string;
  path: string;
  duration: number;
}

interface ActiveClip extends SelectedClip {
  clip: ClipMetadata;
}

export interface ClipVaultProps {
  onSelectionChange?: (groups: PinnedClipGroup[]) => void;
  onCreateMontage?: (groups: PinnedClipGroup[]) => void;
}

function useVisibleThumbnail(
  gameId: string,
  clip: ClipMetadata,
  onGenerated: (path: string) => void,
) {
  const ref = useRef<HTMLDivElement>(null);
  const [path, setPath] = useState(clip.thumbnail_path ?? null);
  const [failed, setFailed] = useState(false);
  const [version, setVersion] = useState(0);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    setPath(clip.thumbnail_path ?? null);
    setFailed(false);
    setVersion(0);
    setVisible(false);
  }, [clip.file_path, clip.thumbnail_path]);

  const ensure = useCallback(() => {
    const pending = thumbnailQueue.request({
      gameId,
      clipFilePath: clip.file_path,
    });
    if (!pending) return;
    void pending
      .then((generatedPath) => {
        setPath(generatedPath);
        setFailed(false);
        setVersion(Date.now());
        onGenerated(generatedPath);
      })
      .catch(() => {
        setFailed(true);
      });
  }, [clip.file_path, gameId, onGenerated]);

  useEffect(() => {
    const node = ref.current;
    if (!node || typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "160px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [clip.file_path]);

  useEffect(() => {
    if (visible && !path) ensure();
  }, [ensure, path, visible]);

  const onImageError = useCallback(() => {
    setFailed(true);
    setPath(null);
    if (visible) ensure();
  }, [ensure, visible]);

  return {
    ref,
    src:
      path && !failed
        ? `${convertFileSrc(path)}${version ? `?v=${version}` : ""}`
        : null,
    onImageError,
  };
}

interface VaultThumbnailProps {
  gameId: string;
  clip: ClipMetadata;
  onGenerated: (path: string) => void;
}

function VaultThumbnail({ gameId, clip, onGenerated }: VaultThumbnailProps) {
  const { ref, src, onImageError } = useVisibleThumbnail(
    gameId,
    clip,
    onGenerated,
  );
  return (
    <div ref={ref as RefObject<HTMLDivElement>} className="absolute inset-0">
      {src ? (
        <img
          src={src}
          alt=""
          loading="lazy"
          className="h-full w-full object-contain"
          onError={onImageError}
        />
      ) : (
        <Film
          className="absolute inset-0 m-auto h-9 w-9 text-muted-foreground"
          aria-hidden="true"
        />
      )}
    </div>
  );
}

export function ClipVault({
  onSelectionChange,
  onCreateMontage,
}: ClipVaultProps) {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const setPinnedClips = useAutoEditStore((state) => state.setPinnedClips);
  const setSelectedGameIds = useAutoEditStore(
    (state) => state.setSelectedGameIds,
  );
  const targetDuration = useAutoEditStore((state) => state.targetDuration);
  const setSelectedGameId = useEditorStore((state) => state.setSelectedGameId);
  const { confirm, ConfirmDialog } = useConfirmDialog();

  const [groups, setGroups] = useState<ClipVaultGameGroup[]>([]);
  const [sortOrder, setSortOrder] = useState<ClipVaultSort>("best");
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [skippedItems, setSkippedItems] = useState(0);
  const [selected, setSelected] = useState<Map<string, SelectedClip>>(
    () => new Map(),
  );
  const [activeClip, setActiveClip] = useState<ActiveClip | null>(null);
  const [eventListOpen, setEventListOpen] = useState(true);
  const [expandedGameIds, setExpandedGameIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [searchQuery, setSearchQuery] = useState("");
  const [appliedQuery, setAppliedQuery] = useState("");
  const [gameMode, setGameMode] = useState("all");
  const [availableGameModes, setAvailableGameModes] = useState<string[]>([]);
  const [storageStats, setStorageStats] = useState<StorageStats | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "error">(
    "loading",
  );
  const [loadingMore, setLoadingMore] = useState(false);
  const requestId = useRef(0);

  const loadPage = useCallback(
    async (
      reset: boolean,
      cursor: string | null,
      sort: ClipVaultSort,
      query = "",
      mode = "all",
    ) => {
      const id = ++requestId.current;
      if (reset) setStatus("loading");
      else setLoadingMore(true);
      try {
        const input = {
          sort,
          cursor,
          game_limit: PAGE_SIZE,
          ...(query ? { query } : {}),
          ...(mode !== "all" ? { game_mode: mode } : {}),
        };
        const page = await storageApi.listClipVaultPage(input);
        if (id !== requestId.current) return;
        setGroups((current) =>
          reset ? page.groups : [...current, ...page.groups],
        );
        if (reset) {
          setExpandedGameIds(new Set());
          setActiveClip(null);
        }
        setNextCursor(page.next_cursor);
        // The storage page reports the library-wide corrupt-row count, so later
        // pages must not add the same omissions again.
        setSkippedItems((current) =>
          reset
            ? page.skipped_item_count
            : Math.max(current, page.skipped_item_count),
        );
        setStatus("ready");
      } catch {
        if (id !== requestId.current) return;
        if (reset) setStatus("error");
      } finally {
        if (id === requestId.current) setLoadingMore(false);
      }
    },
    [],
  );

  useEffect(() => {
    const timer = window.setTimeout(
      () => setAppliedQuery(searchQuery.trim()),
      250,
    );
    return () => window.clearTimeout(timer);
  }, [searchQuery]);

  useEffect(() => {
    setGroups([]);
    setNextCursor(null);
    setSkippedItems(0);
    void loadPage(true, null, sortOrder, appliedQuery, gameMode);
  }, [appliedQuery, gameMode, loadPage, sortOrder]);

  useEffect(() => {
    if (!activeClip) return;
    const activeStillPresent = groups.some(
      (group) =>
        group.game_id === activeClip.gameId &&
        group.clips.some((clip) => clip.file_path === activeClip.path),
    );
    if (!activeStillPresent) setActiveClip(null);
  }, [activeClip, groups]);

  useEffect(() => {
    // Derive the choices from the currently loaded result set. Retaining the
    // previous union made a mode from an earlier search remain selectable even
    // after the query/filter had removed every matching game.
    const next = new Set(
      groups
        .map((group) => group.game?.game_mode)
        .filter((mode): mode is string => Boolean(mode)),
    );
    setAvailableGameModes([...next].sort());
  }, [groups]);

  useEffect(() => {
    const getStats = storageApi.getStorageStats;
    if (typeof getStats !== "function") return;
    void getStats()
      .then(setStorageStats)
      .catch(() => setStorageStats(null));
  }, []);

  const selectedGroups = useMemo(() => {
    const byGame = new Map<string, string[]>();
    for (const item of selected.values()) {
      const paths = byGame.get(item.gameId) ?? [];
      if (!paths.includes(item.path)) paths.push(item.path);
      byGame.set(item.gameId, paths);
    }
    return [...byGame.entries()].map(([gameId, paths]) => ({ gameId, paths }));
  }, [selected]);

  const totalDuration = useMemo(
    () => [...selected.values()].reduce((sum, clip) => sum + clip.duration, 0),
    [selected],
  );

  useEffect(() => {
    onSelectionChange?.(selectedGroups);
  }, [onSelectionChange, selectedGroups]);

  const toggleSelection = useCallback((gameId: string, clip: ClipMetadata) => {
    setSelected((current) => {
      const next = new Map(current);
      const key = selectionKey(gameId, clip.file_path);
      if (next.has(key)) next.delete(key);
      else {
        next.set(key, {
          gameId,
          path: clip.file_path,
          duration: Math.max(0, clip.duration),
        });
      }
      return next;
    });
  }, []);

  const toggleGroup = useCallback((group: ClipVaultGameGroup) => {
    setSelected((current) => {
      const next = new Map(current);
      const allSelected = group.clips.every((clip) =>
        next.has(selectionKey(group.game_id, clip.file_path)),
      );
      for (const clip of group.clips) {
        const key = selectionKey(group.game_id, clip.file_path);
        if (allSelected) next.delete(key);
        else {
          next.set(key, {
            gameId: group.game_id,
            path: clip.file_path,
            duration: Math.max(0, clip.duration),
          });
        }
      }
      return next;
    });
  }, []);

  const toggleGameExpanded = useCallback((gameId: string) => {
    setExpandedGameIds((current) => {
      const next = new Set(current);
      if (next.has(gameId)) next.delete(gameId);
      else next.add(gameId);
      return next;
    });
  }, []);

  const expandAllGames = useCallback(() => {
    setExpandedGameIds(new Set(groups.map((group) => group.game_id)));
  }, [groups]);

  const collapseAllGames = useCallback(() => {
    setExpandedGameIds(new Set());
  }, []);

  const refreshStorageStats = useCallback(async () => {
    const getStats = storageApi.getStorageStats;
    if (typeof getStats !== "function") return;
    try {
      setStorageStats(await getStats());
    } catch {
      setStorageStats(null);
    }
  }, []);

  const handlePolish = useCallback(
    (gameId: string) => {
      setPinnedClips(null);
      setSelectedGameId(gameId);
      void navigate({ to: "/editor", search: { gameId } });
    },
    [navigate, setPinnedClips, setSelectedGameId],
  );

  const handleCreateHighlight = useCallback(
    (gameId: string) => {
      setPinnedClips(null);
      setSelectedGameId(gameId);
      void navigate({ to: "/auto-edit", search: { gameId } });
    },
    [navigate, setPinnedClips, setSelectedGameId],
  );

  const handleDeleteGame = useCallback(
    async (gameId: string) => {
      const confirmed = await confirm({
        title: t("games.deleteConfirmTitle"),
        description: t("games.deleteConfirmDescription"),
        confirmText: t("common.delete"),
        cancelText: t("common.cancel"),
        variant: "danger",
      });
      if (!confirmed) return;

      try {
        const deleteGame = storageApi.deleteGame;
        if (typeof deleteGame !== "function") return;
        await deleteGame(gameId);
        setSelected((current) => {
          const next = new Map(current);
          for (const [key, clip] of next) {
            if (clip.gameId === gameId) next.delete(key);
          }
          return next;
        });
        setExpandedGameIds((current) => {
          const next = new Set(current);
          next.delete(gameId);
          return next;
        });
        if (activeClip?.gameId === gameId) setActiveClip(null);
        await Promise.all([
          loadPage(true, null, sortOrder, appliedQuery, gameMode),
          refreshStorageStats(),
        ]);
      } catch {
        // The storage layer has already logged the failure; keep the current
        // library visible so a transient delete error does not erase context.
      }
    },
    [
      activeClip?.gameId,
      appliedQuery,
      confirm,
      gameMode,
      loadPage,
      refreshStorageStats,
      sortOrder,
      t,
    ],
  );

  const updateThumbnail = useCallback(
    (gameId: string, filePath: string, thumbnailPath: string) => {
      setGroups((current) =>
        current.map((group) =>
          group.game_id !== gameId
            ? group
            : {
                ...group,
                clips: group.clips.map((clip) =>
                  clip.file_path === filePath
                    ? { ...clip, thumbnail_path: thumbnailPath }
                    : clip,
                ),
              },
        ),
      );
    },
    [],
  );

  const makeMontage = useCallback(() => {
    if (selectedGroups.length === 0) return;
    setPinnedClips({ groups: selectedGroups });
    setSelectedGameIds(selectedGroups.map((group) => group.gameId));
    onCreateMontage?.(selectedGroups);
    if (!onCreateMontage) void navigate({ to: "/auto-edit", search: {} });
  }, [
    navigate,
    onCreateMontage,
    selectedGroups,
    setPinnedClips,
    setSelectedGameIds,
  ]);

  if (status === "loading") {
    return (
      <div data-testid="clip-vault-loading">
        <SpinnerCenter label={t("results.clips.loading")} />
      </div>
    );
  }

  if (status === "error") {
    return (
      <div
        className="rounded-lg border border-destructive/40 bg-destructive/10 p-6 text-center"
        data-testid="clip-vault-error"
      >
        <p className="text-sm text-muted-foreground">
          {t("results.clips.loadError")}
        </p>
        <Button
          className="mt-4"
          variant="outline"
          onClick={() =>
            void loadPage(true, null, sortOrder, appliedQuery, gameMode)
          }
        >
          {t("results.refresh")}
        </Button>
      </div>
    );
  }

  if (groups.length === 0) {
    if (appliedQuery || gameMode !== "all") {
      return (
        <div
          className="flex flex-col items-center justify-center rounded-lg border border-dashed border-white/10 p-10 text-center"
          data-testid="clip-vault-no-filter-results"
        >
          <Search
            className="mb-3 h-10 w-10 text-muted-foreground"
            aria-hidden="true"
          />
          <h3 className="font-semibold">{t("results.clips.noMatches")}</h3>
          <Button
            className="mt-4"
            variant="outline"
            onClick={() => {
              setSearchQuery("");
              setGameMode("all");
            }}
          >
            {t("results.clips.clearFilters")}
          </Button>
        </div>
      );
    }
    return (
      <div
        className="flex flex-col items-center justify-center rounded-lg border border-dashed border-white/10 p-10 text-center"
        data-testid="clip-vault-empty"
      >
        <Film
          className="mb-3 h-10 w-10 text-muted-foreground"
          aria-hidden="true"
        />
        <h3 className="font-semibold">{t("results.clips.emptyTitle")}</h3>
        <p className="mt-1 text-sm text-muted-foreground">
          {t("results.clips.emptyDescription")}
        </p>
      </div>
    );
  }

  return (
    <section
      aria-label={t("results.clips.title")}
      className={selected.size > 0 ? "pb-32" : undefined}
    >
      <div className="mb-5 space-y-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold">
              {t("results.clips.title")}
            </h2>
            <p className="text-sm text-muted-foreground">
              {t("results.clips.description")}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <div
              className="flex rounded-md border border-white/10 p-1"
              aria-label={t("results.clips.sortLabel")}
            >
              <Button
                size="sm"
                variant={sortOrder === "best" ? "secondary" : "ghost"}
                aria-pressed={sortOrder === "best"}
                onClick={() => setSortOrder("best")}
              >
                {t("results.clips.sortRecommended")}
              </Button>
              <Button
                size="sm"
                variant={sortOrder === "newest" ? "secondary" : "ghost"}
                aria-pressed={sortOrder === "newest"}
                onClick={() => setSortOrder("newest")}
              >
                {t("results.clips.sortNewest")}
              </Button>
            </div>
            <Button
              size="sm"
              variant="outline"
              onClick={() =>
                void loadPage(true, null, sortOrder, appliedQuery, gameMode)
              }
            >
              {t("results.refresh")}
            </Button>
          </div>
        </div>

        {storageStats && (
          <div
            className="grid grid-cols-3 gap-2 rounded-lg border border-white/10 bg-black/20 p-3"
            data-testid="library-storage-stats"
            aria-label={t("results.clips.storageStats")}
          >
            <div>
              <p className="text-xs text-muted-foreground">
                {t("results.clips.totalGames")}
              </p>
              <p className="text-lg font-semibold text-gaming-cyan">
                {storageStats.total_games}
              </p>
            </div>
            <div>
              <p className="text-xs text-muted-foreground">
                {t("results.clips.totalClips")}
              </p>
              <p className="text-lg font-semibold text-gaming-cyan">
                {storageStats.total_clips}
              </p>
            </div>
            <div>
              <p className="text-xs text-muted-foreground">
                {t("results.clips.storageUsed")}
              </p>
              <p className="text-lg font-semibold text-gaming-cyan">
                {formatStorage(
                  storageStats.total_disk_usage_bytes ??
                    storageStats.total_size_bytes,
                )}
              </p>
            </div>
          </div>
        )}

        <div className="flex flex-col gap-2 rounded-lg border border-white/10 bg-black/20 p-3 sm:flex-row">
          <div className="relative min-w-0 flex-1">
            <Search
              className="absolute left-3 top-3 h-4 w-4 text-muted-foreground"
              aria-hidden="true"
            />
            <Input
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder={t("results.clips.searchPlaceholder")}
              aria-label={t("results.clips.searchLabel")}
              data-testid="clip-vault-search"
              className="pl-9"
            />
          </div>
          <div className="flex items-center gap-2 sm:w-56">
            <Filter
              className="h-4 w-4 shrink-0 text-muted-foreground"
              aria-hidden="true"
            />
            <Select value={gameMode} onValueChange={setGameMode}>
              <SelectTrigger
                className="w-full"
                aria-label={t("results.clips.modeFilterLabel")}
              >
                <SelectValue placeholder={t("results.clips.allModes")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">
                  {t("results.clips.allModes")}
                </SelectItem>
                {availableGameModes.map((mode) => (
                  <SelectItem key={mode} value={mode}>
                    {mode}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-sm text-muted-foreground">
            {t("results.clips.gameCount", { count: groups.length })}
          </p>
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="ghost"
              onClick={expandAllGames}
              data-testid="clip-vault-expand-all"
            >
              {t("results.clips.expandAll")}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={collapseAllGames}
              data-testid="clip-vault-collapse-all"
            >
              {t("results.clips.collapseAll")}
            </Button>
          </div>
        </div>
      </div>

      {skippedItems > 0 && (
        <p
          className="mb-4 flex items-center gap-2 text-sm text-amber-400"
          role="status"
          data-testid="clip-vault-skipped-warning"
        >
          <AlertTriangle className="h-4 w-4" aria-hidden="true" />
          {t("results.clips.skippedItems", { count: skippedItems })}
        </p>
      )}

      <div className="overflow-hidden rounded-xl border border-white/10 bg-black/10">
        <div className="flex min-h-[32rem] flex-col md:flex-row">
          <aside
            className={`${eventListOpen ? "block w-full md:w-80" : "hidden w-0 overflow-hidden border-r-0 md:block"} max-h-80 shrink-0 border-b border-white/10 bg-gaming-sidebar/40 transition-[width] duration-200 md:max-h-none md:border-b-0 md:border-r`}
            aria-label={t("results.clips.title")}
          >
            <div className="h-full overflow-y-auto p-3">
              {groups.map((group) => {
                const selectedCount = group.clips.filter((clip) =>
                  selected.has(selectionKey(group.game_id, clip.file_path)),
                ).length;
                const allSelected =
                  group.clips.length > 0 &&
                  selectedCount === group.clips.length;
                const isExpanded = expandedGameIds.has(group.game_id);
                const gameName =
                  group.game?.champion ||
                  `${t("results.clips.game")} ${group.game_id}`;
                const gameDuration =
                  group.game?.start_time && group.game.end_time
                    ? Math.max(
                        0,
                        (new Date(group.game.end_time).getTime() -
                          new Date(group.game.start_time).getTime()) /
                          1000,
                      )
                    : null;
                return (
                  <section
                    key={group.game_id}
                    data-testid={`clip-vault-game-${group.game_id}`}
                    className="mb-3 last:mb-0"
                  >
                    <div className="overflow-hidden rounded-lg border border-white/10 bg-black/20">
                      <div className="flex items-stretch gap-1 p-1">
                        <button
                          type="button"
                          id={`clip-vault-trigger-${group.game_id}`}
                          className="flex min-h-[64px] min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-white/5 focus-visible:outline focus-visible:outline-2 focus-visible:outline-gaming-cyan"
                          onClick={() => toggleGameExpanded(group.game_id)}
                          aria-expanded={isExpanded}
                          aria-controls={`clip-vault-content-${group.game_id}`}
                          data-testid={`clip-vault-disclosure-${group.game_id}`}
                        >
                          {isExpanded ? (
                            <ChevronDown
                              className="h-4 w-4 shrink-0 text-gaming-cyan"
                              aria-hidden="true"
                            />
                          ) : (
                            <ChevronRight
                              className="h-4 w-4 shrink-0 text-muted-foreground"
                              aria-hidden="true"
                            />
                          )}
                          <div className="min-w-0 flex-1">
                            <h3 className="truncate text-sm font-semibold">
                              {gameName}
                              {group.game?.result && (
                                <span className="ml-2 text-muted-foreground">
                                  {t(
                                    `game.result.${group.game.result.toLowerCase()}`,
                                  )}
                                </span>
                              )}
                            </h3>
                            <p className="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
                              <span className="inline-flex items-center gap-1">
                                <Calendar
                                  className="h-3 w-3"
                                  aria-hidden="true"
                                />
                                {group.game?.start_time
                                  ? new Intl.DateTimeFormat(i18n.language, {
                                      dateStyle: "medium",
                                      timeStyle: "short",
                                    }).format(new Date(group.game.start_time))
                                  : t("results.clips.unknownDate")}
                              </span>
                              {group.game?.game_mode && (
                                <span>{group.game.game_mode}</span>
                              )}
                              {gameDuration !== null && (
                                <span className="inline-flex items-center gap-1">
                                  <Clock
                                    className="h-3 w-3"
                                    aria-hidden="true"
                                  />
                                  {formatDuration(gameDuration)}
                                </span>
                              )}
                              {group.game?.kda && (
                                <span>
                                  {group.game.kda.kills}/{group.game.kda.deaths}
                                  /{group.game.kda.assists}
                                </span>
                              )}
                            </p>
                          </div>
                          <span className="shrink-0 text-right text-xs text-muted-foreground">
                            <span className="block">
                              {t("results.clips.clipCount", {
                                count: group.clip_count,
                              })}
                            </span>
                            {selectedCount > 0 && (
                              <span className="block text-gaming-cyan">
                                {t("results.clips.selectedCount", {
                                  count: selectedCount,
                                })}
                              </span>
                            )}
                          </span>
                        </button>
                        <div className="flex shrink-0 items-center gap-0.5">
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => toggleGroup(group)}
                            aria-pressed={allSelected}
                            aria-label={
                              allSelected
                                ? t("results.clips.clearGameSelection")
                                : t("results.clips.selectGame")
                            }
                            title={
                              allSelected
                                ? t("results.clips.clearGameSelection")
                                : t("results.clips.selectGame")
                            }
                          >
                            {allSelected
                              ? t("results.clips.clearGameSelection")
                              : t("results.clips.selectGame")}
                          </Button>
                          <Button
                            size="icon"
                            variant="ghost"
                            onClick={() => handlePolish(group.game_id)}
                            aria-label={`${t("results.polish")}: ${gameName}`}
                            title={t("results.polish")}
                          >
                            <Scissors className="h-4 w-4" aria-hidden="true" />
                          </Button>
                          <Button
                            size="icon"
                            variant="ghost"
                            onClick={() => handleCreateHighlight(group.game_id)}
                            aria-label={`${t("results.makeHighlight")}: ${gameName}`}
                            title={t("results.makeHighlight")}
                          >
                            <Sparkles className="h-4 w-4" aria-hidden="true" />
                          </Button>
                          <Button
                            size="icon"
                            variant="ghost"
                            className="text-muted-foreground hover:text-destructive"
                            onClick={() => void handleDeleteGame(group.game_id)}
                            aria-label={`${t("common.delete")}: ${gameName}`}
                            title={t("common.delete")}
                          >
                            <Trash2 className="h-4 w-4" aria-hidden="true" />
                          </Button>
                        </div>
                      </div>
                    </div>
                    {isExpanded && (
                      <div
                        id={`clip-vault-content-${group.game_id}`}
                        role="region"
                        aria-labelledby={`clip-vault-trigger-${group.game_id}`}
                        className="space-y-1 border-x border-b border-white/10 bg-black/10 p-2"
                        data-testid={`clip-vault-grid-${group.game_id}`}
                      >
                        {group.clips.map((clip, index) => {
                          const { title, reasons } = clipLabel(clip);
                          const label = t(title.key, title.params);
                          const key = selectionKey(
                            group.game_id,
                            clip.file_path,
                          );
                          const isActive =
                            activeClip?.path === clip.file_path &&
                            activeClip.gameId === group.game_id;
                          return (
                            <article
                              key={clip.file_path}
                              data-testid={`clip-vault-card-${clip.file_path}`}
                              className={`group flex overflow-hidden rounded-lg border ${isActive ? "border-gaming-cyan/70 bg-gaming-cyan/10" : "border-transparent hover:border-white/10 hover:bg-white/5"}`}
                            >
                              <button
                                type="button"
                                className="relative h-16 w-28 shrink-0 bg-black text-left"
                                onClick={() =>
                                  setActiveClip({
                                    gameId: group.game_id,
                                    path: clip.file_path,
                                    duration: clip.duration,
                                    clip,
                                  })
                                }
                                aria-label={`${t("results.clips.play")}: ${label}`}
                              >
                                <VaultThumbnail
                                  gameId={group.game_id}
                                  clip={clip}
                                  onGenerated={(path) =>
                                    updateThumbnail(
                                      group.game_id,
                                      clip.file_path,
                                      path,
                                    )
                                  }
                                />
                                <span className="absolute inset-0 flex items-center justify-center bg-black/20">
                                  <Play
                                    className="h-5 w-5 text-white"
                                    aria-hidden="true"
                                  />
                                </span>
                              </button>
                              <div className="min-w-0 flex-1 p-2">
                                <div className="flex items-start gap-1">
                                  <button
                                    type="button"
                                    className="min-w-0 flex-1 text-left"
                                    onClick={() =>
                                      setActiveClip({
                                        gameId: group.game_id,
                                        path: clip.file_path,
                                        duration: clip.duration,
                                        clip,
                                      })
                                    }
                                  >
                                    <p className="truncate text-sm font-medium">
                                      {label}
                                    </p>
                                    <p className="truncate text-xs text-muted-foreground">
                                      {reasons.length > 0
                                        ? reasons
                                            .map((reason) =>
                                              t(reason.key, reason.params),
                                            )
                                            .join(" · ")
                                        : t("home.clips.seconds", {
                                            count: clipSeconds(clip.duration),
                                          })}
                                    </p>
                                  </button>
                                  <input
                                    type="checkbox"
                                    checked={selected.has(key)}
                                    onChange={() =>
                                      toggleSelection(group.game_id, clip)
                                    }
                                    aria-label={`${t("results.clips.select")}: ${label}`}
                                    className="mt-1"
                                  />
                                </div>
                                {sortOrder === "best" && index < 3 && (
                                  <span className="text-[10px] font-bold text-gaming-cyan">
                                    {t("results.clips.gameRank", {
                                      rank: index + 1,
                                    })}
                                  </span>
                                )}
                              </div>
                            </article>
                          );
                        })}
                      </div>
                    )}
                  </section>
                );
              })}
            </div>
          </aside>
          <div className="min-w-0 flex-1 p-3 md:p-5">
            <div className="mb-3 flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setEventListOpen((open) => !open)}
                aria-label={
                  eventListOpen ? t("common.close") : t("results.clips.title")
                }
                aria-expanded={eventListOpen}
                className="hidden md:inline-flex"
              >
                {eventListOpen ? (
                  <PanelLeftClose className="h-4 w-4" aria-hidden="true" />
                ) : (
                  <PanelLeftOpen className="h-4 w-4" aria-hidden="true" />
                )}
              </Button>
              <div className="min-w-0">
                <h3 className="truncate font-semibold">
                  {activeClip
                    ? t(
                        clipLabel(activeClip.clip).title.key,
                        clipLabel(activeClip.clip).title.params,
                      )
                    : t("results.clips.title")}
                </h3>
                {activeClip && (
                  <p className="text-sm text-muted-foreground">
                    {t("home.clips.seconds", {
                      count: clipSeconds(activeClip.duration),
                    })}
                  </p>
                )}
              </div>
            </div>
            {activeClip ? (
              <VideoPlayer
                src={convertFileSrc(activeClip.path)}
                title={t(
                  clipLabel(activeClip.clip).title.key,
                  clipLabel(activeClip.clip).title.params,
                )}
                className="h-[min(55vh,28rem)] min-h-72 w-full md:h-[min(68vh,48rem)]"
              />
            ) : (
              <div className="flex h-[min(55vh,28rem)] min-h-72 items-center justify-center rounded-lg bg-black text-sm text-muted-foreground md:h-[min(68vh,48rem)]">
                {t("results.clips.emptyDescription")}
              </div>
            )}
          </div>
        </div>
        <div className="border-t border-white/10 p-3 md:hidden">
          <p className="text-xs text-muted-foreground">
            {t("results.clips.clipCount", {
              count: groups.reduce(
                (total, group) => total + group.clips.length,
                0,
              ),
            })}
          </p>
        </div>
      </div>

      {nextCursor && (
        <div className="mt-8 text-center">
          <Button
            variant="outline"
            disabled={loadingMore}
            onClick={() =>
              void loadPage(
                false,
                nextCursor,
                sortOrder,
                appliedQuery,
                gameMode,
              )
            }
          >
            {loadingMore
              ? t("results.clips.loadingMore")
              : t("results.clips.loadMore")}
          </Button>
        </div>
      )}

      {selected.size > 0 && (
        <div
          className="fixed bottom-4 left-4 right-4 z-40 mx-auto max-w-5xl rounded-xl border border-primary/30 bg-background/95 p-4 shadow-2xl backdrop-blur"
          data-testid="clip-vault-action-bar"
          role="region"
          aria-label={t("results.clips.selectionActions")}
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p className="font-medium">
                {t("results.clips.selectionSummary", {
                  clips: selected.size,
                  games: selectedGroups.length,
                  duration: formatDuration(totalDuration),
                })}
              </p>
              {totalDuration > targetDuration && (
                <p className="mt-1 text-sm text-amber-400" role="alert">
                  {t("results.clips.overTargetWarning")}
                </p>
              )}
            </div>
            <div className="flex gap-2">
              <Button variant="ghost" onClick={() => setSelected(new Map())}>
                {t("results.clips.clearAll")}
              </Button>
              <Button onClick={makeMontage} data-testid="create-montage-button">
                <Sparkles className="mr-2 h-4 w-4" aria-hidden="true" />
                {t("results.clips.createMontage")}
              </Button>
            </div>
          </div>
        </div>
      )}
      <ConfirmDialog />
    </section>
  );
}
