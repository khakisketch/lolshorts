import { useMemo, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  DndContext,
  closestCenter,
  DragEndEvent,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  horizontalListSortingStrategy,
} from "@dnd-kit/sortable";
import { useEditorStore } from "@/stores/editorStore";
import { TimelineClip } from "./TimelineClip";
import { TimelineControls } from "./TimelineControls";
import { Film } from "lucide-react";

export function Timeline() {
  const { t } = useTranslation();
  const { timelineClips, reorderTimeline, zoom } = useEditorStore();
  const selectedClipId = useEditorStore((state) => state.selectedClipId);
  const setSelectedClipId = useEditorStore((state) => state.setSelectedClipId);
  const removeFromTimeline = useEditorStore(
    (state) => state.removeFromTimeline,
  );
  const clipListRef = useRef<HTMLDivElement>(null);

  const handleTimelineKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (timelineClips.length === 0) return;

      const currentIndex = timelineClips.findIndex(
        (c) => c.file_path === selectedClipId,
      );

      switch (e.key) {
        case "ArrowLeft": {
          e.preventDefault();
          const prevIndex =
            currentIndex > 0 ? currentIndex - 1 : timelineClips.length - 1;
          setSelectedClipId(timelineClips[prevIndex].file_path);
          const buttons =
            clipListRef.current?.querySelectorAll('[role="button"]');
          (buttons?.[prevIndex] as HTMLElement)?.focus();
          break;
        }
        case "ArrowRight": {
          e.preventDefault();
          const nextIndex =
            currentIndex < timelineClips.length - 1 ? currentIndex + 1 : 0;
          setSelectedClipId(timelineClips[nextIndex].file_path);
          const buttons =
            clipListRef.current?.querySelectorAll('[role="button"]');
          (buttons?.[nextIndex] as HTMLElement)?.focus();
          break;
        }
        case "Delete":
        case "Backspace": {
          if (selectedClipId) {
            e.preventDefault();
            removeFromTimeline(selectedClipId);
          }
          break;
        }
      }
    },
    [timelineClips, selectedClipId, setSelectedClipId, removeFromTimeline],
  );

  // Memoize sensors to prevent recreation on every render
  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 8,
      },
    }),
  );

  // Memoize clip IDs for SortableContext
  const clipIds = useMemo(
    () => timelineClips.map((c) => c.file_path),
    [timelineClips],
  );

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event;

      if (over && active.id !== over.id) {
        const oldIndex = timelineClips.findIndex(
          (c) => c.file_path === active.id.toString(),
        );
        const newIndex = timelineClips.findIndex(
          (c) => c.file_path === over.id.toString(),
        );

        if (oldIndex !== -1 && newIndex !== -1) {
          reorderTimeline(oldIndex, newIndex);
        }
      }
    },
    [timelineClips, reorderTimeline],
  );

  return (
    <div className="h-full flex flex-col">
      <div className="border-b p-3">
        <TimelineControls />
      </div>

      <div className="flex-1 overflow-x-auto overflow-y-hidden">
        {timelineClips.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center">
            <Film className="w-12 h-12 text-muted-foreground mb-2" />
            <p className="text-sm text-muted-foreground">
              {t("editor.timeline.addClipsPrompt")}
            </p>
            <p className="text-xs text-muted-foreground mt-1">
              {t("editor.timeline.dragToReorder")}
            </p>
          </div>
        ) : (
          <DndContext
            sensors={sensors}
            collisionDetection={closestCenter}
            onDragEnd={handleDragEnd}
          >
            <SortableContext
              items={clipIds}
              strategy={horizontalListSortingStrategy}
            >
              <div
                ref={clipListRef}
                className="flex gap-2 p-4 h-full items-center"
                onKeyDown={handleTimelineKeyDown}
                role="listbox"
                tabIndex={0}
                aria-label={t("editor.timeline.clipList", "Timeline clips")}
              >
                {timelineClips.map((clip) => (
                  <TimelineClip key={clip.file_path} clip={clip} zoom={zoom} />
                ))}
              </div>
            </SortableContext>
          </DndContext>
        )}
      </div>
    </div>
  );
}
