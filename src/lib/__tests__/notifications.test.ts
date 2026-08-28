/**
 * Notifications Unit Tests
 *
 * Tests for notify, notifyGameStarted, notifyGameEnded,
 * notifyClipSaved, and notifyUploadComplete.
 *
 * @tauri-apps/plugin-notification is mocked because it is only
 * available at runtime inside Tauri.
 */

const mockIsPermissionGranted = jest.fn();
const mockRequestPermission = jest.fn();
const mockSendNotification = jest.fn();

jest.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: mockIsPermissionGranted,
  requestPermission: mockRequestPermission,
  sendNotification: mockSendNotification,
}));

// Re-import the module fresh for each test so the module-level
// permission cache (permissionChecked / hasPermission) is reset.
beforeEach(() => {
  jest.resetModules();
  mockIsPermissionGranted.mockReset();
  mockRequestPermission.mockReset();
  mockSendNotification.mockReset();
});

async function getNotifications() {
  return import("../notifications");
}

describe("notify", () => {
  it("sends a notification when permission is already granted", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(true);
    const { notify } = await getNotifications();

    await notify("Title", "Body");

    expect(mockSendNotification).toHaveBeenCalledWith({
      title: "Title",
      body: "Body",
    });
  });

  it("requests permission and sends notification when not initially granted", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(false);
    mockRequestPermission.mockResolvedValueOnce("granted");
    const { notify } = await getNotifications();

    await notify("Title", "Body");

    expect(mockRequestPermission).toHaveBeenCalled();
    expect(mockSendNotification).toHaveBeenCalledWith({
      title: "Title",
      body: "Body",
    });
  });

  it("does not send notification when permission is denied", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(false);
    mockRequestPermission.mockResolvedValueOnce("denied");
    const { notify } = await getNotifications();

    await notify("Title", "Body");

    expect(mockSendNotification).not.toHaveBeenCalled();
  });

  it("does not throw when sendNotification throws — silently ignores errors", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(true);
    mockSendNotification.mockImplementationOnce(() => {
      throw new Error("notification failed");
    });
    const { notify } = await getNotifications();

    await expect(notify("Title", "Body")).resolves.toBeUndefined();
  });

  it("does not throw when isPermissionGranted throws", async () => {
    mockIsPermissionGranted.mockRejectedValueOnce(
      new Error("Tauri unavailable"),
    );
    const { notify } = await getNotifications();

    await expect(notify("Title", "Body")).resolves.toBeUndefined();
    expect(mockSendNotification).not.toHaveBeenCalled();
  });
});

describe("notifyGameStarted", () => {
  it("sends notification with champion name in body when champion is provided", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(true);
    const { notifyGameStarted } = await getNotifications();

    await notifyGameStarted("Ahri");

    expect(mockSendNotification).toHaveBeenCalledWith(
      expect.objectContaining({
        title: "LoLShorts",
        body: expect.stringContaining("Ahri"),
      }),
    );
  });

  it("sends notification without champion name when champion is omitted", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(true);
    const { notifyGameStarted } = await getNotifications();

    await notifyGameStarted();

    expect(mockSendNotification).toHaveBeenCalledWith(
      expect.objectContaining({ title: "LoLShorts" }),
    );
  });
});

describe("notifyGameEnded", () => {
  it("includes clip count in notification body", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(true);
    const { notifyGameEnded } = await getNotifications();

    await notifyGameEnded(5);

    expect(mockSendNotification).toHaveBeenCalledWith(
      expect.objectContaining({ body: expect.stringContaining("5") }),
    );
  });

  it("sends notification with title LoLShorts", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(true);
    const { notifyGameEnded } = await getNotifications();

    await notifyGameEnded(0);

    expect(mockSendNotification).toHaveBeenCalledWith(
      expect.objectContaining({ title: "LoLShorts" }),
    );
  });
});

describe("notifyClipSaved", () => {
  it("includes clip name in the notification body", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(true);
    const { notifyClipSaved } = await getNotifications();

    await notifyClipSaved("my-clip.mp4");

    expect(mockSendNotification).toHaveBeenCalledWith(
      expect.objectContaining({ body: expect.stringContaining("my-clip.mp4") }),
    );
  });
});

describe("notifyUploadComplete", () => {
  it("includes video title in the notification body", async () => {
    mockIsPermissionGranted.mockResolvedValueOnce(true);
    const { notifyUploadComplete } = await getNotifications();

    await notifyUploadComplete("My Montage");

    expect(mockSendNotification).toHaveBeenCalledWith(
      expect.objectContaining({ body: expect.stringContaining("My Montage") }),
    );
  });
});
