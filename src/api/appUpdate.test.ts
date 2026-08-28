import { listenToEvent, cmd } from "./client";
import { APP_UPDATE_PROGRESS_EVENT, appUpdateApi } from "./appUpdate";

jest.mock("./client", () => ({
  cmd: jest.fn().mockResolvedValue(undefined),
  listenToEvent: jest.fn().mockResolvedValue(jest.fn()),
}));

describe("app update API contract", () => {
  it("uses the stable update commands and progress event", async () => {
    await appUpdateApi.getStatus();
    await appUpdateApi.check();
    await appUpdateApi.install();
    const callback = jest.fn();
    await appUpdateApi.listen(callback);

    expect((cmd as jest.Mock).mock.calls).toEqual([
      ["get_app_update_status"],
      ["check_app_update"],
      ["install_app_update"],
    ]);
    expect(listenToEvent).toHaveBeenCalledWith(
      APP_UPDATE_PROGRESS_EVENT,
      callback,
    );
  });
});
