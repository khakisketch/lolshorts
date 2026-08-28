/**
 * Video Modal Component
 *
 * Modal dialog for video playback with proper focus management
 */

import { useState, useEffect, useRef } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogTitle,
} from "@/components/ui/dialog";
import { VideoPlayer } from "./VideoPlayer";

interface VideoModalProps {
  isOpen: boolean;
  onClose: () => void;
  src: string;
  title?: string;
  autoPlay?: boolean;
}

export function VideoModal({
  isOpen,
  onClose,
  src,
  title,
  autoPlay = false,
}: VideoModalProps) {
  const [focusElement, setFocusElement] = useState<HTMLElement | null>(null);
  const overlayRef = useRef<HTMLDivElement>(null);

  // Store the element that had focus before opening the modal
  useEffect(() => {
    if (isOpen && document.activeElement instanceof HTMLElement) {
      setFocusElement(document.activeElement);
    }
  }, [isOpen]);

  // Restore focus when closing
  useEffect(() => {
    if (!isOpen && focusElement) {
      focusElement.focus();
    }
  }, [isOpen, focusElement]);

  // Handle overlay click
  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === overlayRef.current) {
      onClose();
    }
  };

  // Handle keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!isOpen) return;

      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogOverlay
        ref={overlayRef}
        onClick={handleOverlayClick}
        className="fixed inset-0 bg-black/90 backdrop-blur-sm z-50"
      />
      <DialogContent className="h-[calc(100vh-2rem)] w-[calc(100vw-2rem)] max-w-none overflow-hidden rounded-lg bg-background p-0 shadow-2xl outline-none">
        <DialogTitle className="sr-only">{title || "Video Player"}</DialogTitle>
        <DialogDescription className="sr-only">
          Video playback dialog with controls
        </DialogDescription>
        <div className="flex h-full min-h-0 flex-col">
          <VideoPlayer
            src={src}
            title={title}
            autoPlay={autoPlay}
            onClose={onClose}
            className="min-h-0 flex-1"
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}
