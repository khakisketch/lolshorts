import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { lcuApi, normalizeReplayStatus } from "@/api/lcu";
import type { MatchInfo, ReplayAvailability } from "@/api/lcu";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import { EmptyState } from "@/components/ui/empty-state";
import { useToast } from "@/components/ui/use-toast";
import {
  Loader2,
  Play,
  Download,
  RefreshCw,
  Search,
  Filter,
} from "lucide-react";
import { ReplayTargetModal } from "@/components/overlay/ReplayTargetModal";
import { cmd } from "@/api/client";
import { pageStyles } from "@/lib/utils";
import { logger } from "@/lib/logger";

export function Replays() {
  const { t } = useTranslation();
  const [matches, setMatches] = useState<MatchInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [downloadingId, setDownloadingId] = useState<number | null>(null);
  const [replayStatuses, setReplayStatuses] = useState<
    Record<number, ReplayAvailability>
  >({});
  const [isReplayModalOpen, setIsReplayModalOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [filterWin, setFilterWin] = useState<string>("all");
  const { toast } = useToast();

  const refreshReplayStatuses = useCallback(async (gameIds: number[]) => {
    if (gameIds.length === 0) return;
    const results = await Promise.allSettled(
      gameIds.map(async (gameId) => ({
        gameId,
        status: normalizeReplayStatus(await lcuApi.getReplayStatus(gameId)),
      })),
    );
    setReplayStatuses((current) => {
      const next = { ...current };
      for (const [index, result] of results.entries()) {
        const gameId = gameIds[index];
        next[gameId] =
          result.status === "fulfilled" ? result.value.status : "unknown";
      }
      return next;
    });
  }, []);

  const loadMatches = useCallback(async () => {
    setLoading(true);
    try {
      const isConnected = await lcuApi.checkStatus();
      if (!isConnected) {
        // A disconnected client is the normal state when the user is not
        // playing — the empty state below says so in plain words, so there is
        // no error toast to dismiss every time this tab is opened.
        setMatches([]);
        setReplayStatuses({});
        return;
      }

      const history = await lcuApi.listMatchHistory(0, 20);
      setMatches(history);
      setReplayStatuses(
        Object.fromEntries(history.map((match) => [match.game_id, "checking"])),
      );
      await refreshReplayStatuses(history.map((match) => match.game_id));
    } catch (error) {
      logger.error("Failed to load match history:", error);
      toast({
        title: t("replays.toast.loadError"),
        description: t("replays.toast.loadErrorDesc"),
        variant: "destructive",
      });
    } finally {
      setLoading(false);
    }
  }, [refreshReplayStatuses, t, toast]);

  const filteredMatches = matches.filter((match) => {
    const searchLower = searchQuery.toLowerCase();
    const matchesSearch =
      match.game_mode.toLowerCase().includes(searchLower) ||
      match.game_id.toString().includes(searchLower);

    if (!matchesSearch) return false;

    if (filterWin !== "all") {
      const winStatus = filterWin === "win";
      if (match.win !== winStatus) return false;
    }

    return true;
  });

  useEffect(() => {
    loadMatches();
  }, [loadMatches]);

  useEffect(() => {
    const downloading = Object.entries(replayStatuses)
      .filter(([, status]) => status === "downloading")
      .map(([gameId]) => Number(gameId));
    if (downloading.length === 0) return;
    const timer = window.setInterval(() => {
      void refreshReplayStatuses(downloading);
    }, 2000);
    return () => window.clearInterval(timer);
  }, [refreshReplayStatuses, replayStatuses]);

  const handleDownload = async (gameId: number) => {
    setDownloadingId(gameId);
    try {
      await lcuApi.downloadReplay(gameId);
      setReplayStatuses((current) => ({
        ...current,
        [gameId]: "downloading",
      }));
      toast({
        title: t("replays.toast.downloadStarted"),
        description: t("replays.toast.downloadStartedDesc"),
      });
    } catch (error) {
      logger.error("Failed to download replay:", error);
      toast({
        title: t("replays.toast.downloadFailed"),
        description: t("replays.toast.downloadFailedDesc"),
        variant: "destructive",
      });
    } finally {
      setDownloadingId(null);
    }
  };

  const handleLaunch = async (gameId: number) => {
    try {
      await lcuApi.launchReplay(gameId);
      await cmd<void>("notify_replay_launched", {});

      toast({
        title: t("replays.toast.launchingReplay"),
        description: t("replays.toast.launchingReplayDesc"),
      });

      setIsReplayModalOpen(true);
    } catch (error) {
      logger.error("Failed to launch replay:", error);
      toast({
        title: t("replays.toast.launchFailed"),
        description: t("replays.toast.launchFailedDesc"),
        variant: "destructive",
      });
    }
  };

  return (
    <div data-testid="replays-page" className={pageStyles.container}>
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2">
        <h2 className="text-xl font-bold">{t("replays.title")}</h2>
        <Button variant="outline" onClick={loadMatches} disabled={loading}>
          <RefreshCw
            className={`mr-2 h-4 w-4 ${loading ? "animate-spin" : ""}`}
          />
          {t("replays.refresh")}
        </Button>
      </div>

      {/* Filters — only worth showing once there is something to filter. */}
      {matches.length > 0 && (
        <div className="bg-black/40 rounded-lg border border-white/5 p-4">
          <div className="flex flex-col sm:flex-row gap-3">
            <div className="flex-1 relative">
              <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder={t("replays.searchPlaceholder")}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-9"
              />
            </div>
            <div className="flex items-center gap-2">
              <Filter className="h-4 w-4 text-muted-foreground shrink-0" />
              <Select value={filterWin} onValueChange={setFilterWin}>
                <SelectTrigger className="w-full sm:w-[150px]">
                  <SelectValue
                    placeholder={t("replays.filterWinPlaceholder")}
                  />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">
                    {t("replays.filter.allResults")}
                  </SelectItem>
                  <SelectItem value="win">{t("replays.victory")}</SelectItem>
                  <SelectItem value="loss">{t("replays.defeat")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
        </div>
      )}

      {matches.length === 0 && !loading && (
        <EmptyState
          title={t("replays.noMatches")}
          description={
            <span style={{ wordBreak: "keep-all" }}>
              {t("replays.noMatchesDesc")}
            </span>
          }
          action={{
            label: t("replays.emptyState.action"),
            onClick: loadMatches,
          }}
          className="py-20"
        />
      )}

      {matches.length > 0 && filteredMatches.length === 0 && (
        <EmptyState
          icon={Search}
          title={t("replays.noMatchesMatchFilters")}
          description={t("replays.tryDifferentFilters")}
          action={{
            label: t("replays.clearFilters"),
            onClick: () => {
              setSearchQuery("");
              setFilterWin("all");
            },
          }}
          className="py-20"
        />
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {filteredMatches.map((match) => (
          <div key={match.game_id} className="gaming-panel p-0 overflow-hidden">
            <div className="p-4 border-b border-white/5">
              <div className="flex justify-between items-center">
                <div className="flex items-center gap-2">
                  <span
                    className={`text-base font-bold ${match.win ? "text-gaming-cyan" : "text-gaming-magenta"}`}
                  >
                    {match.win ? t("replays.victory") : t("replays.defeat")}
                  </span>
                  <Badge
                    variant="outline"
                    className="text-[10px] tracking-wider opacity-70"
                  >
                    {t("replays.badge.match")}
                  </Badge>
                  <Badge variant="secondary" className="text-[10px]">
                    {t(
                      `replays.status.${replayStatuses[match.game_id] ?? "checking"}`,
                    )}
                  </Badge>
                </div>
                <span className="text-sm text-muted-foreground">
                  {new Date(match.game_creation).toLocaleDateString()}
                </span>
              </div>
            </div>
            <div className="p-4 space-y-2 bg-black/20">
              <div className="flex justify-between text-sm">
                <span className="text-muted-foreground">
                  {t("replays.champion")}:
                </span>
                <span className="font-medium">
                  {t("replays.championNumber", { id: match.champion_id })}
                </span>
              </div>
              <div className="flex justify-between text-sm">
                <span className="text-muted-foreground">
                  {t("replays.mode")}:
                </span>
                <span className="font-medium">{match.game_mode}</span>
              </div>
              <div className="flex justify-between text-sm">
                <span className="text-muted-foreground">
                  {t("replays.kda")}:
                </span>
                <span className="font-medium text-gaming-cyan">
                  {match.kills} / {match.deaths} / {match.assists}
                </span>
              </div>
              <div className="flex justify-between text-sm">
                <span className="text-muted-foreground">
                  {t("replays.duration")}:
                </span>
                <span>
                  {Math.floor(match.game_duration / 60)}m{" "}
                  {match.game_duration % 60}s
                </span>
              </div>

              <div className="flex gap-2 pt-2">
                <Button
                  className="flex-1"
                  variant="secondary"
                  onClick={() => handleDownload(match.game_id)}
                  disabled={
                    downloadingId === match.game_id ||
                    replayStatuses[match.game_id] === "checking" ||
                    replayStatuses[match.game_id] === "downloading" ||
                    replayStatuses[match.game_id] === "ready"
                  }
                >
                  {downloadingId === match.game_id ||
                  replayStatuses[match.game_id] === "downloading" ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Download className="mr-2 h-4 w-4" />
                  )}
                  {replayStatuses[match.game_id] === "ready"
                    ? t("replays.downloaded")
                    : t("replays.download")}
                </Button>
                <Button
                  className="flex-1"
                  onClick={() => handleLaunch(match.game_id)}
                  disabled={
                    replayStatuses[match.game_id] === "checking" ||
                    replayStatuses[match.game_id] === "notDownloaded" ||
                    replayStatuses[match.game_id] === "downloading"
                  }
                >
                  <Play className="mr-2 h-4 w-4" />
                  {t("replays.watch")}
                </Button>
              </div>
            </div>
          </div>
        ))}
      </div>

      <ReplayTargetModal
        isOpen={isReplayModalOpen}
        onClose={() => setIsReplayModalOpen(false)}
      />
    </div>
  );
}
