import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { loba, type MeetingStatus } from "./api";

/** Live meeting state, with a ticking clock while recording. Shared by the
 *  Meetings page and the floating pill. */
export function useMeetingStatus() {
  const [status, setStatus] = useState<MeetingStatus | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const anchor = useRef(0);

  useEffect(() => {
    const apply = (s: MeetingStatus) => {
      anchor.current = Date.now() - s.elapsed_ms;
      setStatus(s);
      setElapsed(s.elapsed_ms);
    };
    loba.meetingStatus().then(apply);
    const un = listen<MeetingStatus>("loba:meeting", (e) => apply(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (status?.phase !== "recording") return;
    const id = setInterval(() => setElapsed(Date.now() - anchor.current), 500);
    return () => clearInterval(id);
  }, [status?.phase]);

  return { status, elapsed };
}

export const clock = (ms: number) => {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const mm = String(Math.floor(s / 60) % 60).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
};
