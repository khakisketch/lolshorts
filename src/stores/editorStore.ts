import { create } from "zustand";
import { ClipMetadata } from "@/types/storage";

export interface TimelineClip extends ClipMetadata {
  order: number;
  trimStart?: number; // For future trim feature
  trimEnd?: number; // For future trim feature
}

export type AspectRatio = "9:16" | "16:9" | "1:1";
export type TransitionType = "none" | "fade" | "slide";
export type ExportStatus = "idle" | "exporting" | "complete" | "error";

export interface CompositionSettings {
  aspectRatio: AspectRatio;
  transitionType: TransitionType;
  transitionDuration: number; // In seconds
}

interface EditorStore {
  // Selection
  selectedGameId: string | null;
  availableClips: ClipMetadata[];

  // Timeline
  timelineClips: TimelineClip[];
  currentTime: number;
  totalDuration: number;
  isPlaying: boolean;
  selectedClipId: string | null;
  zoom: number; // 1.0 = normal, 2.0 = 2x zoom

  // Composition
  compositionSettings: CompositionSettings;

  // Export
  exportProgress: number;
  exportStatus: ExportStatus;
  exportError: string | null;
  exportOutputPath: string | null;

  // Actions - Game & Clip Management
  setSelectedGameId: (gameId: string | null) => void;
  setAvailableClips: (clips: ClipMetadata[]) => void;
  clearEditor: () => void;

  // Actions - Timeline Management
  addToTimeline: (clip: ClipMetadata) => void;
  removeFromTimeline: (clipId: string) => void;
  reorderTimeline: (fromIndex: number, toIndex: number) => void;
  clearTimeline: () => void;
  setClipTrimStart: (clipId: string, trimStart: number) => void;
  setClipTrimEnd: (clipId: string, trimEnd: number) => void;
  getClipEffectiveDuration: (clip: TimelineClip) => number;

  // Actions - Playback
  setCurrentTime: (time: number) => void;
  setIsPlaying: (playing: boolean) => void;
  setSelectedClipId: (clipId: string | null) => void;
  play: () => void;
  pause: () => void;
  stop: () => void;

  // Actions - View
  setZoom: (zoom: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  resetZoom: () => void;

  // Actions - Composition
  updateCompositionSettings: (settings: Partial<CompositionSettings>) => void;
  setAspectRatio: (ratio: AspectRatio) => void;
  setTransitionType: (type: TransitionType) => void;
  setTransitionDuration: (duration: number) => void;

  // Actions - Export
  setExportProgress: (progress: number) => void;
  setExportStatus: (status: ExportStatus) => void;
  setExportError: (error: string | null) => void;
  setExportOutputPath: (path: string | null) => void;
  resetExport: () => void;
}

export const useEditorStore = create<EditorStore>((set, get) => ({
  // Initial state
  selectedGameId: null,
  availableClips: [],

  timelineClips: [],
  currentTime: 0,
  totalDuration: 0,
  isPlaying: false,
  selectedClipId: null,
  zoom: 1.0,

  compositionSettings: {
    aspectRatio: "9:16",
    transitionType: "fade",
    transitionDuration: 0.5,
  },

  exportProgress: 0,
  exportStatus: "idle",
  exportError: null,
  exportOutputPath: null,

  // Game & Clip Management
  setSelectedGameId: (gameId) => set({ selectedGameId: gameId }),

  setAvailableClips: (clips) => set({ availableClips: clips }),

  clearEditor: () =>
    set({
      selectedGameId: null,
      availableClips: [],
      timelineClips: [],
      currentTime: 0,
      totalDuration: 0,
      isPlaying: false,
      selectedClipId: null,
      exportProgress: 0,
      exportStatus: "idle",
      exportError: null,
      exportOutputPath: null,
    }),

  // Timeline Management
  addToTimeline: (clip) => {
    const { timelineClips } = get();
    const order = timelineClips.length;
    const newClip: TimelineClip = {
      ...clip,
      order,
    };
    const newTimeline = [...timelineClips, newClip];
    const totalDuration = newTimeline.reduce((sum, c) => sum + c.duration, 0);

    set({
      timelineClips: newTimeline,
      totalDuration,
    });
  },

  removeFromTimeline: (clipId) => {
    const { timelineClips } = get();
    const newTimeline = timelineClips
      .filter((c) => c.file_path !== clipId)
      .map((c, index) => ({ ...c, order: index }));
    const totalDuration = newTimeline.reduce((sum, c) => sum + c.duration, 0);

    set({
      timelineClips: newTimeline,
      totalDuration,
      selectedClipId:
        get().selectedClipId === clipId ? null : get().selectedClipId,
    });
  },

  reorderTimeline: (fromIndex, toIndex) => {
    const { timelineClips } = get();
    const newTimeline = [...timelineClips];
    const [movedClip] = newTimeline.splice(fromIndex, 1);
    newTimeline.splice(toIndex, 0, movedClip);

    // Update order numbers
    const reorderedTimeline = newTimeline.map((c, index) => ({
      ...c,
      order: index,
    }));

    set({ timelineClips: reorderedTimeline });
  },

  clearTimeline: () =>
    set({
      timelineClips: [],
      totalDuration: 0,
      currentTime: 0,
      isPlaying: false,
      selectedClipId: null,
    }),

  setClipTrimStart: (clipId, trimStart) => {
    const { timelineClips } = get();
    const newTimeline = timelineClips.map((clip) => {
      if (clip.file_path === clipId) {
        const maxTrimStart = (clip.trimEnd ?? clip.duration) - 0.5; // Min 0.5s duration
        return {
          ...clip,
          trimStart: Math.max(0, Math.min(trimStart, maxTrimStart)),
        };
      }
      return clip;
    });

    const totalDuration = newTimeline.reduce((sum, c) => {
      const effectiveStart = c.trimStart ?? 0;
      const effectiveEnd = c.trimEnd ?? c.duration;
      return sum + (effectiveEnd - effectiveStart);
    }, 0);

    set({ timelineClips: newTimeline, totalDuration });
  },

  setClipTrimEnd: (clipId, trimEnd) => {
    const { timelineClips } = get();
    const newTimeline = timelineClips.map((clip) => {
      if (clip.file_path === clipId) {
        const minTrimEnd = (clip.trimStart ?? 0) + 0.5; // Min 0.5s duration
        return {
          ...clip,
          trimEnd: Math.max(minTrimEnd, Math.min(trimEnd, clip.duration)),
        };
      }
      return clip;
    });

    const totalDuration = newTimeline.reduce((sum, c) => {
      const effectiveStart = c.trimStart ?? 0;
      const effectiveEnd = c.trimEnd ?? c.duration;
      return sum + (effectiveEnd - effectiveStart);
    }, 0);

    set({ timelineClips: newTimeline, totalDuration });
  },

  getClipEffectiveDuration: (clip) => {
    const effectiveStart = clip.trimStart ?? 0;
    const effectiveEnd = clip.trimEnd ?? clip.duration;
    return effectiveEnd - effectiveStart;
  },

  // Playback
  setCurrentTime: (time) =>
    set({ currentTime: Math.max(0, Math.min(time, get().totalDuration)) }),

  setIsPlaying: (playing) => set({ isPlaying: playing }),

  setSelectedClipId: (clipId) => set({ selectedClipId: clipId }),

  play: () => set({ isPlaying: true }),

  pause: () => set({ isPlaying: false }),

  stop: () =>
    set({
      isPlaying: false,
      currentTime: 0,
      selectedClipId: null,
    }),

  // View
  setZoom: (zoom) => set({ zoom: Math.max(0.5, Math.min(zoom, 4.0)) }),

  zoomIn: () => {
    const { zoom } = get();
    set({ zoom: Math.min(zoom * 1.5, 4.0) });
  },

  zoomOut: () => {
    const { zoom } = get();
    set({ zoom: Math.max(zoom / 1.5, 0.5) });
  },

  resetZoom: () => set({ zoom: 1.0 }),

  // Composition
  updateCompositionSettings: (settings) =>
    set({
      compositionSettings: { ...get().compositionSettings, ...settings },
    }),

  setAspectRatio: (ratio) =>
    set({
      compositionSettings: { ...get().compositionSettings, aspectRatio: ratio },
    }),

  setTransitionType: (type) =>
    set({
      compositionSettings: {
        ...get().compositionSettings,
        transitionType: type,
      },
    }),

  setTransitionDuration: (duration) =>
    set({
      compositionSettings: {
        ...get().compositionSettings,
        transitionDuration: Math.max(0, Math.min(duration, 2.0)),
      },
    }),

  // Export
  setExportProgress: (progress) =>
    set({ exportProgress: Math.max(0, Math.min(progress, 100)) }),

  setExportStatus: (status) => set({ exportStatus: status }),

  setExportError: (error) => set({ exportError: error }),

  setExportOutputPath: (path) => set({ exportOutputPath: path }),

  resetExport: () =>
    set({
      exportProgress: 0,
      exportStatus: "idle",
      exportError: null,
      exportOutputPath: null,
    }),
}));
