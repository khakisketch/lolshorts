import type { HTMLAttributes, ReactNode } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { StatusDashboard } from "./StatusDashboard";
import { cmd } from "@/api/client";

type ChildrenProps = HTMLAttributes<HTMLElement> & { children?: ReactNode };
type ProgressProps = HTMLAttributes<HTMLDivElement> & { value?: number };

// Mock i18n
jest.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

// Mock Tauri command wrapper
jest.mock("@/api/client", () => ({
  cmd: jest.fn(),
}));

// Mock lucide-react icons
jest.mock("lucide-react", () => ({
  Activity: () => null,
  AlertTriangle: () => null,
  CheckCircle2: () => null,
  XCircle: () => null,
  Cpu: () => null,
  HardDrive: () => null,
  Radio: () => null,
  Clock: () => null,
  WifiOff: () => null,
  ShieldCheck: () => null,
  Loader2: () => null,
}));

// Mock UI components
jest.mock("@/components/ui/badge", () => ({
  Badge: ({ children, ...props }: ChildrenProps) => (
    <span {...props}>{children}</span>
  ),
}));
jest.mock("@/components/ui/progress", () => ({
  Progress: (props: ProgressProps) => <div role="progressbar" {...props} />,
}));
jest.mock("@/components/ui/alert", () => ({
  Alert: ({ children, ...props }: ChildrenProps) => (
    <div role="alert" {...props}>
      {children}
    </div>
  ),
  AlertTitle: ({ children, ...props }: ChildrenProps) => (
    <h5 {...props}>{children}</h5>
  ),
  AlertDescription: ({ children, ...props }: ChildrenProps) => (
    <div {...props}>{children}</div>
  ),
}));

const mockedCmd = cmd as jest.MockedFunction<typeof cmd>;

const mockCommand = async (command: string): Promise<unknown> => {
  switch (command) {
    case "get_detailed_recording_status":
      return {
        status: "idle",
        is_monitoring: false,
        buffer_duration_secs: 0,
      };
    case "get_diagnostics_status":
      return {
        overall_status: "warning",
        checks: [
          {
            key: "updater_pubkey",
            label: "Updater public key",
            status: "warning",
            message: "TAURI_UPDATER_PUBKEY is not configured.",
            action:
              "Provide TAURI_UPDATER_PUBKEY in CI/release build configuration.",
          },
        ],
      };
    case "export_diagnostics_bundle":
      return {
        output_path:
          "C:\\Users\\tester\\AppData\\Roaming\\LoLShorts\\diagnostics\\diagnostics_test.json",
        redacted: true,
        generated_at: "2026-05-05T00:00:00Z",
        included_logs: 1,
      };
    default:
      return null;
  }
};

const renderDashboard = async () => {
  const result = render(<StatusDashboard />);
  await screen.findByText("Updater public key");
  return result;
};

describe("StatusDashboard", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockedCmd.mockImplementation(mockCommand);
  });

  it("renders recording status section", async () => {
    await renderDashboard();

    expect(
      screen.getByText("statusDashboard.recordingStatus"),
    ).toBeInTheDocument();
    expect(screen.getByText("idle")).toBeInTheDocument();
  });

  it("renders auto capture info", async () => {
    await renderDashboard();

    expect(
      screen.getAllByText(/statusDashboard\.autoCapture/).length,
    ).toBeGreaterThan(0);
  });

  it("renders buffer duration", async () => {
    await renderDashboard();

    expect(
      screen.getAllByText("statusDashboard.bufferDuration").length,
    ).toBeGreaterThan(0);
  });

  it("renders hotkey reference section", async () => {
    await renderDashboard();

    expect(
      screen.getByText("statusDashboard.hotkeyReference"),
    ).toBeInTheDocument();
    expect(screen.getByText("F10")).toBeInTheDocument();
  });

  it("renders without errors", async () => {
    const { container } = await renderDashboard();
    expect(container).toBeInTheDocument();
  });

  it("renders diagnostics updater configuration state", async () => {
    await renderDashboard();

    expect(
      screen.getByText("TAURI_UPDATER_PUBKEY is not configured."),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Provide TAURI_UPDATER_PUBKEY in CI/release build configuration.",
      ),
    ).toBeInTheDocument();
  });

  it("renders Tauri unavailable diagnostics without crashing", async () => {
    mockedCmd.mockRejectedValue({
      code: "TAURI_UNAVAILABLE",
      message:
        "Desktop runtime unavailable. Please run this action inside the LoLShorts desktop app.",
    });

    render(<StatusDashboard />);

    await waitFor(() => {
      expect(
        screen.getByText(
          "Desktop runtime unavailable. Please run this action inside the LoLShorts desktop app.",
        ),
      ).toBeInTheDocument();
    });
  });

  it("exports a redacted diagnostics bundle from the dashboard", async () => {
    render(<StatusDashboard />);

    fireEvent.click(
      screen.getByRole("button", {
        name: "statusDashboard.diagnostics.export",
      }),
    );

    await waitFor(() => {
      expect(mockedCmd).toHaveBeenCalledWith("export_diagnostics_bundle", {
        redact: true,
      });
    });

    expect(
      await screen.findByText("statusDashboard.diagnostics.exportReady"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "C:\\Users\\tester\\AppData\\Roaming\\LoLShorts\\diagnostics\\diagnostics_test.json",
      ),
    ).toBeInTheDocument();
  });
});
