/**
 * Video Player Component
 *
 * Handles video playback with controls, keyboard shortcuts, and accessibility
 */

import { useState, useRef, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Volume2,
  VolumeX,
  Maximize2,
  Minimize2,
  RotateCcw,
} from "lucide-react";
import { toast } from "@/components/ui/use-toast";
import { logger } from "@/lib/logger";
import { getErrorMessage } from "@/lib/utils";

interface VideoPlayerProps {
  src: string;
  title?: string;
  autoPlay?: boolean;
  onClose?: () => void;
  className?: string;
}

export function VideoPlayer({
  src,
  title,
  autoPlay = false,
  onClose,
  className = "",
}: VideoPlayerProps) {
  const { t } = useTranslation();
  const videoRef = useRef<HTMLVideoElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const [isPlaying, setIsPlaying] = useState(autoPlay);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [isMuted, setIsMuted] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [playbackRate, setPlaybackRate] = useState(1);

  // Format time helper
  const formatTime = useCallback((seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  }, []);

  // Handle play/pause
  const togglePlay = useCallback(async () => {
    if (!videoRef.current) return;

    try {
      if (isPlaying) {
        await videoRef.current.pause();
        setIsPlaying(false);
      } else {
        await videoRef.current.play();
        setIsPlaying(true);
      }
    } catch (error) {
      logger.error("Failed to toggle playback:", error);
      toast({
        title: t("video.errors.playbackFailed"),
        description: getErrorMessage(error),
        variant: "destructive",
      });
    }
  }, [isPlaying, t]);

  // Handle seeking
  const handleSeek = useCallback((value: number[]) => {
    if (videoRef.current) {
      videoRef.current.currentTime = value[0];
      setCurrentTime(value[0]);
    }
  }, []);

  // Handle volume change
  const handleVolumeChange = useCallback((value: number[]) => {
    const newVolume = value[0];
    setVolume(newVolume);
    setIsMuted(newVolume === 0);

    if (videoRef.current) {
      videoRef.current.volume = newVolume;
      videoRef.current.muted = newVolume === 0;
    }
  }, []);

  // Toggle mute
  const toggleMute = useCallback(() => {
    const newMuted = !isMuted;
    setIsMuted(newMuted);

    if (videoRef.current) {
      videoRef.current.muted = newMuted;
    }
  }, [isMuted]);

  // Skip forward/backward
  const skip = useCallback(
    (seconds: number) => {
      if (videoRef.current) {
        videoRef.current.currentTime = Math.max(
          0,
          Math.min(duration, currentTime + seconds),
        );
      }
    },
    [currentTime, duration],
  );

  // Toggle fullscreen
  const toggleFullscreen = useCallback(() => {
    if (!document.fullscreenElement) {
      containerRef.current?.requestFullscreen();
      setIsFullscreen(true);
    } else {
      document.exitFullscreen();
      setIsFullscreen(false);
    }
  }, []);

  // Reset playback speed
  const resetPlaybackRate = useCallback(() => {
    setPlaybackRate(1);
    if (videoRef.current) {
      videoRef.current.playbackRate = 1;
    }
  }, []);

  // Video event handlers
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const handleLoadedMetadata = () => {
      setDuration(video.duration);
      setIsLoading(false);
    };

    const handleTimeUpdate = () => {
      setCurrentTime(video.currentTime);
    };

    const handleEnded = () => {
      setIsPlaying(false);
    };

    const handleError = () => {
      setIsLoading(false);
      toast({
        title: t("video.errors.loadFailed"),
        description: t("video.errors.loadFailedDesc"),
        variant: "destructive",
      });
    };

    video.addEventListener("loadedmetadata", handleLoadedMetadata);
    video.addEventListener("timeupdate", handleTimeUpdate);
    video.addEventListener("ended", handleEnded);
    video.addEventListener("error", handleError);

    return () => {
      video.removeEventListener("loadedmetadata", handleLoadedMetadata);
      video.removeEventListener("timeupdate", handleTimeUpdate);
      video.removeEventListener("ended", handleEnded);
      video.removeEventListener("error", handleError);
    };
  }, [t]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Only handle shortcuts when video player is focused
      if (
        document.activeElement !== videoRef.current &&
        !containerRef.current?.contains(document.activeElement)
      ) {
        return;
      }

      switch (e.key) {
        case " ":
        case "k":
          e.preventDefault();
          togglePlay();
          break;
        case "ArrowLeft":
          e.preventDefault();
          skip(-5);
          break;
        case "ArrowRight":
          e.preventDefault();
          skip(5);
          break;
        case "j":
          e.preventDefault();
          skip(-10);
          break;
        case "l":
          e.preventDefault();
          skip(10);
          break;
        case "m":
          e.preventDefault();
          toggleMute();
          break;
        case "ArrowUp":
          e.preventDefault();
          handleVolumeChange([Math.min(1, volume + 0.05)]);
          break;
        case "ArrowDown":
          e.preventDefault();
          handleVolumeChange([Math.max(0, volume - 0.05)]);
          break;
        case "f":
          e.preventDefault();
          toggleFullscreen();
          break;
        case "r":
          e.preventDefault();
          resetPlaybackRate();
          break;
        case "1":
          e.preventDefault();
          setPlaybackRate(0.5);
          if (videoRef.current) videoRef.current.playbackRate = 0.5;
          break;
        case "2":
          e.preventDefault();
          setPlaybackRate(1);
          if (videoRef.current) videoRef.current.playbackRate = 1;
          break;
        case "3":
          e.preventDefault();
          setPlaybackRate(2);
          if (videoRef.current) videoRef.current.playbackRate = 2;
          break;
        case "Escape":
          if (onClose) {
            e.preventDefault();
            onClose();
          }
          break;
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    togglePlay,
    skip,
    toggleMute,
    toggleFullscreen,
    resetPlaybackRate,
    onClose,
    volume,
    handleVolumeChange,
  ]);

  return (
    <div
      ref={containerRef}
      className={`relative min-h-0 overflow-hidden rounded-lg bg-black ${className}`}
      role="application"
      aria-label={t("video.playerLabel")}
    >
      {/* Video Element */}
      <video
        ref={videoRef}
        src={src}
        className="h-full w-full object-contain"
        autoPlay={autoPlay}
        playsInline
        tabIndex={0}
        aria-label={title || t("video.videoLabel")}
        title={title}
      >
        <track kind="captions" label="No captions available" />
      </video>

      {/* Loading Overlay */}
      {isLoading && (
        <div className="absolute inset-0 flex items-center justify-center bg-black/50">
          <div className="text-white text-center">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-white mx-auto mb-2" />
            <div className="text-sm">{t("video.loading")}</div>
          </div>
        </div>
      )}

      {/* Controls Overlay */}
      <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent p-4">
        <div className="space-y-3">
          {/* Progress Bar */}
          <div className="space-y-2">
            <Slider
              value={[currentTime]}
              max={duration}
              step={1}
              onValueChange={handleSeek}
              className="w-full"
              aria-label={t("video.seekLabel")}
            />
            <div className="flex justify-between text-xs text-white/80">
              <span>{formatTime(currentTime)}</span>
              <span>{formatTime(duration)}</span>
            </div>
          </div>

          {/* Control Buttons */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              {/* Skip Backward */}
              <Button
                variant="ghost"
                size="sm"
                onClick={() => skip(-5)}
                className="text-white hover:text-white/80"
                aria-label={t("video.skipBackward")}
              >
                <SkipBack className="h-4 w-4" />
              </Button>

              {/* Play/Pause */}
              <Button
                variant="ghost"
                size="sm"
                onClick={togglePlay}
                className="text-white hover:text-white/80"
                aria-label={isPlaying ? t("video.pause") : t("video.play")}
              >
                {isPlaying ? (
                  <Pause className="h-4 w-4" />
                ) : (
                  <Play className="h-4 w-4" />
                )}
              </Button>

              {/* Skip Forward */}
              <Button
                variant="ghost"
                size="sm"
                onClick={() => skip(5)}
                className="text-white hover:text-white/80"
                aria-label={t("video.skipForward")}
              >
                <SkipForward className="h-4 w-4" />
              </Button>

              {/* Volume Control */}
              <div className="flex items-center gap-2">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={toggleMute}
                  className="text-white hover:text-white/80"
                  aria-label={isMuted ? t("video.unmute") : t("video.mute")}
                >
                  {isMuted ? (
                    <VolumeX className="h-4 w-4" />
                  ) : (
                    <Volume2 className="h-4 w-4" />
                  )}
                </Button>
                <Slider
                  value={[isMuted ? 0 : volume * 100]}
                  max={100}
                  step={1}
                  onValueChange={(value) =>
                    handleVolumeChange([value[0] / 100])
                  }
                  className="w-20"
                  aria-label={t("video.volumeLabel")}
                />
              </div>

              {/* Playback Speed */}
              <div className="flex items-center gap-1">
                <span className="text-xs text-white/80 min-w-[30px] text-center">
                  {playbackRate}x
                </span>
              </div>
            </div>

            {/* Right Side Controls */}
            <div className="flex items-center gap-2">
              {/* Reset Speed */}
              <Button
                variant="ghost"
                size="sm"
                onClick={resetPlaybackRate}
                className="text-white hover:text-white/80"
                aria-label={t("video.resetSpeed")}
              >
                <RotateCcw className="h-4 w-4" />
              </Button>

              {/* Fullscreen */}
              <Button
                variant="ghost"
                size="sm"
                onClick={toggleFullscreen}
                className="text-white hover:text-white/80"
                aria-label={
                  isFullscreen
                    ? t("video.exitFullscreen")
                    : t("video.enterFullscreen")
                }
              >
                {isFullscreen ? (
                  <Minimize2 className="h-4 w-4" />
                ) : (
                  <Maximize2 className="h-4 w-4" />
                )}
              </Button>
            </div>
          </div>
        </div>
      </div>

      {/* Title */}
      {title && (
        <div className="absolute top-4 left-4 bg-black/50 px-3 py-1 rounded">
          <h3 className="text-white text-sm font-medium truncate max-w-xs">
            {title}
          </h3>
        </div>
      )}

      {/* Close Button */}
      {onClose && (
        <Button
          variant="ghost"
          size="sm"
          onClick={onClose}
          className="absolute top-4 right-4 text-white hover:text-white/80"
          aria-label={t("common.close")}
        >
          ×
        </Button>
      )}
    </div>
  );
}
