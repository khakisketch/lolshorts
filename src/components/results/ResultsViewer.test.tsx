import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ResultsViewer } from "./ResultsViewer";

const mockNavigate = jest.fn();
const mockGetAutoEditResultGroups = jest.fn();

jest.mock("@tanstack/react-router", () => ({
  useNavigate: () => mockNavigate,
}));

jest.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://${path}`,
}));

jest.mock("@/hooks/useAutoEditResults", () => ({
  useAutoEditResults: () => ({
    deleteResult: jest.fn(),
    isLoading: false,
    error: null,
  }),
}));

jest.mock("@/api/storage", () => ({
  storageApi: {
    getAutoEditResultGroups: (...args: unknown[]) =>
      mockGetAutoEditResultGroups(...args),
    deleteAutoEditResultGroup: jest.fn(),
  },
}));

jest.mock("@/api/utils", () => ({
  utilsApi: {
    openFileWithDefaultApp: jest.fn(),
    showInFolder: jest.fn(),
  },
}));

jest.mock("@/api/video", () => ({
  videoApi: {
    revalidateAutoEditResult: jest.fn(),
  },
}));

jest.mock("@/stores/editorStore", () => ({
  useEditorStore: (selector: (state: unknown) => unknown) =>
    selector({ setSelectedGameId: jest.fn() }),
}));

jest.mock("@/components/ui/confirm-dialog", () => ({
  useConfirmDialog: () => ({
    confirm: jest.fn(),
    ConfirmDialog: () => null,
  }),
}));

jest.mock("./ShareDialog", () => ({
  ShareDialog: () => null,
}));

describe("ResultsViewer", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("shows an actionable load error and recovers after retry", async () => {
    mockGetAutoEditResultGroups
      .mockRejectedValueOnce(new Error("storage unavailable"))
      .mockResolvedValueOnce([]);

    render(<ResultsViewer />);

    expect(await screen.findByTestId("results-load-error")).toHaveTextContent(
      "results.highlights.loadError",
    );
    expect(screen.queryByTestId("results-empty")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "common.retry" }));

    await waitFor(() =>
      expect(mockGetAutoEditResultGroups).toHaveBeenCalledTimes(2),
    );
    expect(await screen.findByTestId("results-empty")).toBeInTheDocument();
    expect(screen.queryByTestId("results-load-error")).not.toBeInTheDocument();
  });
});
