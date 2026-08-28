import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Safely extract error message from unknown error type
 * Handles Error objects, strings, and unknown types
 * @param err - Unknown error value
 * @returns Human-readable error message string
 */
export function getErrorMessage(err: unknown): string {
  if (err instanceof Error) {
    return err.message;
  }
  if (typeof err === "string") {
    return err;
  }
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return String(err);
}

/**
 * Format bytes to human-readable storage size
 * @param bytes - Size in bytes
 * @returns Formatted string (e.g., "1.5 GB", "250 MB")
 */
export function formatStorage(bytes: number): string {
  if (bytes === 0) return "0 B";

  const units = ["B", "KB", "MB", "GB", "TB"];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${units[i]}`;
}

/**
 * Format seconds to MM:SS or HH:MM:SS format
 * @param seconds - Duration in seconds
 * @returns Formatted duration string
 */
export function formatDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);

  if (hours > 0) {
    return `${hours}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  }
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

/**
 * Standardized page styles for consistent UI across all pages
 */
export const pageStyles = {
  /** Page title: responsive sizing with consistent weight */
  title: "text-2xl md:text-3xl font-bold mb-4 md:mb-6",
  /** Page subtitle/description */
  subtitle: "text-sm text-muted-foreground",
  /** Section title within a page */
  sectionTitle: "text-lg font-semibold",
  /** Page container with consistent padding */
  container: "space-y-6",
} as const;
