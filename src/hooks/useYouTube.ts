import { useState, useCallback, useEffect, useRef } from "react";
import { youtubeApi, UploadHistoryEntry } from "@/api/youtube";
import { getErrorMessage } from "@/lib/utils";
import {
  AuthStatus,
  QuotaInfo,
  UploadProgress,
  YouTubeVideo,
  ScheduledUpload,
} from "@/types/youtube";
import { listenToEvent } from "@/api/client";

export interface UploadMetadata {
  title: string;
  description: string;
  tags?: string[];
  privacy_status: string;
}

export function useYouTube() {
  const [authStatus, setAuthStatus] = useState<AuthStatus>({
    authenticated: false,
    expires_at: null,
    has_refresh_token: false,
  });
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [queueError, setQueueError] = useState<string | null>(null);
  const [uploadHistory, setUploadHistory] = useState<UploadHistoryEntry[]>([]);
  const [uploadQueue, setUploadQueue] = useState<ScheduledUpload[]>([]);
  const [uploadProgress, setUploadProgress] = useState<UploadProgress | null>(
    null,
  );
  const [authEventError, setAuthEventError] = useState<string | null>(null);

  const pollingInterval = useRef<NodeJS.Timeout | null>(null);

  const checkAuthStatus = useCallback(async () => {
    try {
      const status = await youtubeApi.getAuthStatus();
      setAuthStatus(status);
    } catch {
      // Auth status check failure is non-critical - user not logged in
    }
  }, []);

  const loadHistory = useCallback(async () => {
    try {
      const history = await youtubeApi.getUploadHistory();
      setUploadHistory(history);
    } catch {
      // History load failure is non-critical - will show empty list
    }
  }, []);

  const loadQueue = useCallback(async () => {
    try {
      setQueueError(null);
      const queue = await youtubeApi.getUploadQueue();
      setUploadQueue(queue);
    } catch (err) {
      setQueueError(getErrorMessage(err));
    }
  }, []);

  const getQuotaInfo = useCallback(async (): Promise<QuotaInfo> => {
    setError(null);
    try {
      const quota = await youtubeApi.getQuotaInfo();
      return quota;
    } catch (err) {
      setError(getErrorMessage(err));
      throw err;
    }
  }, []);

  useEffect(() => {
    checkAuthStatus();
    loadHistory();
    loadQueue();
  }, [checkAuthStatus, loadHistory, loadQueue]);

  useEffect(() => {
    let unlistenCompleted: (() => void) | null = null;
    let unlistenFailed: (() => void) | null = null;
    let unlistenAuthCompleted: (() => void) | null = null;
    let unlistenAuthFailed: (() => void) | null = null;

    const setupListeners = async () => {
      unlistenCompleted = await listenToEvent<unknown>(
        "scheduled-upload-completed",
        async () => {
          await loadQueue();
          await loadHistory();
          await getQuotaInfo();
        },
      );
      unlistenFailed = await listenToEvent<unknown>(
        "scheduled-upload-failed",
        async () => {
          await loadQueue();
          await loadHistory();
          await getQuotaInfo();
        },
      );
      // youtube_start_auth_with_server's background OAuth callback task emits
      // these once the browser flow finishes (no payload on success).
      unlistenAuthCompleted = await listenToEvent<unknown>(
        "youtube-auth-completed",
        async () => {
          setAuthEventError(null);
          await checkAuthStatus();
        },
      );
      unlistenAuthFailed = await listenToEvent<{ error: string }>(
        "youtube-auth-failed",
        (payload) => {
          setAuthEventError(payload?.error || "YouTube authentication failed.");
        },
      );
    };

    setupListeners();

    return () => {
      if (unlistenCompleted) unlistenCompleted();
      if (unlistenFailed) unlistenFailed();
      if (unlistenAuthCompleted) unlistenAuthCompleted();
      if (unlistenAuthFailed) unlistenAuthFailed();
    };
  }, [loadQueue, loadHistory, getQuotaInfo, checkAuthStatus]);

  const startAuthWithServer = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const authUrl = await youtubeApi.startAuthWithServer();
      return authUrl;
    } catch (err) {
      setError(getErrorMessage(err));
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const completeAuth = useCallback(
    async (code: string, state: string) => {
      setIsLoading(true);
      setError(null);
      try {
        await youtubeApi.completeAuth(code, state);
        await checkAuthStatus();
        await loadHistory();
      } catch (err) {
        setError(getErrorMessage(err));
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [checkAuthStatus, loadHistory],
  );

  const logout = useCallback(async () => {
    setIsLoading(true);
    try {
      await youtubeApi.logout();
      setAuthStatus({
        authenticated: false,
        expires_at: null,
        has_refresh_token: false,
      });
      setUploadHistory([]);
    } catch (err) {
      setError(getErrorMessage(err));
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const uploadVideo = useCallback(
    async (
      filePath: string,
      metadata: UploadMetadata,
      thumbnailPath?: string,
    ): Promise<YouTubeVideo> => {
      setIsLoading(true);
      setError(null);
      try {
        const result = await youtubeApi.uploadVideo(
          filePath,
          metadata.title,
          metadata.description,
          metadata.tags ?? [],
          metadata.privacy_status,
          thumbnailPath,
        );

        // Reload history after successful upload
        await loadHistory();

        return result;
      } catch (err) {
        setError(getErrorMessage(err));
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [loadHistory],
  );

  const getUploadHistory = useCallback(async () => {
    await loadHistory();
    return uploadHistory;
  }, [loadHistory, uploadHistory]);

  const pollUploadProgress = useCallback(async () => {
    try {
      const progress = await youtubeApi.getUploadProgress();
      setUploadProgress(progress);
    } catch {
      // Progress polling failure is non-critical - will retry next interval
    }
  }, []);

  const startProgressPolling = useCallback(() => {
    if (pollingInterval.current) return;
    pollingInterval.current = setInterval(() => {
      pollUploadProgress();
    }, 1000);
  }, [pollUploadProgress]);

  const stopProgressPolling = useCallback(() => {
    if (pollingInterval.current) {
      clearInterval(pollingInterval.current);
      pollingInterval.current = null;
    }
    setUploadProgress(null);
  }, []);

  const addToHistory = useCallback(
    async (video: YouTubeVideo) => {
      try {
        await youtubeApi.addToHistory(video);
        await loadHistory();
      } catch {
        // History addition failure is non-critical
      }
    },
    [loadHistory],
  );

  useEffect(() => {
    return () => stopProgressPolling();
  }, [stopProgressPolling]);

  return {
    authStatus,
    isAuthenticated: authStatus.authenticated,
    isLoading,
    error,
    queueError,
    uploadHistory,
    uploadQueue,
    uploadProgress,
    startAuth: startAuthWithServer,
    startAuthWithServer,
    completeAuth,
    logout,
    uploadVideo,
    checkAuthStatus,
    addToHistory,
    getQuotaInfo,
    getUploadHistory,
    loadQueue,
    startProgressPolling,
    stopProgressPolling,
    /** Error from the 'youtube-auth-failed' background event, if any. */
    authEventError,
    clearAuthEventError: () => setAuthEventError(null),
  };
}
