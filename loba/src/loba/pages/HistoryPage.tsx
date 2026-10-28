import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { Check, Copy } from "lucide-react";
import { useModelStore } from "@/stores/modelStore";
import { loba, type HistoryRow } from "../api";
import { Empty, PageTitle } from "../controls";
import { FALLBACK, hears, nameOf } from "../languages";

const DOT: Record<string, string> = {
  cleaned: "text-ok",
  timed_out: "text-warn",
  fallback: "text-warn",
  skipped: "text-faint",
};

const dayKey = (ts: number) => new Date(ts * 1000).toDateString();

export const HistoryPage: React.FC = () => {
  const { t, i18n } = useTranslation("loba");
  const [rows, setRows] = useState<HistoryRow[] | null>(null);
  const [spoken, setSpoken] = useState<string[]>([]);
  const { models, currentModel } = useModelStore();
  const model = models.find((m) => m.id === currentModel);
  const redoIn = (spoken.length > 0 ? spoken : FALLBACK).filter((c) =>
    hears(model, c),
  );

  useEffect(() => {
    loba.getConfig().then((c) => setSpoken(c.languages));
  }, []);

  const refresh = useCallback(() => {
    loba.history(50).then(setRows);
  }, []);

  useEffect(() => {
    refresh();
    // Our own event fires after the metadata is written; Handy's covers
    // deletes and retries made from Engine → History.
    const a = listen("loba:history-changed", refresh);
    const b = listen("history-update-payload", refresh);
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, [refresh]);

  if (!rows) return null;

  const dayLabel = (ts: number) => {
    const d = new Date(ts * 1000);
    const today = new Date();
    const yesterday = new Date(today.getTime() - 86_400_000);
    if (d.toDateString() === today.toDateString()) return t("history.today");
    if (d.toDateString() === yesterday.toDateString())
      return t("history.yesterday");
    return d.toLocaleDateString(i18n.language, {
      weekday: "short",
      day: "numeric",
      month: "short",
    });
  };

  // Group into days, each day one card.
  const days: { key: string; ts: number; rows: HistoryRow[] }[] = [];
  for (const row of rows) {
    const key = dayKey(row.timestamp);
    const last = days[days.length - 1];
    if (last?.key === key) last.rows.push(row);
    else days.push({ key, ts: row.timestamp, rows: [row] });
  }

  return (
    <div>
      <PageTitle title={t("history.title")} lede={t("history.lede")} />
      {rows.length === 0 && <Empty>{t("history.empty")}</Empty>}
      {days.map((day) => (
        <section key={day.key} className="mt-7 first:mt-0">
          <h2 className="loba-label mb-2 ps-1">{dayLabel(day.ts)}</h2>
          <div className="loba-card divide-y divide-line">
            {day.rows.map((row) => (
              <Item key={row.id} row={row} redoIn={redoIn} onRedone={refresh} />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
};

const Item: React.FC<{
  row: HistoryRow;
  redoIn: string[];
  onRedone: () => void;
}> = ({ row, redoIn, onRedone }) => {
  const { t, i18n } = useTranslation("loba");
  const [showRaw, setShowRaw] = useState(false);
  const [redo, setRedo] = useState<"closed" | "open" | "busy">("closed");
  const [copied, setCopied] = useState<"final" | "raw" | null>(null);

  const final = row.cleaned ?? row.raw;
  const changed = final !== row.raw;
  const time = new Date(row.timestamp * 1000).toLocaleTimeString(
    i18n.language,
    {
      hour: "2-digit",
      minute: "2-digit",
      // A fixed-width 24 h stamp keeps the text column from shifting.
      hour12: false,
    },
  );

  const copy = async (which: "final" | "raw") => {
    await loba.copy(which === "raw" ? row.raw : final);
    setCopied(which);
    setTimeout(() => setCopied(null), 1200);
  };

  const redoAs = async (code: string) => {
    setRedo("busy");
    try {
      await loba.redoDictation(row.id, code);
      toast.success(t("history.redo.done", { lang: nameOf(code) }), {
        description: t("history.redo.copied"),
      });
      onRedone();
    } catch (e) {
      toast.error(t("history.redo.failed"), { description: String(e) });
    } finally {
      setRedo("closed");
    }
  };

  return (
    <article className="group grid grid-cols-[3rem_1fr] gap-x-3 px-5 py-3.5">
      <time className="pt-[3px] text-[12px] text-faint tabular-nums whitespace-nowrap">
        {time}
      </time>
      <div className="min-w-0">
        <div className="flex items-center gap-2 text-[11.5px] text-faint">
          {row.outcome && (
            <span
              className={`loba-dot ${DOT[row.outcome] ?? "text-faint"}`}
              title={t(`history.outcome.${row.outcome}`, {
                detail: row.detail ?? "",
              })}
            />
          )}
          <span className="truncate">
            {[row.app, row.style].filter(Boolean).join(" · ")}
          </span>
        </div>

        {/* The text itself is the copy target: one click, no aiming. */}
        <button
          type="button"
          onClick={() => copy("final")}
          className="mt-1 w-full text-start text-[13.5px] leading-[21px] text-ink whitespace-pre-wrap break-words cursor-copy rounded-[6px] hover:bg-raised -mx-1.5 px-1.5 py-0.5 transition-colors"
          title={t("history.copy")}
        >
          {final}
        </button>

        <div className="mt-1 flex items-center gap-4 text-[11.5px] text-soft empty:hidden">
          {changed && (
            <button
              type="button"
              onClick={() => setShowRaw((v) => !v)}
              className="hover:text-ink cursor-pointer"
            >
              {showRaw ? t("history.hideRaw") : t("history.showRaw")}
            </button>
          )}
          {redo === "busy" ? (
            <span className="inline-flex items-center gap-2 text-ink">
              <span className="loba-dot text-steel" data-live="true" />
              {t("history.redo.busy")}
            </span>
          ) : redo === "open" ? (
            <span className="inline-flex items-center gap-1">
              <span className="me-1.5">{t("history.redo.as")}</span>
              {redoIn.map((code) => (
                <button
                  key={code}
                  type="button"
                  onClick={() => redoAs(code)}
                  className="h-[22px] px-2 rounded-full border border-line-strong text-ink hover:bg-raised cursor-pointer"
                >
                  {nameOf(code)}
                </button>
              ))}
              <button
                type="button"
                onClick={() => setRedo("closed")}
                className="ms-1.5 text-faint hover:text-ink cursor-pointer"
              >
                {t("history.redo.cancel")}
              </button>
            </span>
          ) : (
            redoIn.length > 0 && (
              // Quiet until the row is hovered or focused: most dictations
              // come out right, and the list shouldn't read as a list of doubts.
              <button
                type="button"
                onClick={() => setRedo("open")}
                className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 transition-opacity hover:text-ink cursor-pointer"
              >
                {t("history.redo.open")}
              </button>
            )
          )}
          <span
            className={`inline-flex items-center gap-1 transition-opacity ${
              copied ? "opacity-100 text-ok" : "opacity-0"
            }`}
            aria-live="polite"
          >
            <Check size={12} />
            {copied === "raw" ? t("history.copiedRaw") : t("history.copied")}
          </span>
        </div>

        {showRaw && (
          <div className="mt-2 flex items-start gap-2 rounded-[9px] bg-void border border-line px-3 py-2">
            <p className="flex-1 text-[13px] leading-5 text-soft whitespace-pre-wrap break-words select-text">
              {row.raw}
            </p>
            <button
              type="button"
              onClick={() => copy("raw")}
              aria-label={t("history.copyRaw")}
              title={t("history.copyRaw")}
              className="shrink-0 pt-0.5 text-soft hover:text-ink cursor-pointer"
            >
              <Copy size={13} />
            </button>
          </div>
        )}
      </div>
    </article>
  );
};
