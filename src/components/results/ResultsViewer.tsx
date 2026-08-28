import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "@tanstack/react-router";
import { useAutoEditResults } from "@/hooks/useAutoEditResults";
import { convertFileSrc } from "@tauri-apps/api/core";
import { formatDuration, formatStorage } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { useConfirmDialog } from "@/components/ui/confirm-dialog";
import { Spinner, SpinnerCenter } from "@/components/ui/spinner";
import { EmptyState } from "@/components/ui/empty-state";
import {
  Clock,
  Trash2,
  Play,
  Upload,
  CheckCircle2,
  XCircle,
  Loader2,
  AlertCircle,
  Film,
  Calendar,
  Sparkles,
  Search,
  Filter,
  Folder,
  Scissors,
  Share2,
} from "lucide-react";
import { AutoEditResultGroup, AutoEditResultMetadata } from "@/types/autoEdit";
import { logger } from "@/lib/logger";
import { utilsApi } from "@/api/utils";
import { useEditorStore } from "@/stores/editorStore";
import { ShareDialog } from "./ShareDialog";
import { storageApi } from "@/api/storage";
import { videoApi } from "@/api/video";

export function ResultsViewer() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [results, setResults] = useState<AutoEditResultMetadata[]>([]);
  const [groups, setGroups] = useState<AutoEditResultGroup[]>([]);
  const [activePlayer, setActivePlayer] =
    useState<AutoEditResultMetadata | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [filterStatus, setFilterStatus] = useState<string>("all");
  const [shareTarget, setShareTarget] = useState<AutoEditResultMetadata | null>(
    null,
  );
  const { deleteResult, isLoading, error } = useAutoEditResults();
  const { confirm, ConfirmDialog } = useConfirmDialog();
  const setSelectedGameId = useEditorStore((state) => state.setSelectedGameId);

  const loadResults = useCallback(async () => {
    try {
      const fetchedGroups = await storageApi.getAutoEditResultGroups();
      const safeGroups = Array.isArray(fetchedGroups) ? fetchedGroups : [];
      const fetchedResults = safeGroups.flatMap((group) => group.outputs);
      // Never trust the IPC return shape. A command that fails, is missing, or
      // returns `None` hands back null/undefined, and assigning that straight to
      // state made the whole screen throw `null.filter` on first paint — an empty
      // library is the normal cold-start state, so that crash was the default
      // experience for a new install.
      setResults(Array.isArray(fetchedResults) ? fetchedResults : []);
      setGroups(safeGroups);
    } catch (err) {
      logger.error("Failed to load results:", err);
    }
  }, []);

  const filteredResults = (results ?? []).filter((result) => {
    const searchLower = searchQuery.toLowerCase();
    const matchesSearch =
      result.result_id.toLowerCase().includes(searchLower) ||
      result.job_id.toLowerCase().includes(searchLower);

    if (!matchesSearch) return false;

    if (filterStatus !== "all") {
      const status = result.youtube_status?.status || "NotUploaded";
      if (status !== filterStatus) return false;
    }

    return true;
  });
  const filteredResultIds = new Set(
    filteredResults.map((result) => result.result_id),
  );
  const filteredGroups = groups.filter((group) =>
    group.outputs.some((result) => filteredResultIds.has(result.result_id)),
  );

  // Load results on mount
  useEffect(() => {
    loadResults();
  }, [loadResults]);

  const handleDelete = async (resultId: string) => {
    const confirmed = await confirm({
      title: t("results.deleteConfirmTitle"),
      description: t("results.deleteConfirmDescription"),
      confirmText: t("common.delete"),
      cancelText: t("common.cancel"),
      variant: "danger",
    });

    if (!confirmed) {
      return;
    }

    try {
      await deleteResult(resultId, true);
      setResults(results.filter((r) => r.result_id !== resultId));
    } catch (err) {
      logger.error("Failed to delete result:", err);
    }
  };

  const handleDeleteGroup = async (seriesId: string) => {
    const confirmed = await confirm({
      title: t("resultSeries.deleteGroup"),
      description: t("results.deleteConfirmDescription"),
      confirmText: t("common.delete"),
      cancelText: t("common.cancel"),
      variant: "danger",
    });
    if (!confirmed) return;
    await storageApi.deleteAutoEditResultGroup(seriesId, true);
    setGroups((items) => items.filter((group) => group.series_id !== seriesId));
    setResults((items) =>
      items.filter(
        (result) => (result.series_id || result.result_id) !== seriesId,
      ),
    );
  };

  const handlePlay = (result: AutoEditResultMetadata) => {
    setActivePlayer(result);
  };

  const handleOpenFile = async (outputPath: string) => {
    try {
      await utilsApi.openFileWithDefaultApp(outputPath);
    } catch (err) {
      logger.error("Failed to open file:", err);
    }
  };

  const handleShowInFolder = async (outputPath: string) => {
    try {
      await utilsApi.showInFolder(outputPath);
    } catch (err) {
      logger.error("Failed to show in folder:", err);
    }
  };

  /** "공유" — the YouTube upload UI opens on top of the library, not as a screen. */
  const handleShare = async (result: AutoEditResultMetadata) => {
    let report = result.validation;
    if (!report || report.status === "unknown") {
      report = await videoApi.revalidateAutoEditResult(result.result_id);
      setResults((items) =>
        items.map((item) =>
          item.result_id === result.result_id
            ? { ...item, validation: report }
            : item,
        ),
      );
    }
    if (report.status === "invalid" || report.status === "unknown") return;
    if (
      report.status === "warning" &&
      !window.confirm(t("platformExport.confirmWarning"))
    )
      return;
    setShareTarget({ ...result, validation: report });
  };

  /**
   * "다듬기" — hand the finished short back to the editor. The editor works on a
   * game's clips, so we preselect the game this short was built from before
   * navigating; without it the editor would open on an empty selection.
   */
  const handlePolish = (result: AutoEditResultMetadata) => {
    const gameId = result.game_ids?.[0];
    if (gameId) {
      setSelectedGameId(gameId);
      navigate({ to: "/editor", search: { gameId } });
      return;
    }
    navigate({ to: "/editor" });
  };

  const formatDate = (dateString: string): string => {
    return new Date(dateString).toLocaleDateString();
  };

  const getUploadStatusBadge = (result: AutoEditResultMetadata) => {
    if (!result.youtube_status) {
      return <Badge variant="secondary">{t("results.notUploaded")}</Badge>;
    }

    const { status, progress, error } = result.youtube_status;

    switch (status) {
      case "NotUploaded":
        return <Badge variant="secondary">{t("results.notUploaded")}</Badge>;
      case "Queued":
        return (
          <Badge variant="outline">
            <Loader2 className="w-3 h-3 mr-1 animate-spin" />
            {t("results.queued")}
          </Badge>
        );
      case "Uploading":
        return (
          <Badge variant="outline">
            <Upload className="w-3 h-3 mr-1" />
            {t("results.uploading")} {progress}%
          </Badge>
        );
      case "Processing":
        return (
          <Badge variant="outline">
            <Loader2 className="w-3 h-3 mr-1 animate-spin" />
            {t("results.processing")}
          </Badge>
        );
      case "Completed":
        return (
          <Badge variant="default">
            <CheckCircle2 className="w-3 h-3 mr-1" />
            {t("results.completed")}
          </Badge>
        );
      case "Failed":
        return (
          <Badge
            variant="destructive"
            title={error || t("results.uploadError")}
          >
            <XCircle className="w-3 h-3 mr-1" />
            {t("results.failed")}
          </Badge>
        );
      default:
        return <Badge variant="secondary">{t("results.unknown")}</Badge>;
    }
  };

  return (
    <div className="flex flex-col space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2">
        <div>
          <h2 className="text-xl font-bold">{t("results.highlights.title")}</h2>
          <p
            className="text-sm text-muted-foreground"
            style={{ wordBreak: "keep-all" }}
          >
            {t("results.highlights.description")}
          </p>
        </div>
        <Button onClick={loadResults} disabled={isLoading}>
          {isLoading ? (
            <Spinner size="sm" className="mr-2" />
          ) : (
            <Film className="w-4 h-4 mr-2" />
          )}
          {t("results.refresh")}
        </Button>
      </div>

      {/* Error Alert */}
      {error && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {/* Filters — only worth showing once there is something to filter. */}
      {results.length > 0 && (
        <div className="bg-black/40 rounded-lg border border-white/5 p-4">
          <div className="flex flex-col sm:flex-row gap-3">
            <div className="flex-1 relative">
              <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder={t("results.searchPlaceholder")}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-9"
              />
            </div>
            <div className="flex items-center gap-2">
              <Filter className="h-4 w-4 text-muted-foreground shrink-0" />
              <Select value={filterStatus} onValueChange={setFilterStatus}>
                <SelectTrigger className="w-full sm:w-[180px]">
                  <SelectValue
                    placeholder={t("results.filterStatusPlaceholder")}
                  />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">
                    {t("results.filter.allStatuses")}
                  </SelectItem>
                  <SelectItem value="Completed">
                    {t("results.completed")}
                  </SelectItem>
                  <SelectItem value="Queued">{t("results.queued")}</SelectItem>
                  <SelectItem value="Uploading">
                    {t("results.uploading")}
                  </SelectItem>
                  <SelectItem value="Processing">
                    {t("results.processing")}
                  </SelectItem>
                  <SelectItem value="Failed">{t("results.failed")}</SelectItem>
                  <SelectItem value="NotUploaded">
                    {t("results.notUploaded")}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
        </div>
      )}

      {/* Loading State */}
      {isLoading && results.length === 0 && (
        <SpinnerCenter size="lg" label={t("results.loading")} />
      )}

      {/* Empty State — nothing recorded yet, so point back at the one thing the
          user actually has to do: play a game. */}
      {!isLoading && results.length === 0 && (
        <div className="gaming-panel p-6" data-testid="results-empty">
          <div>
            <EmptyState
              icon={Sparkles}
              title={t("results.empty.noVideosTitle")}
              description={
                <span style={{ wordBreak: "keep-all" }}>
                  {t("results.empty.noVideosDescription")}
                </span>
              }
              action={{
                label: t("results.empty.goHome"),
                onClick: () => navigate({ to: "/" }),
              }}
              size="lg"
            />
          </div>
        </div>
      )}

      {!isLoading && results.length > 0 && filteredResults.length === 0 && (
        <div className="gaming-panel p-6">
          <EmptyState
            icon={Search}
            title={t("results.noResultsMatchFilters")}
            description={t("results.tryDifferentFilters")}
            action={{
              label: t("results.clearFilters"),
              onClick: () => {
                setSearchQuery("");
                setFilterStatus("all");
              },
            }}
            size="lg"
          />
        </div>
      )}

      {activePlayer && (
        <div
          className="gaming-panel mx-auto w-full max-w-3xl space-y-3 p-4"
          data-testid="active-result-player"
        >
          {/* Generated gameplay has no authored caption track. */}
          {/* eslint-disable-next-line jsx-a11y/media-has-caption */}
          <video
            key={activePlayer.result_id}
            controls
            preload="metadata"
            className="max-h-[60vh] w-full bg-black object-contain"
            src={convertFileSrc(activePlayer.output_path)}
          />
          <div className="flex justify-between gap-2 text-sm">
            <span className="truncate">{activePlayer.output_path}</span>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setActivePlayer(null)}
            >
              {t("common.close")}
            </Button>
          </div>
        </div>
      )}

      {results.length > 0 && (
        <div className="space-y-4" data-testid="result-groups">
          {filteredGroups.map((group) => (
            <section
              key={group.series_id}
              className="overflow-hidden rounded-lg border border-white/10 bg-black/40"
            >
              <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/10 p-4">
                <div>
                  <h3 className="flex items-center gap-2 text-lg font-semibold">
                    <Film className="h-5 w-5" />
                    {t("resultSeries.title")}
                  </h3>
                  <p className="text-sm text-muted-foreground">
                    {t("resultSeries.parts", { count: group.outputs.length })} ·{" "}
                    {formatDuration(group.total_duration)} ·{" "}
                    {formatStorage(group.total_file_size_bytes)}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <Badge
                    variant={
                      group.validation_status === "invalid"
                        ? "destructive"
                        : "outline"
                    }
                  >
                    {t(`outputValidation.${group.validation_status}`)}
                  </Badge>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => void handleDeleteGroup(group.series_id)}
                    aria-label={t("resultSeries.deleteGroup")}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>
              <div className="grid gap-3 p-4 md:grid-cols-2 lg:grid-cols-3">
                {group.outputs.map((result) => (
                  <article
                    key={result.result_id}
                    className="overflow-hidden rounded-md border border-white/10 bg-background/30"
                  >
                    <button
                      type="button"
                      className="aspect-video bg-black relative block w-full overflow-hidden"
                      onClick={() => handlePlay(result)}
                    >
                      {result.thumbnail_path ? (
                        <img
                          src={convertFileSrc(result.thumbnail_path)}
                          alt={t("results.thumbnailAlt")}
                          className="absolute inset-0 h-full w-full object-contain"
                        />
                      ) : (
                        <span className="absolute inset-0 flex items-center justify-center">
                          <Film className="h-10 w-10 text-muted-foreground" />
                        </span>
                      )}
                      <span className="absolute right-2 top-2">
                        {getUploadStatusBadge(result)}
                      </span>
                    </button>
                    <div className="space-y-3 p-3">
                      <div className="flex items-center justify-between gap-2">
                        <strong>
                          {t("resultSeries.part", {
                            current: result.part_index ?? 1,
                            total: result.part_count ?? 1,
                          })}
                        </strong>
                        <Badge
                          variant={
                            result.validation?.status === "invalid"
                              ? "destructive"
                              : "secondary"
                          }
                        >
                          {t(
                            `outputValidation.${result.validation?.status ?? "unknown"}`,
                          )}
                        </Badge>
                      </div>
                      <p className="text-xs text-muted-foreground">
                        <Calendar className="mr-1 inline h-3 w-3" />
                        {formatDate(result.created_at)} ·{" "}
                        <Clock className="ml-1 mr-1 inline h-3 w-3" />
                        {formatDuration(result.duration)}
                      </p>
                      <div className="grid grid-cols-3 gap-1">
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => handlePlay(result)}
                          data-testid={`result-play-${result.result_id}`}
                        >
                          <Play className="h-3.5 w-3.5" />
                          <span className="sr-only">{t("results.play")}</span>
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => handlePolish(result)}
                          data-testid={`result-polish-${result.result_id}`}
                        >
                          <Scissors className="h-3.5 w-3.5" />
                          <span className="sr-only">{t("results.polish")}</span>
                        </Button>
                        <Button
                          size="sm"
                          disabled={result.validation?.status === "invalid"}
                          onClick={() => void handleShare(result)}
                          data-testid={`result-share-${result.result_id}`}
                        >
                          <Share2 className="h-3.5 w-3.5" />
                          <span className="sr-only">{t("results.share")}</span>
                        </Button>
                      </div>
                      <div className="flex gap-1">
                        <Button
                          size="sm"
                          variant="ghost"
                          className="flex-1"
                          onClick={() =>
                            void handleOpenFile(result.output_path)
                          }
                        >
                          <Film className="mr-1 h-3 w-3" />
                          {t("results.openFile")}
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() =>
                            void handleShowInFolder(result.output_path)
                          }
                        >
                          <Folder className="h-3 w-3" />
                          <span className="sr-only">
                            {t("results.showInFolder")}
                          </span>
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => void handleDelete(result.result_id)}
                          aria-label={t("resultSeries.deletePart", {
                            part: result.part_index ?? 1,
                          })}
                        >
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      </div>
                    </div>
                  </article>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}

      <ConfirmDialog />

      <ShareDialog
        open={shareTarget !== null}
        onOpenChange={(open) => {
          if (!open) setShareTarget(null);
        }}
        videoPath={shareTarget?.output_path}
        resultId={shareTarget?.result_id}
      />
    </div>
  );
}
