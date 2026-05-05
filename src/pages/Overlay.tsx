import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';

function RecordingIndicator() {
  const [isRecording, setIsRecording] = useState(false);

  useEffect(() => {
    const unlisten = listen('recording-status', (event: { payload: { recording?: boolean } }) => {
      setIsRecording(event.payload?.recording ?? false);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  if (!isRecording) return null;

  return (
    <div className="flex items-center gap-2 p-2" role="status" aria-live="polite" aria-label="Recording active">
      <div className="w-3 h-3 bg-red-500 rounded-full animate-pulse" aria-hidden="true" />
      <span className="text-white text-sm font-medium">REC</span>
    </div>
  );
}

function ClipSavedToast() {
  const [show, setShow] = useState(false);

  useEffect(() => {
    const unlisten = listen('clip-saved', () => {
      setShow(true);
      const timer = setTimeout(() => setShow(false), 3000);
      return () => clearTimeout(timer);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  if (!show) return null;

  return (
    <div
      className="p-2 bg-green-500/80 rounded text-white text-sm animate-fade-in"
      role="alert"
      aria-live="assertive"
    >
      클립 저장됨!
    </div>
  );
}

function EventFeed() {
  const [events, setEvents] = useState<string[]>([]);

  useEffect(() => {
    const unlisten = listen('game-event', (event: { payload: { name?: string } }) => {
      setEvents((prev) => [event.payload?.name ?? 'Event', ...prev].slice(0, 3));
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <div className="space-y-1" role="log" aria-label="Game events" aria-live="polite">
      {events.map((e, i) => (
        <div key={i} className="text-white/70 text-xs">
          {e}
        </div>
      ))}
    </div>
  );
}

export default function Overlay() {
  return (
    <div
      className="bg-transparent w-full h-full p-4 select-none"
      style={{ background: 'transparent' }}
      role="status"
      aria-label="In-game overlay"
    >
      <RecordingIndicator />
      <ClipSavedToast />
      <EventFeed />
    </div>
  );
}
