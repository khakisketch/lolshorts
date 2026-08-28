import { act, render, screen } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import Overlay from "./Overlay";

type EventHandler = (event: { payload: unknown }) => void;

const listeners = new Map<string, EventHandler>();

jest.mock("@tauri-apps/api/event", () => ({
  listen: jest.fn((eventName: string, handler: EventHandler) => {
    listeners.set(eventName, handler);
    return Promise.resolve(() => listeners.delete(eventName));
  }),
}));

const emit = (eventName: string, payload: unknown) => {
  act(() => listeners.get(eventName)?.({ payload }));
};

describe("Overlay", () => {
  beforeEach(() => {
    listeners.clear();
    jest.clearAllMocks();
  });

  it("shows REC only while recording-status reports recording", () => {
    render(<Overlay />);

    expect(screen.queryByText("REC")).not.toBeInTheDocument();

    emit("recording-status", { recording: true });
    expect(screen.getByText("REC")).toBeInTheDocument();

    emit("recording-status", { recording: false });
    expect(screen.queryByText("REC")).not.toBeInTheDocument();
  });

  it("does not subscribe to or render raw game event names", () => {
    render(<Overlay />);

    expect(listen).not.toHaveBeenCalledWith("game-event", expect.any(Function));
    expect(
      screen.queryByRole("log", { name: "Game events" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("GameEnd")).not.toBeInTheDocument();
    expect(screen.queryByText("ChampionKill")).not.toBeInTheDocument();
  });

  it.each([
    ["clip-saved", "클립 저장됨!"],
    ["clip-save-failed", "클립 저장 실패"],
  ])(
    "shows and clears the %s toast after three seconds",
    (eventName, message) => {
      jest.useFakeTimers();
      render(<Overlay />);

      emit(eventName, {});
      expect(screen.getByRole("alert")).toHaveTextContent(message);

      act(() => jest.advanceTimersByTime(3000));
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();

      jest.useRealTimers();
    },
  );
});
