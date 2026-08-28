import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import "@testing-library/jest-dom";
import { ExportModal } from "./ExportModal";
import { useEditorStore } from "@/stores/editorStore";

// react-i18next is globally mocked in jest.setup.js to return the key as-is,
// so button/label text below is the raw translation key.

jest.mock("@/hooks/useEditor", () => ({
  useEditor: () => ({
    composeShorts: jest.fn(),
    createMontage: jest.fn(),
    isLoading: false,
  }),
}));

jest.mock("@/components/ui/use-toast", () => ({
  useToast: () => ({ toast: jest.fn() }),
}));

jest.mock("@/components/ui/confirm-dialog", () => ({
  useConfirmDialog: () => ({
    confirm: jest.fn().mockResolvedValue(true),
    ConfirmDialog: () => null,
  }),
}));

jest.mock("@tauri-apps/plugin-shell", () => ({
  open: jest.fn(),
}));

jest.mock("@tauri-apps/api/core", () => ({
  invoke: jest.fn(),
}));

const mockSave = jest.fn();
jest.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => mockSave(...args),
}));

const mockVideoDir = jest.fn();
const mockDownloadDir = jest.fn();
jest.mock("@tauri-apps/api/path", () => ({
  join: (...parts: string[]) => Promise.resolve(parts.join("/")),
  dirname: jest.fn().mockResolvedValue("C:/some/dir"),
  videoDir: (...args: unknown[]) => mockVideoDir(...args),
  downloadDir: (...args: unknown[]) => mockDownloadDir(...args),
}));

// Radix Select needs pointer-capture / scrollIntoView APIs that jsdom
// doesn't implement.
beforeAll(() => {
  window.HTMLElement.prototype.scrollIntoView = jest.fn();
  Object.assign(window.HTMLElement.prototype, {
    hasPointerCapture: jest.fn(() => false),
    setPointerCapture: jest.fn(),
    releasePointerCapture: jest.fn(),
  });
});

const initialEditorState = useEditorStore.getState();

describe("ExportModal - save location selection", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    act(() => {
      useEditorStore.setState({
        ...initialEditorState,
        timelineClips: [
          {
            file_path: "C:/clips/clip1.mp4",
            duration: 5,
            order: 0,
          } as never,
        ],
        totalDuration: 5,
      });
    });
    mockVideoDir.mockResolvedValue("C:/Users/tester/Videos");
    mockDownloadDir.mockResolvedValue("C:/Users/tester/Downloads");
  });

  afterEach(() => {
    act(() => {
      useEditorStore.setState(initialEditorState);
    });
  });

  it("opens the save dialog (not the open dialog) with a videoDir-based default path", async () => {
    mockSave.mockResolvedValue("C:/Users/tester/Videos/my_export.mp4");

    render(<ExportModal isOpen={true} onClose={jest.fn()} />);

    fireEvent.click(screen.getByText("editor.export.selectLocation"));

    await waitFor(() => expect(mockSave).toHaveBeenCalledTimes(1));

    expect(mockVideoDir).toHaveBeenCalledTimes(1);
    expect(mockSave).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: "C:/Users/tester/Videos/lolshorts_export.mp4",
        filters: [{ name: "editor.export.videoFiles", extensions: ["mp4"] }],
      }),
    );

    // The full path returned by save() is stored and surfaced verbatim -
    // it is not treated as a directory to join a filename onto.
    expect(
      await screen.findByTitle("C:/Users/tester/Videos/my_export.mp4"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("editor.export.changeLocation"),
    ).toBeInTheDocument();
  });

  it("falls back to downloadDir() when videoDir() rejects", async () => {
    mockVideoDir.mockRejectedValue(new Error("no video dir"));
    mockSave.mockResolvedValue("C:/Users/tester/Downloads/my_export.mp4");

    render(<ExportModal isOpen={true} onClose={jest.fn()} />);

    fireEvent.click(screen.getByText("editor.export.selectLocation"));

    await waitFor(() => expect(mockSave).toHaveBeenCalledTimes(1));

    expect(mockDownloadDir).toHaveBeenCalledTimes(1);
    expect(mockSave).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: "C:/Users/tester/Downloads/lolshorts_export.mp4",
      }),
    );
  });

  it("keeps the selection unset when the user cancels the save dialog", async () => {
    mockSave.mockResolvedValue(null);

    render(<ExportModal isOpen={true} onClose={jest.fn()} />);

    fireEvent.click(screen.getByText("editor.export.selectLocation"));

    await waitFor(() => expect(mockSave).toHaveBeenCalledTimes(1));

    expect(
      screen.getByText("editor.export.selectLocation"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("editor.export.changeLocation"),
    ).not.toBeInTheDocument();
  });

  it('does not surface the NoParent dirname("") crash anymore (regression guard)', async () => {
    // Historically defaultPath was built from `await dirname('')`, which
    // always rejects with a NoParent error and aborted the whole flow
    // before the dialog even opened. Guard against that regressing.
    mockSave.mockResolvedValue("C:/Users/tester/Videos/lolshorts_export.mp4");

    render(<ExportModal isOpen={true} onClose={jest.fn()} />);

    fireEvent.click(screen.getByText("editor.export.selectLocation"));

    await waitFor(() => expect(mockSave).toHaveBeenCalledTimes(1));
    expect(
      await screen.findByTitle("C:/Users/tester/Videos/lolshorts_export.mp4"),
    ).toBeInTheDocument();
  });
});
