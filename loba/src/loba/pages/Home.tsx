import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { ArrowRight } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { loba, type HistoryRow, type StatsView } from "../api";
import { useLoba } from "../useOllama";
import { useLastRun } from "../lastRun";
import { ShortcutField } from "../ShortcutDialog";
import { Keys } from "../Keys";

const partOfDay = () => {
  const h = new Date().getHours();
  return h < 5
    ? "evening"
    : h < 12
      ? "morning"
      : h < 18
        ? "afternoon"
        : "evening";
};

export const Home: React.FC<{
  onOpenHistory: () => void;
  onOpenCleanup: () => void;
}> = ({ onOpenHistory, onOpenCleanup }) => {
  const { t, i18n } = useTranslation("loba");
  const { settings } = useSettings();
  const [stats, setStats] = useState<StatsView | null>(null);
  const [recent, setRecent] = useState<HistoryRow[]>([]);
  const binding = settings?.bindings?.transcribe?.current_binding;
  const commandBinding = settings?.bindings?.loba_command?.current_binding;

  const refresh = useCallback(() => {
    loba.stats().then(setStats);
    loba.history(4).then(setRecent);
  }, []);

  useEffect(() => {
    refresh();
    const a = listen("loba:stats-changed", refresh);
    const b = listen("loba:history-changed", refresh);
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, [refresh]);

  const n = (v: number) => v.toLocaleString(i18n.language);
  const saved = stats?.minutes_saved ?? 0;

  return (
    <div className="@container flex flex-col">
      <h1 className="loba-title text-[26px] text-ink">
        {t(`home.greeting.${partOfDay()}`)}
      </h1>
      <p className="mt-1.5 text-[13.5px] text-soft">
        {stats && stats.dictations > 0
          ? t("home.sub.returning", { count: stats.days })
          : t("home.sub.first")}
      </p>

      {/* Totals. Four numbers, no charts: the point is a glance. */}
      <div className="mt-6 grid grid-cols-2 @min-[600px]:grid-cols-4 gap-2.5">
        <Stat label={t("home.stats.words")} value={n(stats?.words ?? 0)} />
        <Stat
          label={t("home.stats.saved")}
          value={
            saved >= 60
              ? t("home.stats.hours", { value: (saved / 60).toFixed(1) })
              : t("home.stats.minutes", { value: Math.round(saved) })
          }
        />
        <Stat
          label={t("home.stats.dictations")}
          value={n(stats?.dictations ?? 0)}
        />
        <Stat
          label={t("home.stats.streak")}
          value={t("home.stats.days", { count: stats?.streak ?? 0 })}
        />
      </div>

      {/* The hotkey, as big as anything on the page, with Change beside it. */}
      <section className="loba-card mt-3 px-6 py-5">
        <div className="flex items-center justify-between gap-6">
          <div>
            <div className="text-[15px] font-medium text-ink">
              {t("home.hotkey.title")}
            </div>
            <div className="mt-1 text-[12.5px] text-soft">
              {t(
                `home.hotkey.${settings?.shortcut_activation ?? "push_to_talk"}`,
              )}
            </div>
          </div>
          <ShortcutField id="transcribe" />
        </div>
        <div className="mt-4 pt-4 border-t border-line flex items-center justify-between gap-6">
          <div className="min-w-0">
            <div className="text-[13.5px] font-medium text-ink">
              {t("home.command.title")}
            </div>
            <div className="mt-1 text-[12.5px] leading-[18px] text-soft">
              {t("home.command.body")}
            </div>
          </div>
          <ShortcutField id="loba_command" compact />
        </div>
      </section>

      <CleanupStatus onOpen={onOpenCleanup} />

      <section className="mt-8">
        <div className="flex items-baseline justify-between mb-2 ps-1">
          <h2 className="loba-label">{t("home.recent.title")}</h2>
          {recent.length > 0 && (
            <button
              type="button"
              onClick={onOpenHistory}
              className="inline-flex items-center gap-1 text-[12px] text-soft hover:text-ink cursor-pointer"
            >
              {t("home.recent.all")}
              <ArrowRight size={12} />
            </button>
          )}
        </div>
        {recent.length === 0 ? (
          <div className="loba-card px-6 py-8 text-center">
            <p className="text-[13px] text-soft">{t("home.recent.empty")}</p>
            <p className="mt-3">
              <Keys binding={binding} />
            </p>
          </div>
        ) : (
          <ul className="loba-card divide-y divide-line">
            {recent.map((r) => (
              <RecentRow key={r.id} row={r} />
            ))}
          </ul>
        )}
      </section>

      {commandBinding && (
        <p className="mt-6 ps-1 text-[12px] text-faint">
          {t("home.command.examplesLead")}{" "}
          {(t("home.command.examples", { returnObjects: true }) as string[])
            .map((ex) => `“${ex}”`)
            .join(", ")}
        </p>
      )}
    </div>
  );
};

const Stat: React.FC<{ label: string; value: string }> = ({ label, value }) => (
  <div className="loba-card px-4 py-3.5">
    <div className="text-[11.5px] text-faint">{label}</div>
    <div className="mt-1.5 text-[21px] font-semibold tracking-[-0.02em] tabular-nums text-ink">
      {value}
    </div>
  </div>
);

const RecentRow: React.FC<{ row: HistoryRow }> = ({ row }) => {
  const { t, i18n } = useTranslation("loba");
  const [copied, setCopied] = useState(false);
  const text = row.cleaned ?? row.raw;
  const time = new Date(row.timestamp * 1000).toLocaleTimeString(
    i18n.language,
    { hour: "2-digit", minute: "2-digit", hour12: false },
  );
  return (
    <li>
      <button
        type="button"
        title={t("history.copy")}
        onClick={async () => {
          await loba.copy(text);
          setCopied(true);
          setTimeout(() => setCopied(false), 1100);
        }}
        className="w-full grid grid-cols-[3rem_1fr_auto] items-baseline gap-3 px-5 py-3 text-start cursor-copy hover:bg-raised/60 transition-colors first:rounded-t-[14px] last:rounded-b-[14px]"
      >
        <time className="text-[12px] text-faint tabular-nums">{time}</time>
        <span className="text-[13.5px] leading-5 text-ink truncate">
          {text}
        </span>
        <span className={`text-[11.5px] ${copied ? "text-ok" : "text-faint"}`}>
          {copied ? t("history.copied") : (row.app ?? "")}
        </span>
      </button>
    </li>
  );
};

/** One line on whether cleanup is working, with the fix when it isn't. */
const CleanupStatus: React.FC<{ onOpen: () => void }> = ({ onOpen }) => {
  const { t } = useTranslation("loba");
  const { config, ollama } = useLoba();
  const last = useLastRun();
  if (!config) return null;

  const tone = !config.cleanup_enabled
    ? { color: "text-faint", live: false, key: "home.status.off" }
    : ollama.kind === "ready"
      ? { color: "text-ok", live: true, key: "home.status.ready" }
      : ollama.kind === "checking"
        ? { color: "text-faint", live: false, key: "home.status.checking" }
        : {
            color: "text-warn",
            live: false,
            key: `home.status.${ollama.kind}`,
          };

  const fix =
    config.cleanup_enabled && ollama.kind === "down"
      ? ["ollama serve", `ollama pull ${config.model}`]
      : config.cleanup_enabled && ollama.kind === "missing-model"
        ? [`ollama pull ${config.model}`]
        : null;

  return (
    <section className="loba-card mt-3 px-5 py-3.5">
      <div className="flex items-center gap-3 text-[13px]">
        <span className={`loba-dot ${tone.color}`} data-live={tone.live} />
        <span className="text-ink">{t(tone.key, { model: config.model })}</span>
        {last && (
          <span className="text-faint truncate">
            {t(`home.last.${last.outcome.kind}`, {
              seconds: (last.elapsed_ms / 1000).toFixed(1),
              app: last.app ?? t("home.unknownApp"),
            })}
          </span>
        )}
        <span className="flex-1" />
        <button
          type="button"
          onClick={onOpen}
          className="loba-btn loba-btn-quiet -me-2 h-[26px]"
        >
          {t("home.status.adjust")}
        </button>
      </div>
      {fix && (
        <pre className="mt-3 select-text cursor-text text-[12px] leading-6 font-mono text-ink bg-void border border-line rounded-[9px] px-4 py-2.5">
          {fix.join("\n")}
        </pre>
      )}
    </section>
  );
};
