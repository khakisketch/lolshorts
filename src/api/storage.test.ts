import { cmd } from "./client";
import { storageApi } from "./storage";

jest.mock("./client", () => ({
  cmd: jest.fn().mockResolvedValue(undefined),
}));

const mockedCmd = cmd as jest.MockedFunction<typeof cmd>;

describe("storage API contract", () => {
  beforeEach(() => {
    mockedCmd.mockClear();
  });

  it("maps library, clip, and auto-edit operations to backend commands", async () => {
    await storageApi.listGames();
    await storageApi.getGameMetadata("game-1");
    await storageApi.getGameEvents("game-1");
    await storageApi.getStorageStats();
    await storageApi.listClips("game-1");
    await storageApi.listClipVaultPage({
      sort: "newest",
      cursor: null,
      game_limit: 20,
    });
    await storageApi.ensureClipThumbnail("game-1", "clip.mp4");
    await storageApi.deleteClip("game-1", "clip.mp4");
    await storageApi.saveGameMetadata("game-1", {} as never);
    await storageApi.saveGameEvents("game-1", []);
    await storageApi.saveClipMetadata("game-1", {} as never);
    await storageApi.deleteGame("game-1");
    await storageApi.getAutoEditResults();
    await storageApi.getAutoEditResultGroups();
    await storageApi.getAutoEditResult("result-1");
    await storageApi.deleteAutoEditResult("result-1", true);
    await storageApi.deleteAutoEditResultGroup("series-1", false);
    await storageApi.updateAutoEditYoutubeStatus("result-1", "uploaded");

    expect(mockedCmd.mock.calls).toEqual([
      ["list_games"],
      ["get_game_metadata", { game_id: "game-1" }],
      ["get_game_events", { game_id: "game-1" }],
      ["get_storage_stats"],
      ["list_clips", { game_id: "game-1" }],
      [
        "list_clip_vault_page",
        { input: { sort: "newest", cursor: null, game_limit: 20 } },
      ],
      [
        "ensure_clip_thumbnail",
        { game_id: "game-1", clip_file_path: "clip.mp4" },
      ],
      ["delete_clip", { clip_file_path: "clip.mp4", game_id: "game-1" }],
      ["save_game_metadata", { game_id: "game-1", metadata: {} }],
      ["save_game_events", { game_id: "game-1", events: [] }],
      ["save_clip_metadata", { game_id: "game-1", clip: {} }],
      ["delete_game", { game_id: "game-1" }],
      ["get_auto_edit_results"],
      ["get_auto_edit_result_groups"],
      ["get_auto_edit_result", { result_id: "result-1" }],
      ["delete_auto_edit_result", { result_id: "result-1", delete_file: true }],
      [
        "delete_auto_edit_result_group",
        { series_id: "series-1", delete_files: false },
      ],
      [
        "update_auto_edit_youtube_status",
        { result_id: "result-1", status: "uploaded" },
      ],
    ]);
  });

  it("serializes clip-vault search and mode filters without changing the base contract", async () => {
    await storageApi.listClipVaultPage({
      sort: "best",
      cursor: "cursor-1",
      game_limit: 6,
      query: "Ahri",
      game_mode: "CLASSIC",
    });

    expect(mockedCmd).toHaveBeenCalledWith("list_clip_vault_page", {
      input: {
        sort: "best",
        cursor: "cursor-1",
        game_limit: 6,
        query: "Ahri",
        game_mode: "CLASSIC",
      },
    });
  });
});
