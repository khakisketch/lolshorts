import { memo, useState, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { TimelineClip as TimelineClipType } from "@/stores/editorStore";
import { Button } from "@/components/ui/button";
import { X, GripVertical } from "lucide-react";
import { useEditorStore } from "@/stores/editorStore";
import { convertFileSrc } from "@tauri-apps/api/core";
import { formatDuration } from "@/lib/utils";

interface TimelineClipProps {
  clip: TimelineClipType;
  zoom: number;
}

const PIXELS_PER_SECOND = 50;
const MIN_CLIP_WIDTH = 100;

export const TimelineClip = memo(
  function TimelineClip({ clip, zoom }: TimelineClipProps) {
    const { t } = useTranslation();
    // Performance Optimization: Use granular selectors to prevent unnecessary re-renders
    const removeFromTimeline = useEditorStore(
      (state) => state.removeFromTimeline,
    );
    const setSelectedClipId = useEditorStore(
      (state) => state.setSelectedClipId,
    );
    const setClipTrimStart = useEditorStore((state) => state.setClipTrimStart);
    const setClipTrimEnd = useEditorStore((state) => state.setClipTrimEnd);
    const getClipEffectiveDuration = useEditorStore(
      (state) => state.getClipEffectiveDuration,
    );
    const isSelected = useEditorStore(
      (state) => state.selectedClipId === clip.file_path,
    );

    // Trim handle drag state
    const [isDraggingTrimStart, setIsDraggingTrimStart] = useState(false);
    const [isDraggingTrimEnd, setIsDraggingTrimEnd] = useState(false);
    const containerRef = useRef<HTMLDivElement>(null);

    const {
      attributes,
      listeners,
      setNodeRef,
      transform,
      transition,
      isDragging,
    } = useSortable({ id: clip.file_path });

    const style = {
      transform: CSS.Transform.toString(transform),
      transition,
      opacity: isDragging ? 0.5 : 1,
    };

    // Calculate clip width based on effective duration (after trimming)
    const effectiveDuration = getClipEffectiveDuration(clip);
    const clipWidth = Math.max(
      MIN_CLIP_WIDTH,
      effectiveDuration * PIXELS_PER_SECOND * zoom,
    );

    // Trim handle drag handlers
    const handleTrimStartDrag = useCallback(
      (e: React.MouseEvent) => {
        e.stopPropagation();
        e.preventDefault();
        setIsDraggingTrimStart(true);

        const startX = e.clientX;
        const originalTrimStart = clip.trimStart ?? 0;

        const handleMouseMove = (moveEvent: MouseEvent) => {
          const deltaX = moveEvent.clientX - startX;
          const deltaSeconds = deltaX / (PIXELS_PER_SECOND * zoom);
          const newTrimStart = Math.max(0, originalTrimStart + deltaSeconds);
          setClipTrimStart(clip.file_path, newTrimStart);
        };

        const handleMouseUp = () => {
          setIsDraggingTrimStart(false);
          document.removeEventListener("mousemove", handleMouseMove);
          document.removeEventListener("mouseup", handleMouseUp);
        };

        document.addEventListener("mousemove", handleMouseMove);
        document.addEventListener("mouseup", handleMouseUp);
      },
      [clip.file_path, clip.trimStart, zoom, setClipTrimStart],
    );

    const handleTrimEndDrag = useCallback(
      (e: React.MouseEvent) => {
        e.stopPropagation();
        e.preventDefault();
        setIsDraggingTrimEnd(true);

        const startX = e.clientX;
        const originalTrimEnd = clip.trimEnd ?? clip.duration;

        const handleMouseMove = (moveEvent: MouseEvent) => {
          const deltaX = moveEvent.clientX - startX;
          const deltaSeconds = deltaX / (PIXELS_PER_SECOND * zoom);
          const newTrimEnd = Math.min(
            clip.duration,
            originalTrimEnd + deltaSeconds,
          );
          setClipTrimEnd(clip.file_path, newTrimEnd);
        };

        const handleMouseUp = () => {
          setIsDraggingTrimEnd(false);
          document.removeEventListener("mousemove", handleMouseMove);
          document.removeEventListener("mouseup", handleMouseUp);
        };

        document.addEventListener("mousemove", handleMouseMove);
        document.addEventListener("mouseup", handleMouseUp);
      },
      [clip.file_path, clip.trimEnd, clip.duration, zoom, setClipTrimEnd],
    );

    const handleRemove = () => {
      removeFromTimeline(clip.file_path);
    };

    const handleClick = () => {
      setSelectedClipId(clip.file_path);
    };

    const handleKeyDown = (event: React.KeyboardEvent) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        handleClick();
      }
    };

    // Convert thumbnail path
    const thumbnailSrc = clip.thumbnail_path
      ? convertFileSrc(clip.thumbnail_path)
      : undefined;

    // Calculate trim info for display
    const trimStart = clip.trimStart ?? 0;
    const trimEnd = clip.trimEnd ?? clip.duration;
    const isTrimmed = trimStart > 0 || trimEnd < clip.duration;

    return (
      <div
        ref={(node) => {
          setNodeRef(node);
          if (containerRef) {
            (
              containerRef as React.MutableRefObject<HTMLDivElement | null>
            ).current = node;
          }
        }}
        style={{ ...style, width: `${clipWidth}px` }}
        className={`
        relative h-32 bg-card border-2 rounded-lg overflow-hidden cursor-pointer
        transition-all hover:border-primary group
        ${isSelected ? "border-primary ring-2 ring-primary" : "border-border"}
        ${isDragging ? "shadow-lg" : ""}
        ${isDraggingTrimStart || isDraggingTrimEnd ? "cursor-ew-resize" : ""}
      `}
        onClick={handleClick}
        onKeyDown={handleKeyDown}
        role="button"
        tabIndex={0}
        aria-label={`Timeline clip at ${formatDuration(clip.event_time)}, trimmed from ${formatDuration(trimStart)} to ${formatDuration(trimEnd)}`}
        aria-pressed={isSelected}
      >
        {/* Left Trim Handle */}
        <div
          className={`
          absolute left-0 top-0 bottom-0 w-3 z-20 cursor-ew-resize
          bg-primary/0 hover:bg-primary/30 transition-colors
          flex items-center justify-center
          ${isDraggingTrimStart ? "bg-primary/50" : ""}
          ${isSelected ? "opacity-100" : "opacity-0 group-hover:opacity-100"}
        `}
          role="slider"
          tabIndex={0}
          aria-label="Drag to trim start"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round((trimStart / clip.duration) * 100)}
          onMouseDown={handleTrimStartDrag}
          onKeyDown={(e) => {
            if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
              e.preventDefault();
            }
          }}
        >
          <div className="w-1 h-8 bg-primary rounded-full" />
        </div>

        {/* Right Trim Handle */}
        <div
          className={`
          absolute right-0 top-0 bottom-0 w-3 z-20 cursor-ew-resize
          bg-primary/0 hover:bg-primary/30 transition-colors
          flex items-center justify-center
          ${isDraggingTrimEnd ? "bg-primary/50" : ""}
          ${isSelected ? "opacity-100" : "opacity-0 group-hover:opacity-100"}
        `}
          role="slider"
          tabIndex={0}
          aria-label="Drag to trim end"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round((trimEnd / clip.duration) * 100)}
          onMouseDown={handleTrimEndDrag}
          onKeyDown={(e) => {
            if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
              e.preventDefault();
            }
          }}
        >
          <div className="w-1 h-8 bg-primary rounded-full" />
        </div>

        {/* Drag Handle */}
        <div
          {...attributes}
          {...listeners}
          className="absolute top-1 left-4 z-10 cursor-grab active:cursor-grabbing bg-background/80 rounded p-1"
        >
          <GripVertical className="w-4 h-4 text-muted-foreground" />
        </div>

        {/* Remove Button */}
        <Button
          size="sm"
          variant="destructive"
          className="absolute top-1 right-4 z-10 h-6 w-6 p-0"
          onClick={(e) => {
            e.stopPropagation();
            handleRemove();
          }}
        >
          <X className="w-3 h-3" />
        </Button>

        {/* Thumbnail */}
        <div className="relative w-full h-full">
          {thumbnailSrc ? (
            <img
              src={thumbnailSrc}
              alt="Clip thumbnail"
              className="w-full h-full object-cover"
            />
          ) : (
            <div className="w-full h-full bg-muted flex items-center justify-center">
              <span className="text-xs text-muted-foreground">
                {t("editor.timeline.noThumbnail")}
              </span>
            </div>
          )}

          {/* Clip Info Overlay */}
          <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent p-2">
            <div className="flex items-center justify-between text-white text-xs">
              <span className="font-medium">#{clip.order + 1}</span>
              <div className="text-right">
                <span>{formatDuration(effectiveDuration)}</span>
                {isTrimmed && (
                  <span
                    className="ml-1 text-yellow-400"
                    title={`Trimmed: ${formatDuration(trimStart)} - ${formatDuration(trimEnd)}`}
                  >
                    ✂
                  </span>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  },
  (prevProps, nextProps) => {
    // Custom comparison function for React.memo
    return (
      prevProps.clip.file_path === nextProps.clip.file_path &&
      prevProps.clip.duration === nextProps.clip.duration &&
      prevProps.clip.order === nextProps.clip.order &&
      prevProps.clip.event_time === nextProps.clip.event_time &&
      prevProps.clip.trimStart === nextProps.clip.trimStart &&
      prevProps.clip.trimEnd === nextProps.clip.trimEnd &&
      prevProps.zoom === nextProps.zoom
    );
  },
);
