import { useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  ArrowDown,
  ArrowUp,
  GripVertical,
  Redo2,
  RotateCcw,
  Trash2,
  Undo2,
} from "lucide-react";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { AutoEditOutputIntent, AutoEditPlanClip } from "@/types/autoEdit";
import { formatDuration } from "@/lib/utils";

interface AutoEditStoryboardProps {
  clips: AutoEditPlanClip[];
  outputIntent: AutoEditOutputIntent;
  onOutputIntentChange: (intent: AutoEditOutputIntent) => void;
  onMove: (from: number, to: number) => void;
  onTrim: (path: string, start: number, end: number) => void;
  onRemove: (path: string) => void;
  onResetRecommendation: () => void;
  onUndo: () => void;
  onRedo: () => void;
  canUndo: boolean;
  canRedo: boolean;
  onBack: () => void;
  onGenerate: () => void;
  isLoading?: boolean;
}

export function AutoEditStoryboard({
  clips,
  outputIntent,
  onOutputIntentChange,
  onMove,
  onTrim,
  onRemove,
  onResetRecommendation,
  onUndo,
  onRedo,
  canUndo,
  canRedo,
  onBack,
  onGenerate,
  isLoading = false,
}: AutoEditStoryboardProps) {
  const { t } = useTranslation();
  const [activePath, setActivePath] = useState<string | null>(
    clips[0]?.file_path ?? null,
  );
  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );
  const duration = useMemo(
    () =>
      clips.reduce(
        (sum, clip) => sum + clip.trim_end_secs - clip.trim_start_secs,
        0,
      ),
    [clips],
  );
  const partCount =
    outputIntent === "shorts_series"
      ? Math.max(1, Math.ceil(duration / 180))
      : 1;
  const invalidTrim = clips.some(
    (clip) =>
      !Number.isFinite(clip.trim_start_secs) ||
      !Number.isFinite(clip.trim_end_secs) ||
      clip.trim_start_secs < 0 ||
      clip.trim_end_secs <= clip.trim_start_secs ||
      clip.trim_end_secs > clip.source_duration_secs + 0.001,
  );
  const inlineError =
    clips.length === 0
      ? t("autoEdit.storyboard.empty", "Add at least one clip")
      : invalidTrim
        ? t("autoEdit.storyboard.invalidTrim", "Fix invalid trim ranges")
        : duration > 180 && outputIntent === "single_short"
          ? t(
              "autoEdit.storyboard.over180Title",
              "This selection is longer than one Short",
            )
          : null;
  const estimatedBytes = Math.ceil((duration * 12_000_000) / 8);
  const cumulativeEnds = clips.reduce<number[]>((values, clip) => {
    values.push(
      (values[values.length - 1] ?? 0) +
        clip.trim_end_secs -
        clip.trim_start_secs,
    );
    return values;
  }, []);

  return (
    <div
      className="mx-auto max-w-5xl space-y-5"
      data-testid="auto-edit-storyboard"
    >
      <div className="gaming-panel p-5 space-y-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="text-lg font-semibold">
              {t("autoEdit.storyboard.title", "Review your clips")}
            </h3>
            <p className="text-sm text-muted-foreground">
              {t("autoEdit.storyboard.summary", {
                defaultValue:
                  "{{clips}} clips · {{duration}} · {{parts}} output(s)",
                clips: clips.length,
                duration: formatDuration(duration),
                parts: partCount,
              })}
            </p>
          </div>
          <div className="flex gap-1">
            <Button
              type="button"
              size="icon"
              variant="outline"
              disabled={!canUndo}
              onClick={onUndo}
              aria-label={t("common.undo", "Undo")}
            >
              <Undo2 className="h-4 w-4" />
            </Button>
            <Button
              type="button"
              size="icon"
              variant="outline"
              disabled={!canRedo}
              onClick={onRedo}
              aria-label={t("common.redo", "Redo")}
            >
              <Redo2 className="h-4 w-4" />
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={onResetRecommendation}
            >
              <RotateCcw className="mr-2 h-4 w-4" />
              {t("autoEdit.storyboard.reset", "Reset recommendation")}
            </Button>
          </div>
        </div>

        {duration > 180 && (
          <div
            className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3"
            role="status"
          >
            <p className="font-medium">
              {t(
                "autoEdit.storyboard.over180Title",
                "This selection is longer than one Short",
              )}
            </p>
            <p className="text-sm text-muted-foreground">
              {t(
                "autoEdit.storyboard.over180Body",
                "Every selected scene will be kept. Choose a Shorts series or one general vertical video.",
              )}
            </p>
          </div>
        )}

        <fieldset className="grid gap-2 sm:grid-cols-3">
          <legend className="mb-2 text-sm font-medium">
            {t("autoEdit.storyboard.output", "Output")}
          </legend>
          {(
            [
              ["single_short", t("autoEdit.output.single", "Single Short")],
              ["shorts_series", t("autoEdit.output.series", "Shorts series")],
              [
                "vertical_video",
                t("autoEdit.output.vertical", "General vertical video"),
              ],
            ] as const
          ).map(([value, label]) => (
            <Button
              key={value}
              type="button"
              variant={outputIntent === value ? "default" : "outline"}
              aria-pressed={outputIntent === value}
              disabled={value === "single_short" && duration > 180}
              onClick={() => onOutputIntentChange(value)}
            >
              {label}
            </Button>
          ))}
        </fieldset>
      </div>

      <div className="gaming-panel space-y-2 p-4">
        <p className="text-sm text-muted-foreground">
          {t("results.fileSize")}:{" "}
          {Math.max(1, Math.round(estimatedBytes / 1024 / 1024))} MB
        </p>
        {activePath && (
          // Source gameplay clips have no authored caption track at review time.
          // eslint-disable-next-line jsx-a11y/media-has-caption
          <video
            src={convertFileSrc(activePath)}
            className="mx-auto max-h-[48vh] w-full bg-black object-contain"
            controls
            preload="metadata"
          />
        )}
      </div>

      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={({ active, over }) => {
          if (!over || active.id === over.id) return;
          const from = clips.findIndex((clip) => clip.file_path === active.id);
          const to = clips.findIndex((clip) => clip.file_path === over.id);
          if (from >= 0 && to >= 0) onMove(from, to);
        }}
      >
        <SortableContext
          items={clips.map((clip) => clip.file_path)}
          strategy={verticalListSortingStrategy}
        >
          <ol
            className="space-y-3"
            aria-label={t(
              "autoEdit.storyboard.timeline",
              "Storyboard timeline",
            )}
          >
            {clips.map((clip, index) => (
              <SortableStoryboardItem
                key={`${clip.game_id}:${clip.file_path}`}
                id={clip.file_path}
              >
                <button
                  type="button"
                  className="aspect-video w-full rounded-md bg-black"
                  onClick={() => setActivePath(clip.file_path)}
                >
                  {clip.thumbnail_path ? (
                    <img
                      src={convertFileSrc(clip.thumbnail_path)}
                      alt=""
                      className="h-full w-full object-contain"
                    />
                  ) : (
                    <span className="text-xs text-muted-foreground">
                      {t("results.play")}
                    </span>
                  )}
                </button>
                <div className="min-w-0 space-y-3">
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex min-w-0 items-start gap-2">
                      <GripVertical
                        className="mt-1 h-4 w-4 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                      />
                      <div className="min-w-0">
                        <p className="font-medium">
                          #{index + 1} · {clip.event_type}
                        </p>
                        <p className="truncate text-xs text-muted-foreground">
                          {clip.file_path}
                        </p>
                      </div>
                    </div>
                    <div className="flex gap-1">
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        disabled={index === 0}
                        onClick={() => onMove(index, index - 1)}
                        aria-label={t(
                          "autoEdit.storyboard.moveUp",
                          "Move clip up",
                        )}
                      >
                        <ArrowUp className="h-4 w-4" />
                      </Button>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        disabled={index === clips.length - 1}
                        onClick={() => onMove(index, index + 1)}
                        aria-label={t(
                          "autoEdit.storyboard.moveDown",
                          "Move clip down",
                        )}
                      >
                        <ArrowDown className="h-4 w-4" />
                      </Button>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        onClick={() => onRemove(clip.file_path)}
                        aria-label={t(
                          "autoEdit.storyboard.remove",
                          "Exclude clip",
                        )}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                  <div className="grid gap-3 sm:grid-cols-2">
                    <div className="space-y-1">
                      <Label htmlFor={`trim-start-${index}`}>
                        {t("autoEdit.storyboard.start", "Start (seconds)")}
                      </Label>
                      <Input
                        id={`trim-start-${index}`}
                        type="number"
                        min={0}
                        max={Math.max(0, clip.trim_end_secs - 0.1)}
                        step={0.1}
                        value={Number(clip.trim_start_secs.toFixed(1))}
                        onChange={(event) =>
                          onTrim(
                            clip.file_path,
                            Number(event.target.value),
                            clip.trim_end_secs,
                          )
                        }
                        onKeyDown={(event) => {
                          if (
                            !event.shiftKey ||
                            !["ArrowUp", "ArrowDown"].includes(event.key)
                          )
                            return;
                          event.preventDefault();
                          onTrim(
                            clip.file_path,
                            clip.trim_start_secs +
                              (event.key === "ArrowUp" ? 1 : -1),
                            clip.trim_end_secs,
                          );
                        }}
                      />
                    </div>
                    <div className="space-y-1">
                      <Label htmlFor={`trim-end-${index}`}>
                        {t("autoEdit.storyboard.end", "End (seconds)")}
                      </Label>
                      <Input
                        id={`trim-end-${index}`}
                        type="number"
                        min={clip.trim_start_secs + 0.1}
                        max={clip.source_duration_secs}
                        step={0.1}
                        value={Number(clip.trim_end_secs.toFixed(1))}
                        onChange={(event) =>
                          onTrim(
                            clip.file_path,
                            clip.trim_start_secs,
                            Number(event.target.value),
                          )
                        }
                        onKeyDown={(event) => {
                          if (
                            !event.shiftKey ||
                            !["ArrowUp", "ArrowDown"].includes(event.key)
                          )
                            return;
                          event.preventDefault();
                          onTrim(
                            clip.file_path,
                            clip.trim_start_secs,
                            clip.trim_end_secs +
                              (event.key === "ArrowUp" ? 1 : -1),
                          );
                        }}
                      />
                    </div>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {t("autoEdit.storyboard.clipDuration", {
                      defaultValue: "Selected {{duration}} of {{source}}",
                      duration: formatDuration(
                        clip.trim_end_secs - clip.trim_start_secs,
                      ),
                      source: formatDuration(clip.source_duration_secs),
                    })}
                  </p>
                  {outputIntent === "shorts_series" &&
                    index > 0 &&
                    Math.floor((cumulativeEnds[index - 1] - 0.001) / 180) >
                      Math.floor((cumulativeEnds[index - 2] - 0.001) / 180) && (
                      <p className="text-xs font-medium text-primary">
                        {t("resultSeries.part", {
                          current:
                            Math.floor(
                              (cumulativeEnds[index - 1] - 0.001) / 180,
                            ) + 1,
                          total: partCount,
                        })}
                      </p>
                    )}
                </div>
              </SortableStoryboardItem>
            ))}
          </ol>
        </SortableContext>
      </DndContext>

      <div className="sticky bottom-0 flex justify-between gap-3 border-t bg-background/95 py-4 backdrop-blur">
        <Button type="button" variant="outline" onClick={onBack}>
          {t("common.back", "Back")}
        </Button>
        <div className="text-right">
          {inlineError && (
            <p className="mb-1 text-sm text-destructive" role="alert">
              {inlineError}
            </p>
          )}
          <Button
            type="button"
            disabled={isLoading || inlineError !== null}
            onClick={onGenerate}
          >
            {outputIntent === "shorts_series"
              ? t("autoEdit.storyboard.generateSeries", "Generate series")
              : t("autoEdit.generate", "Generate")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function SortableStoryboardItem({
  id,
  children,
}: {
  id: string;
  children: React.ReactNode;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });
  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={`gaming-panel grid gap-4 p-4 md:grid-cols-[240px_1fr] ${isDragging ? "opacity-60" : ""}`}
      {...attributes}
      {...listeners}
    >
      {children}
    </li>
  );
}
