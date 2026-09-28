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
};
