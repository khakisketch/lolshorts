import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

type TauriRuntimeWindow = Window & {
  __TAURI__?: unknown;
  __TAURI_INTERNALS__?: unknown;
};

// Environment detection helper - works in both Vite and Jest
const isDev = (): boolean => {
  try {
    // Vite environment
    return import.meta.env?.DEV ?? false;
  } catch {
    // Jest/Node environment
    return process.env.NODE_ENV !== 'production';
  }
};

const hasTauriRuntime = (): boolean => {
  if (typeof window === 'undefined') {
    return false;
  }

  const tauriWindow = window as TauriRuntimeWindow;
  return Boolean(tauriWindow.__TAURI__ || tauriWindow.__TAURI_INTERNALS__);
};

export interface AppErrorResponse {
  code: string;
  message: string;
}

export class AppError extends Error {
  code: string;

  constructor(response: AppErrorResponse) {
    super(response.message);
    this.code = response.code;
    this.name = 'AppError';
  }
}

/**
 * Helper to listen for Tauri events.
 * Returns an unlisten function to clean up the listener.
 */
export async function listenToEvent<T>(
  eventName: string,
  callback: (payload: T) => void
): Promise<UnlistenFn> {
  if (!hasTauriRuntime()) {
    console.warn(`Tauri runtime unavailable. Cannot listen to event: ${eventName}`);
    return () => {};
  }
  return await listen<T>(eventName, (event) => {
    callback(event.payload);
  });
}

/**
 * Generic wrapper for Tauri invoke calls.
 * Handles structured error responses from the Rust backend.
 */
export async function cmd<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!hasTauriRuntime()) {
    throw new AppError({
      code: 'TAURI_UNAVAILABLE',
      message: 'Desktop runtime unavailable. Please run this action inside the LoLShorts desktop app.',
    });
  }

  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    // Only log detailed errors in development
    if (isDev()) {
      console.error(`Command '${command}' failed:`, error);
    }

    // Check if error is our structured AppErrorResponse
    if (typeof error === 'object' && error !== null && 'code' in error && 'message' in error) {
      throw new AppError(error as AppErrorResponse);
    }

    // Handle string errors (legacy or unhandled Rust errors)
    if (typeof error === 'string') {
        // Try to parse if it's a JSON string
        try {
            const parsed = JSON.parse(error);
            if (typeof parsed === 'object' && parsed !== null && 'code' in parsed) {
                throw new AppError(parsed as AppErrorResponse);
            }
        } catch {
            // Not JSON, treat as plain message
        }
        throw new AppError({ code: 'UNKNOWN_ERROR', message: error });
    }

    throw new AppError({ code: 'INTERNAL_ERROR', message: 'An unexpected error occurred' });
  }
}
