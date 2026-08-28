import React from "react";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import "@testing-library/jest-dom";
import { ReplayTargetModal } from "./ReplayTargetModal";
import { recordingApi } from "../../api/recording";
import { cmd } from "../../api/client";
import { toast } from "../ui/use-toast";

jest.mock("@/api/recording", () => ({
  recordingApi: {
    getReplayTargetReadiness: jest.fn(),
  },
}));

jest.mock("@/api/client", () => ({
  cmd: jest.fn(),
}));

// Mock toast
jest.mock("@/components/ui/use-toast", () => {
  const mockToast = jest.fn();
  return {
    useToast: () => ({
      toast: mockToast,
    }),
    toast: mockToast,
  };
});

describe("ReplayTargetModal", () => {
  const mockOnClose = jest.fn();

  beforeEach(() => {
    jest.clearAllMocks();
  });

  test("shows loading state initially", async () => {
    (recordingApi.getReplayTargetReadiness as jest.Mock).mockResolvedValue({
      state: "loading",
      candidates: [],
      selectedTarget: null,
      retryable: true,
    });

    render(<ReplayTargetModal isOpen={true} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText("replayTarget.loading")).toBeInTheDocument();
    });
  });

  test("shows empty state and retry button", async () => {
    (recordingApi.getReplayTargetReadiness as jest.Mock).mockResolvedValue({
      state: "empty",
      candidates: [],
      selectedTarget: null,
      retryable: true,
    });

    render(<ReplayTargetModal isOpen={true} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText("replayTarget.empty")).toBeInTheDocument();
      expect(screen.getByText("common.retry")).toBeInTheDocument();
    });
  });

  test("shows unavailable state and retry button", async () => {
    (recordingApi.getReplayTargetReadiness as jest.Mock).mockResolvedValue({
      state: "unavailable",
      candidates: [],
      selectedTarget: null,
      retryable: true,
    });

    render(<ReplayTargetModal isOpen={true} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText("replayTarget.unavailable")).toBeInTheDocument();
      expect(screen.getByText("common.retry")).toBeInTheDocument();
    });
  });

  test("shows failed state and retry button", async () => {
    (recordingApi.getReplayTargetReadiness as jest.Mock).mockResolvedValue({
      state: "failed",
      candidates: [],
      selectedTarget: null,
      error: "Backend Error",
      retryable: true,
    });

    render(<ReplayTargetModal isOpen={true} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText("Backend Error")).toBeInTheDocument();
      expect(screen.getByText("common.retry")).toBeInTheDocument();
    });
  });

  test("hides retry button for non-retryable failed state", async () => {
    (recordingApi.getReplayTargetReadiness as jest.Mock).mockResolvedValue({
      state: "failed",
      candidates: [],
      selectedTarget: null,
      error: "Replay parser unavailable",
      retryable: false,
    });

    render(<ReplayTargetModal isOpen={true} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText("Replay parser unavailable")).toBeInTheDocument();
      expect(screen.queryByText("common.retry")).not.toBeInTheDocument();
    });
  });

  test("shows ready state with candidates", async () => {
    (recordingApi.getReplayTargetReadiness as jest.Mock).mockResolvedValue({
      state: "ready",
      candidates: [
        { summoner_name: "Faker", champion_id: 123, team_id: 1 },
        { summoner_name: "Chovy", champion_id: 456, team_id: 2 },
      ],
      selectedTarget: null,
      retryable: true,
    });

    render(<ReplayTargetModal isOpen={true} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText("Faker")).toBeInTheDocument();
      expect(screen.getByText("Chovy")).toBeInTheDocument();
    });
  });

  test("retry button triggers new poll", async () => {
    (recordingApi.getReplayTargetReadiness as jest.Mock)
      .mockResolvedValueOnce({
        state: "failed",
        candidates: [],
        selectedTarget: null,
        error: "First Fail",
        retryable: true,
      })
      .mockResolvedValueOnce({
        state: "ready",
        candidates: [{ summoner_name: "Faker", champion_id: 123, team_id: 1 }],
        selectedTarget: null,
        retryable: true,
      });

    render(<ReplayTargetModal isOpen={true} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText("First Fail")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("common.retry"));

    await waitFor(() => {
      expect(screen.getByText("Faker")).toBeInTheDocument();
    });
  });

  test("selecting target calls set_recording_target and closes modal", async () => {
    (recordingApi.getReplayTargetReadiness as jest.Mock).mockResolvedValue({
      state: "ready",
      candidates: [{ summoner_name: "Faker", champion_id: 123, team_id: 1 }],
      selectedTarget: null,
      retryable: true,
    });
    (cmd as jest.Mock).mockResolvedValue(undefined);

    render(<ReplayTargetModal isOpen={true} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText("Faker")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Faker"));

    await waitFor(() => {
      expect(cmd).toHaveBeenCalledWith("set_recording_target", {
        summonerName: "Faker",
      });
      expect(mockOnClose).toHaveBeenCalled();
    });
  });

  test("selecting target handles backend validation failure", async () => {
    (recordingApi.getReplayTargetReadiness as jest.Mock).mockResolvedValue({
      state: "ready",
      candidates: [{ summoner_name: "Faker", champion_id: 123, team_id: 1 }],
      selectedTarget: null,
      retryable: true,
    });
    (cmd as jest.Mock).mockRejectedValue(new Error("Invalid Target"));

    render(<ReplayTargetModal isOpen={true} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText("Faker")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Faker"));

    await waitFor(() => {
      // We check if toast was called with the error message
      // Since toast is mocked, we can check the call arguments
      expect(toast).toHaveBeenCalledWith(
        expect.objectContaining({
          description: "Invalid Target",
        }),
      );
    });
  });
});
