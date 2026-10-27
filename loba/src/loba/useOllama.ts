import { useCallback, useEffect, useState } from "react";
import { loba, type LobaConfig } from "./api";

export type OllamaState =
  | { kind: "checking" }
  | { kind: "down" }
  | { kind: "missing-model"; models: string[] }
  | { kind: "ready"; models: string[] };

/** Loaded config plus a live read of what Ollama actually has installed. */
export function useLoba() {
  const [config, setConfig] = useState<LobaConfig | null>(null);
  const [ollama, setOllama] = useState<OllamaState>({ kind: "checking" });

  const probe = useCallback(async (cfg: LobaConfig) => {
    try {
      const models = await loba.listModels();
      const has = models.some(
        (m) => m === cfg.model || m === `${cfg.model}:latest`,
      );
      setOllama(
        has ? { kind: "ready", models } : { kind: "missing-model", models },
      );
    } catch {
      setOllama({ kind: "down" });
    }
  }, []);

  useEffect(() => {
    loba.getConfig().then((cfg) => {
      setConfig(cfg);
      probe(cfg);
    });
  }, [probe]);

  // Re-probe when the window regains focus: the usual fix ("ollama serve",
  // "ollama pull") happens in another app.
  useEffect(() => {
    const onFocus = () => config && probe(config);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [config, probe]);

  const save = useCallback(
    async (patch: Partial<LobaConfig>) => {
      if (!config) return;
      const next = await loba.setConfig({ ...config, ...patch });
      setConfig(next);
      if (patch.model !== undefined || patch.ollama_url !== undefined) {
        probe(next);
      }
    },
    [config, probe],
  );

  return { config, ollama, save, reprobe: () => config && probe(config) };
}
