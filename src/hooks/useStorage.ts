import { useState, useCallback } from 'react';
import { storageApi } from '@/api/storage';
import { GameMetadata, EventData, ClipMetadata, StorageStats } from '@/types/storage';

export function useStorage() {
  const [loadingCount, setLoadingCount] = useState(0);
  const loading = loadingCount > 0;
  const [error, setError] = useState<string | null>(null);

  const clearError = useCallback(() => setError(null), []);

  const listGames = useCallback(async (): Promise<string[]> => {
    setLoadingCount(c => c + 1);
    try {
      const gameIds = await storageApi.listGames();
      return gameIds;
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw err;
    } finally {
      setLoadingCount(c => c - 1);
    }
  }, []);

  const getGameMetadata = useCallback(async (gameId: string): Promise<GameMetadata> => {
    setLoadingCount(c => c + 1);
    try {
      const metadata = await storageApi.getGameMetadata(gameId);
      return metadata;
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw err;
    } finally {
      setLoadingCount(c => c - 1);
    }
  }, []);

  const getAllGames = useCallback(async (): Promise<GameMetadata[]> => {
    setLoadingCount(c => c + 1);
    try {
      const gameIds = await storageApi.listGames();
      // Use Promise.allSettled to handle partial failures gracefully
      const results = await Promise.allSettled(
        gameIds.map((id) => storageApi.getGameMetadata(id))
      );
      // Filter only successful results
      const detailedGames = results
        .filter((result): result is PromiseFulfilledResult<GameMetadata> =>
          result.status === 'fulfilled'
        )
        .map((result) => result.value);
      return detailedGames;
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw err;
    } finally {
      setLoadingCount(c => c - 1);
    }
  }, []);

  const saveGameMetadata = useCallback(async (gameId: string, metadata: GameMetadata): Promise<void> => {
    setLoadingCount(c => c + 1);
    try {
      await storageApi.saveGameMetadata(gameId, metadata);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw err;
    } finally {
      setLoadingCount(c => c - 1);
    }
  }, []);

  const getGameEvents = useCallback(async (gameId: string): Promise<EventData[]> => {
    setLoadingCount(c => c + 1);
    try {
      const events = await storageApi.getGameEvents(gameId);
      return events;
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw err;
    } finally {
      setLoadingCount(c => c - 1);
    }
  }, []);

  const saveGameEvents = useCallback(async (gameId: string, events: EventData[]): Promise<void> => {
    setLoadingCount(c => c + 1);
    try {
      await storageApi.saveGameEvents(gameId, events);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw err;
    } finally {
      setLoadingCount(c => c - 1);
    }
  }, []);

  const saveClipMetadata = useCallback(async (gameId: string, clip: ClipMetadata): Promise<void> => {
    setLoadingCount(c => c + 1);
    try {
      await storageApi.saveClipMetadata(gameId, clip);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw err;
    } finally {
      setLoadingCount(c => c - 1);
    }
  }, []);

  const deleteGame = useCallback(async (gameId: string): Promise<void> => {
    setLoadingCount(c => c + 1);
    try {
      await storageApi.deleteGame(gameId);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw err;
    } finally {
      setLoadingCount(c => c - 1);
    }
  }, []);

  const getStorageStats = useCallback(async (): Promise<StorageStats> => {
    setLoadingCount(c => c + 1);
    try {
      const stats = await storageApi.getStorageStats();
      return stats;
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
      throw err;
    } finally {
      setLoadingCount(c => c - 1);
    }
  }, []);

  return {
    isLoading: loading,
    error,
    clearError,
    listGames,
    getAllGames,
    getGameMetadata,
    saveGameMetadata,
    getGameEvents,
    saveGameEvents,
    saveClipMetadata,
    deleteGame,
    getStorageStats,
  };
}