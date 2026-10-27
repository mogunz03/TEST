import { invoke } from "@tauri-apps/api/core";

// Hand-typed mirror of src-tauri/src/loba/commands.rs. The specta export in
// bindings.ts regenerates these on the next debug run; this file keeps the
// L.O.B.A surface readable in one place.

export interface LobaConfig {
  cleanup_enabled: boolean;
  ollama_url: string;
  model: string;
  timeout_ms: number;
  command_timeout_ms: number;
  /** light trims fillers; polished rewrites for clarity. */
  edit_level: "light" | "polished";
  /** Used instead of timeout_ms when edit_level is polished. */
  polished_timeout_ms: number;
  /** Press play/pause around a dictation when media is playing. */
  pause_media: boolean;
  keep_alive: string;
}

export interface DictionaryEntry {
  term: string;
  sounds_like: string[];
}

export interface Dictionary {
  entries: DictionaryEntry[];
}

export interface Style {
  tone: string;
  /** false = literal: dictionary only, no LLM. */
  rewrite: boolean;
}

export interface Styles {
  default: string;
  styles: Record<string, Style>;
  apps: Record<string, string>;
  window_titles: Record<string, string>;
}

export interface AppContext {
  name: string | null;
  bundle_id: string | null;
  window_title: string | null;
}

export interface Probe {
  app: AppContext;
  style: { id: string; style: Style; matched_by: string | null };
}

export interface HistoryRow {
  id: number;
  timestamp: number;
  raw: string;
  cleaned: string | null;
  app: string | null;
  style: string | null;
  outcome: "cleaned" | "timed_out" | "fallback" | "skipped" | null;
  detail: string | null;
  elapsed_ms: number | null;
}

export interface StatsView {
  dictations: number;
  words: number;
  days: number;
  streak: number;
  minutes_saved: number;
}

export type Speaker = "you" | "others";

export interface Segment {
  start_ms: number;
  speaker: Speaker;
  text: string;
}

export interface Meeting {
  id: string;
  /** Unix seconds. */
  started_at: number;
  duration_ms: number;
  segments: Segment[];
  /** Markdown. */
  recap: string | null;
  recap_error: string | null;
  /** False when only the microphone was recorded. */
  system_audio: boolean;
}

export interface MeetingSummary {
  id: string;
  started_at: number;
  duration_ms: number;
  segments: number;
  has_recap: boolean;
  preview: string;
}

export interface MeetingStatus {
  phase: "idle" | "recording" | "summarizing";
  id: string | null;
  elapsed_ms: number;
  segments: number;
  system_audio: boolean;
}

export const loba = {
  getConfig: () => invoke<LobaConfig>("loba_get_config"),
  setConfig: (config: LobaConfig) =>
    invoke<LobaConfig>("loba_set_config", { config }),
  /** Rejects when Ollama is not reachable. */
  listModels: () => invoke<string[]>("loba_list_models"),
  dataDir: () => invoke<string>("loba_data_dir"),
  getDictionary: () => invoke<Dictionary>("loba_get_dictionary"),
  /** Normalises (trims, dedupes) and returns what was actually saved. */
  setDictionary: (dictionary: Dictionary) =>
    invoke<Dictionary>("loba_set_dictionary", { dictionary }),
  getStyles: () => invoke<Styles>("loba_get_styles"),
  setStyles: (styles: Styles) => invoke<Styles>("loba_set_styles", { styles }),
  probeFrontmost: (delayMs: number) =>
    invoke<Probe>("loba_probe_frontmost", { delayMs }),
  history: (limit = 50) => invoke<HistoryRow[]>("loba_history", { limit }),
  copy: (text: string) => invoke<void>("loba_copy", { text }),
  stats: () => invoke<StatsView>("loba_get_stats"),
  meetingStart: () => invoke<MeetingStatus>("loba_meeting_start"),
  /** Resolves with the id; the recap arrives later (loba:meetings-changed). */
  meetingStop: () => invoke<string>("loba_meeting_stop"),
  meetingStatus: () => invoke<MeetingStatus>("loba_meeting_status"),
  meetings: () => invoke<MeetingSummary[]>("loba_meetings"),
  meeting: (id: string) => invoke<Meeting>("loba_meeting", { id }),
  meetingDelete: (id: string) => invoke<void>("loba_meeting_delete", { id }),
  meetingRecap: (id: string) => invoke<void>("loba_meeting_recap", { id }),
  meetingMarkdown: (id: string, title: string) =>
    invoke<string>("loba_meeting_markdown", { id, title }),
  openScreenRecordingSettings: () =>
    invoke<void>("loba_open_screen_recording_settings"),
};
