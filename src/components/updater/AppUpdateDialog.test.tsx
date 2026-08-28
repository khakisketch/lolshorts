import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AppUpdateDialog } from "./AppUpdateDialog";

const initialize = jest.fn().mockResolvedValue(() => {});
const check = jest.fn().mockResolvedValue(undefined);
const install = jest.fn().mockResolvedValue(undefined);
let snapshot = {
  status: "available" as const,
  current_version: "1.2.0",
  available_version: "1.3.0",
  notes: "Keyboard and updater fixes",
  published_at: "2026-08-10T00:00:00Z",
  progress_percentage: 0,
  error_code: null,
};

jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

jest.mock("@/stores/appUpdateStore", () => ({
  useAppUpdateStore: () => ({ snapshot, initialize, check, install }),
}));

describe("AppUpdateDialog", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    snapshot = {
      status: "available",
      current_version: "1.2.0",
      available_version: "1.3.0",
      notes: "Keyboard and updater fixes",
      published_at: "2026-08-10T00:00:00Z",
      progress_percentage: 0,
      error_code: null,
    };
  });

  it("shows release notes and starts installation from the keyboard-accessible action", () => {
    render(<AppUpdateDialog />);

    expect(screen.getByRole("dialog")).toBeVisible();
    expect(screen.getByText("Keyboard and updater fixes")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "appUpdater.install" }));
    expect(install).toHaveBeenCalledTimes(1);
  });

  it("defers without forcing installation and closes on Escape", async () => {
    render(<AppUpdateDialog />);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(install).not.toHaveBeenCalled();
  });
});
