import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

function RecordingIndicator() {
  const [isRecording, setIsRecording] = useState(false);

  useEffect(() => {
    const unlisten = listen(
      "recording-status",
      (event: { payload: { recording?: boolean } }) => {
        setIsRecording(event.payload?.recording ?? false);
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  if (!isRecording) return null;

  return (
    <div
      className="flex items-center gap-2 p-2"
      role="status"
      aria-live="polite"
      aria-label="Recording active"
    >
      <div
        className="w-3 h-3 bg-red-500 rounded-full animate-pulse"
        aria-hidden="true"
      />
      <span className="text-white text-sm font-medium">REC</span>
    </div>
  );
}

function ClipSavedToast() {
  const [status, setStatus] = useState<"idle" | "saved" | "failed">("idle");

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const showToast = (next: "saved" | "failed") => {
      setStatus(next);
      clearTimeout(timer);
      timer = setTimeout(() => setStatus("idle"), 3000);
    };

    const unlistenSaved = listen("clip-saved", () => showToast("saved"));
    const unlistenFailed = listen("clip-save-failed", () =>
      showToast("failed"),
    );

    return () => {
      clearTimeout(timer);
      unlistenSaved.then((fn) => fn());
      unlistenFailed.then((fn) => fn());
    };
  }, []);

  if (status === "idle") return null;

  return (
    <div
      className={
        status === "saved"
          ? "p-2 bg-green-500/80 rounded text-white text-sm animate-fade-in"
          : "p-2 bg-red-500/80 rounded text-white text-sm animate-fade-in"
      }
      role="alert"
      aria-live="assertive"
    >
      {status === "saved" ? "클립 저장됨!" : "클립 저장 실패"}
    </div>
  );
}

export default function Overlay() {
  return (
    <div
      className="bg-transparent w-full h-full p-4 select-none"
      style={{ background: "transparent" }}
      role="status"
      aria-label="In-game overlay"
    >
      <RecordingIndicator />
      <ClipSavedToast />
    </div>
  );
}
