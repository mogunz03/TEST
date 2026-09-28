import React from "react";
import { Trans, useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { Keys } from "../Keys";
import { useLoba } from "../useOllama";
import { useLastRun } from "../lastRun";

export const Home: React.FC<{ onOpenCleanup: () => void }> = ({
  onOpenCleanup,
}) => {
  const { t } = useTranslation("loba");
  const { settings } = useSettings();
  const { config, ollama } = useLoba();
  const binding = settings?.bindings?.transcribe?.current_binding;
  const commandBinding = settings?.bindings?.loba_command?.current_binding;
  const last = useLastRun();

  const tone = !config?.cleanup_enabled
    ? { color: "text-soft", live: false, key: "home.status.off" }
    : ollama.kind === "ready"
      ? { color: "text-moss", live: true, key: "home.status.ready" }
      : ollama.kind === "checking"
        ? { color: "text-soft", live: false, key: "home.status.checking" }
        : {
            color: "text-clay",
            live: false,
            key: `home.status.${ollama.kind}`,
          };

  return (
    <div className="flex flex-col">
      <h1 className="loba-display text-[34px]">
        <Trans
          t={t}
          i18nKey="home.headline"
          components={{
            keys: (
              <Keys binding={binding} className="mx-1 -translate-y-[3px]" />
            ),
          }}
        />
      </h1>
      <p className="mt-3 text-[15px] leading-6 text-soft max-w-[440px]">
        {t("home.lede")}
      </p>

      <div className="mt-10 flex items-center gap-2.5 text-[13px]">
        <span className={`loba-dot ${tone.color}`} data-live={tone.live} />
        <span>{t(tone.key, { model: config?.model ?? "" })}</span>
        <button
          type="button"
          onClick={onOpenCleanup}
          className="ms-1 text-soft underline decoration-rule underline-offset-4 hover:text-ink hover:decoration-clay transition-colors cursor-pointer"
        >
          {t("home.status.adjust")}
        </button>
      </div>

      {last && (
        <p className="mt-2 ps-[17px] text-[12.5px] text-soft">
          {t(`home.last.${last.outcome.kind}`, {
            seconds: (last.elapsed_ms / 1000).toFixed(1),
            detail: "detail" in last.outcome ? last.outcome.detail : "",
            app: last.app ?? t("home.unknownApp"),
            style: last.style,
          })}
        </p>
      )}

      {config?.cleanup_enabled && ollama.kind === "down" && (
        <Fix lines={["ollama serve", `ollama pull ${config.model}`]} />
      )}
      {config?.cleanup_enabled && ollama.kind === "missing-model" && (
        <Fix lines={[`ollama pull ${config.model}`]} />
      )}

      <ol className="mt-14 grid grid-cols-[auto_1fr] gap-x-5 gap-y-4 text-[14px]">
        {(["speak", "clean", "paste"] as const).map((step, i) => (
          <React.Fragment key={step}>
            <span className="loba-display text-[15px] text-soft tabular-nums">
              {i + 1}
            </span>
            <span>
              <span className="font-medium">
                {t(`home.flow.${step}.title`)}
              </span>
              <span className="text-soft">
                {" — "}
                {t(`home.flow.${step}.body`, {
                  seconds: ((config?.timeout_ms ?? 4000) / 1000).toFixed(0),
                })}
              </span>
            </span>
          </React.Fragment>
        ))}
      </ol>

      <section className="mt-14 pt-6 border-t border-rule">
        <h2 className="loba-label">{t("home.command.label")}</h2>
        <p className="mt-3 text-[15px] leading-7">
          <Trans
            t={t}
            i18nKey="home.command.body"
            components={{
              keys: (
                <Keys
                  binding={commandBinding}
                  className="mx-0.5 -translate-y-[1px]"
                />
              ),
            }}
          />
        </p>
        <ul className="mt-3 flex flex-wrap gap-x-5 gap-y-1 text-[13px] text-soft">
          {(
            t("home.command.examples", { returnObjects: true }) as string[]
          ).map((ex) => (
            <li key={ex} className="italic">
              “{ex}”
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
};

/** The exact terminal lines that fix the problem, selectable. */
const Fix: React.FC<{ lines: string[] }> = ({ lines }) => (
  <pre className="mt-4 select-text cursor-text text-[12.5px] leading-6 font-mono bg-clay-wash rounded-lg px-4 py-3 w-fit">
    {lines.join("\n")}
  </pre>
);
