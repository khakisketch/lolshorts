import { useRef, useEffect, useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useEditorStore } from "@/stores/editorStore";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import { Play, Pause, Volume2, VolumeX, Maximize } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { formatDuration } from "@/lib/utils";
import { logger } from "@/lib/logger";

export function VideoPreview() {
  const { t } = useTranslation();
  const videoRef = useRef<HTMLVideoElement>(null);
  const [volume, setVolume] = useState(1.0);
  const [isMuted, setIsMuted] = useState(false);
  const [currentVideoTime, setCurrentVideoTime] = useState(0);
  const [duration, setDuration] = useState(0);

  const {
    selectedClipId,
    availableClips,
    timelineClips,
    isPlaying,
    play,
    pause,
    setCurrentTime,
  } = useEditorStore();

  // Get selected clip
  const selectedClip = availableClips.find(
    (c) => c.file_path === selectedClipId,
  );

  // If the selected clip is on the timeline, it may have a trim range - the
  // library-only ClipMetadata has no trim fields, so look it up separately.
  const timelineClip = timelineClips.find(
    (c) => c.file_path === selectedClipId,
  );
  const trimStart = timelineClip?.trimStart ?? 0;
  // trimEnd is undefined until the video's real duration is known (metadata
  // load), at which point it falls back to the full duration.
  const trimEnd = timelineClip?.trimEnd;
  const effectiveEnd = trimEnd ?? duration;

  // Update video source when clip changes
  useEffect(() => {
    if (videoRef.current && selectedClip) {
      const src = convertFileSrc(selectedClip.file_path);
      videoRef.current.src = src;
      videoRef.current.load();
      setCurrentVideoTime(trimStart);
    }
    // Only reset when the clip itself changes, not on every trim adjustment.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedClip]);

  // Handle play/pause state
  useEffect(() => {
    if (videoRef.current) {
      if (isPlaying) {
        videoRef.current
          .play()
          .catch((err) => logger.error("Play failed:", err));
      } else {
        videoRef.current.pause();
      }
    }
  }, [isPlaying]);

  // Update volume
  useEffect(() => {
    if (videoRef.current) {
      videoRef.current.volume = isMuted ? 0 : volume;
    }
  }, [volume, isMuted]);

  const handleTimeUpdate = useCallback(() => {
    if (videoRef.current) {
      const t = videoRef.current.currentTime;
      // Auto-stop at the trimmed end so the preview reflects what will
      // actually be exported (compose_shorts_v2 cuts trimEnd from the clip).
      if (trimEnd !== undefined && t >= trimEnd) {
        videoRef.current.pause();
        videoRef.current.currentTime = trimEnd;
        pause();
        setCurrentVideoTime(trimEnd);
        setCurrentTime(trimEnd);
        return;
      }
      setCurrentVideoTime(t);
      setCurrentTime(t);
    }
  }, [pause, setCurrentTime, trimEnd]);

  const handleLoadedMetadata = useCallback(() => {
    if (videoRef.current) {
      setDuration(videoRef.current.duration);
      // Start playback position at the trim-in point rather than 0.
      if (trimStart > 0) {
        videoRef.current.currentTime = trimStart;
        setCurrentVideoTime(trimStart);
      }
    }
    // Only re-run when the clip (and thus trimStart) actually changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedClip]);

  const handlePlayPause = useCallback(() => {
    if (isPlaying) {
      pause();
    } else {
      // If the playhead is outside the trimmed range (e.g. sitting at the
      // trimmed-out end from a previous auto-stop), restart from trimStart.
      if (videoRef.current) {
        const t = videoRef.current.currentTime;
        if (t < trimStart || t >= effectiveEnd) {
          videoRef.current.currentTime = trimStart;
          setCurrentVideoTime(trimStart);
        }
      }
      play();
    }
  }, [isPlaying, pause, play, trimStart, effectiveEnd]);

  const handleSeek = useCallback(
    (value: number[]) => {
      if (videoRef.current) {
        const clamped = Math.min(Math.max(value[0], trimStart), effectiveEnd);
        videoRef.current.currentTime = clamped;
        setCurrentVideoTime(clamped);
      }
    },
    [trimStart, effectiveEnd],
  );

  const handleVolumeChange = useCallback((value: number[]) => {
    setVolume(value[0]);
    setIsMuted(value[0] === 0);
  }, []);

  const toggleMute = useCallback(() => {
    setIsMuted((prev) => !prev);
  }, []);

  const toggleFullscreen = useCallback(() => {
    if (videoRef.current) {
      if (document.fullscreenElement) {
        document.exitFullscreen();
      } else {
        videoRef.current.requestFullscreen();
      }
    }
  }, []);

  return (
    <div className="flex flex-col h-full bg-black">
      {/* Video Player */}
      <div className="flex-1 flex items-center justify-center relative">
        {selectedClip ? (
          <video
            ref={videoRef}
            className="max-w-full max-h-full"
            onTimeUpdate={handleTimeUpdate}
            onLoadedMetadata={handleLoadedMetadata}
            onEnded={() => pause()}
          >
            <track kind="captions" label={t("editor.preview.noCaptions")} />
          </video>
        ) : (
          <div className="text-center p-8">
            <Play className="w-24 h-24 mx-auto text-muted-foreground mb-4" />
            <p className="text-muted-foreground">
              {t("editor.preview.selectClipHint")}
            </p>
          </div>
        )}
      </div>

      {/* Custom Controls */}
      {selectedClip && (
        <div className="bg-card border-t p-4 space-y-2">
          {/* Seek Bar - clamped to the timeline clip's trim range, if any */}
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground w-12 text-right">
              {formatDuration(currentVideoTime)}
            </span>
            <Slider
              value={[currentVideoTime]}
              min={trimStart}
              max={effectiveEnd}
              step={0.1}
              onValueChange={handleSeek}
              className="flex-1"
            />
            <span className="text-xs text-muted-foreground w-12">
              {formatDuration(effectiveEnd)}
            </span>
          </div>
          {timelineClip &&
            (trimStart > 0 ||
              (trimEnd !== undefined && trimEnd < duration)) && (
              <p className="text-xs text-muted-foreground text-center">
                {t("editor.preview.trimmedRangeNotice", {
                  start: formatDuration(trimStart),
                  end: formatDuration(effectiveEnd),
                  defaultValue: "Trimmed preview: {{start}} - {{end}}",
                })}
              </p>
            )}

          {/* Control Buttons */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              {/* Play/Pause */}
              <Button size="sm" variant="ghost" onClick={handlePlayPause}>
                {isPlaying ? (
                  <Pause className="w-5 h-5" />
                ) : (
                  <Play className="w-5 h-5" />
                )}
              </Button>

              {/* Volume */}
              <div className="flex items-center gap-2">
                <Button size="sm" variant="ghost" onClick={toggleMute}>
                  {isMuted || volume === 0 ? (
                    <VolumeX className="w-5 h-5" />
                  ) : (
                    <Volume2 className="w-5 h-5" />
                  )}
                </Button>
                <Slider
                  value={[isMuted ? 0 : volume]}
                  max={1}
                  step={0.1}
                  onValueChange={handleVolumeChange}
                  className="w-24"
                />
              </div>
            </div>

            <div className="flex items-center gap-2">
              {/* Fullscreen */}
              <Button size="sm" variant="ghost" onClick={toggleFullscreen}>
                <Maximize className="w-5 h-5" />
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
