import { useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";

export type Outcome =
  | { kind: "cleaned" }
  | { kind: "skipped"; detail: string }
  | { kind: "timed_out" }
  | { kind: "fallback"; detail: string };

export interface LastRun {
  outcome: Outcome;
  elapsed_ms: number;
}

// One listener for the app's lifetime, so the result survives page switches.
let current: LastRun | null = null;
const subscribers = new Set<() => void>();
listen<LastRun>("loba:ran", (e) => {
  current = e.payload;
  subscribers.forEach((fn) => fn());
});

export const useLastRun = () =>
  useSyncExternalStore(
    (fn) => {
      subscribers.add(fn);
      return () => subscribers.delete(fn);
    },
    () => current,
  );
