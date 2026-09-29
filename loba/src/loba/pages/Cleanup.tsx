import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Group, Row, Switch } from "../controls";
import { useLoba } from "../useOllama";

export const Cleanup: React.FC = () => {
  const { t } = useTranslation("loba");
  const { config, ollama, save } = useLoba();
  const [model, setModel] = useState("");
  const [budget, setBudget] = useState(4000);

  useEffect(() => {
    if (!config) return;
    setModel(config.model);
    setBudget(config.timeout_ms);
  }, [config]);

  if (!config) return null;

  const installed =
    ollama.kind === "ready" || ollama.kind === "missing-model"
      ? ollama.models
      : [];

  return (
    <>
      <p className="mb-6 ps-1 text-[13px] leading-5 text-soft max-w-[480px]">
        {t("cleanup.lede")}
      </p>
      <Group>
        <Row
          label={t("cleanup.enabled.label")}
          note={t("cleanup.enabled.note")}
        >
          <Switch
            label={t("cleanup.enabled.label")}
            checked={config.cleanup_enabled}
            onChange={(v) => save({ cleanup_enabled: v })}
          />
        </Row>

        <Row
          label={t("cleanup.model.label")}
          note={
            ollama.kind === "down"
              ? t("cleanup.model.down")
              : ollama.kind === "missing-model"
                ? t("cleanup.model.missing", { model: config.model })
                : t("cleanup.model.note")
          }
        >
          <input
            className="loba-input w-44 text-[13px] font-mono select-text"
            list="loba-models"
            value={model}
            spellCheck={false}
            onChange={(e) => setModel(e.target.value)}
            onBlur={() => model !== config.model && save({ model })}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
          <datalist id="loba-models">
            {installed.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        </Row>

        <Row label={t("cleanup.budget.label")} note={t("cleanup.budget.note")}>
          <div className="flex items-center gap-3">
            <input
              type="range"
              min={1000}
              max={8000}
              step={500}
              value={budget}
              onChange={(e) => setBudget(Number(e.target.value))}
              onMouseUp={() =>
                budget !== config.timeout_ms && save({ timeout_ms: budget })
              }
              onKeyUp={() =>
                budget !== config.timeout_ms && save({ timeout_ms: budget })
              }
              className="w-28 accent-[var(--loba-ink)] cursor-pointer"
              aria-label={t("cleanup.budget.label")}
            />
            <span className="w-10 text-end text-[13px] tabular-nums">
              {t("cleanup.budget.value", {
                seconds: (budget / 1000).toFixed(1),
              })}
            </span>
          </div>
        </Row>
      </Group>
      <Group label={t("cleanup.server.group")}>
        <Row label={t("cleanup.server.label")} note={t("cleanup.server.note")}>
          <span className="text-[13px] font-mono text-soft select-text">
            {config.ollama_url.replace(/^https?:\/\//, "")}
          </span>
        </Row>
      </Group>
    </>
  );
};
