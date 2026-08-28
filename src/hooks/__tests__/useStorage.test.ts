/**
 * useStorage Hook Unit Tests
 *
 * Tests for loading state management, error propagation, and
 * delegation to the storageApi for each operation.
 *
 * storageApi is built on top of Tauri cmd() which is already mocked
 * via jest.setup.js -> './src/api/client'. We mock storageApi directly
 * to keep tests isolated from Tauri command routing details.
 */

import { renderHook, act } from "@testing-library/react";
import { useStorage } from "../useStorage";

const mockListGames = jest.fn();
const mockGetGameMetadata = jest.fn();
const mockSaveGameMetadata = jest.fn();
const mockGetGameEvents = jest.fn();
const mockSaveGameEvents = jest.fn();
const mockSaveClipMetadata = jest.fn();
const mockDeleteGame = jest.fn();
const mockGetStorageStats = jest.fn();

jest.mock("@/api/storage", () => ({
  storageApi: {
    listGames: (...args: unknown[]) => mockListGames(...args),
    getGameMetadata: (...args: unknown[]) => mockGetGameMetadata(...args),
    saveGameMetadata: (...args: unknown[]) => mockSaveGameMetadata(...args),
    getGameEvents: (...args: unknown[]) => mockGetGameEvents(...args),
    saveGameEvents: (...args: unknown[]) => mockSaveGameEvents(...args),
    saveClipMetadata: (...args: unknown[]) => mockSaveClipMetadata(...args),
    deleteGame: (...args: unknown[]) => mockDeleteGame(...args),
    getStorageStats: (...args: unknown[]) => mockGetStorageStats(...args),
  },
}));

beforeEach(() => {
  jest.clearAllMocks();
});

describe("useStorage - initial state", () => {
  it("starts with isLoading false and error null", () => {
    const { result } = renderHook(() => useStorage());
    expect(result.current.isLoading).toBe(false);
    expect(result.current.error).toBeNull();
  });
});

describe("useStorage - listGames", () => {
  it("returns game objects mapped from game IDs on success", async () => {
    mockListGames.mockResolvedValueOnce(["game-1", "game-2"]);
    const { result } = renderHook(() => useStorage());

    let games: unknown;
    await act(async () => {
      games = await result.current.listGames();
    });

    expect(games).toEqual(["game-1", "game-2"]);
  });

  it("sets error and rethrows when listGames fails", async () => {
    mockListGames.mockRejectedValueOnce(new Error("disk read error"));
    const { result } = renderHook(() => useStorage());

    await act(async () => {
      await expect(result.current.listGames()).rejects.toThrow(
        "disk read error",
      );
    });

    expect(result.current.error).toBe("disk read error");
    expect(result.current.isLoading).toBe(false);
  });

  it("resets loading to false after successful call", async () => {
    mockListGames.mockResolvedValueOnce([]);
    const { result } = renderHook(() => useStorage());

    await act(async () => {
      await result.current.listGames();
    });

    expect(result.current.isLoading).toBe(false);
  });

  it("clears a stale error once a retried call succeeds", async () => {
    mockListGames.mockRejectedValueOnce(new Error("network error"));
    const { result } = renderHook(() => useStorage());

    await act(async () => {
      await expect(result.current.listGames()).rejects.toThrow("network error");
    });
    expect(result.current.error).toBe("network error");

    mockListGames.mockResolvedValueOnce(["game-1"]);
    await act(async () => {
      await result.current.listGames();
    });

    expect(result.current.error).toBeNull();
  });
});

describe("useStorage - getGameMetadata", () => {
  it("returns metadata for the given game ID", async () => {
    const meta = {
      game_id: "g1",
      game_start_time: "2024-01-01",
      game_end_time: null,
      champion_name: "Ahri",
      game_mode: "CLASSIC",
    };
    mockGetGameMetadata.mockResolvedValueOnce(meta);
    const { result } = renderHook(() => useStorage());

    let returned: unknown;
    await act(async () => {
      returned = await result.current.getGameMetadata("g1");
    });

    expect(mockGetGameMetadata).toHaveBeenCalledWith("g1");
    expect(returned).toEqual(meta);
  });

  it("sets error and rethrows when getGameMetadata fails", async () => {
    mockGetGameMetadata.mockRejectedValueOnce(new Error("not found"));
    const { result } = renderHook(() => useStorage());

    await act(async () => {
      await expect(result.current.getGameMetadata("g1")).rejects.toThrow(
        "not found",
      );
    });

    expect(result.current.error).toBe("not found");
  });
});

describe("useStorage - getAllGames", () => {
  it("returns only fulfilled game metadata, skipping failures", async () => {
    mockListGames.mockResolvedValueOnce(["ok-id", "fail-id"]);
    mockGetGameMetadata
      .mockResolvedValueOnce({ game_id: "ok-id" })
      .mockRejectedValueOnce(new Error("corrupt"));

    const { result } = renderHook(() => useStorage());

    let all: unknown;
    await act(async () => {
      all = await result.current.getAllGames();
    });

    expect(all).toEqual([{ game_id: "ok-id" }]);
  });

  it("returns empty array when all metadata fetches fail", async () => {
    mockListGames.mockResolvedValueOnce(["a", "b"]);
    mockGetGameMetadata.mockRejectedValue(new Error("bad"));

    const { result } = renderHook(() => useStorage());

    let all: unknown;
    await act(async () => {
      all = await result.current.getAllGames();
    });

    expect(all).toEqual([]);
  });
});

describe("useStorage - saveGameMetadata", () => {
  it("calls storageApi.saveGameMetadata with correct arguments", async () => {
    mockSaveGameMetadata.mockResolvedValueOnce(undefined);
    const meta = {
      game_id: "g1",
      game_start_time: "2024-01-01",
      game_end_time: null,
      champion_name: null,
      game_mode: null,
    };
    const { result } = renderHook(() => useStorage());

    await act(async () => {
      await result.current.saveGameMetadata("g1", meta);
    });

    expect(mockSaveGameMetadata).toHaveBeenCalledWith("g1", meta);
    expect(result.current.error).toBeNull();
  });
});

describe("useStorage - deleteGame", () => {
  it("calls storageApi.deleteGame with the game ID", async () => {
    mockDeleteGame.mockResolvedValueOnce(undefined);
    const { result } = renderHook(() => useStorage());

    await act(async () => {
      await result.current.deleteGame("game-xyz");
    });

    expect(mockDeleteGame).toHaveBeenCalledWith("game-xyz");
  });

  it("sets error string when deleteGame fails with a non-Error value", async () => {
    mockDeleteGame.mockRejectedValueOnce("string-error");
    const { result } = renderHook(() => useStorage());

    await act(async () => {
      await expect(result.current.deleteGame("game-xyz")).rejects.toBe(
        "string-error",
      );
    });

    expect(result.current.error).toBe("string-error");
  });
});

describe("useStorage - getStorageStats", () => {
  it("returns storage stats from the API", async () => {
    const stats = { total_size_bytes: 1024, game_count: 3, clip_count: 12 };
    mockGetStorageStats.mockResolvedValueOnce(stats);
    const { result } = renderHook(() => useStorage());

    let returned: unknown;
    await act(async () => {
      returned = await result.current.getStorageStats();
    });

    expect(returned).toEqual(stats);
  });

  it("sets error when getStorageStats fails", async () => {
    mockGetStorageStats.mockRejectedValueOnce(new Error("storage unavailable"));
    const { result } = renderHook(() => useStorage());

    await act(async () => {
      await expect(result.current.getStorageStats()).rejects.toThrow(
        "storage unavailable",
      );
    });

    expect(result.current.error).toBe("storage unavailable");
  });
});
