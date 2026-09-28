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
};
