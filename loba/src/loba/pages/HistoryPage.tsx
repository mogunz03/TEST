import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Check, Copy } from "lucide-react";
import { loba, type HistoryRow } from "../api";
import { PageTitle } from "../controls";

const DOT: Record<string, string> = {
  cleaned: "text-moss",
  timed_out: "text-clay",
  fallback: "text-clay",
  skipped: "text-soft",
};

const dayKey = (ts: number) => new Date(ts * 1000).toDateString();

export const HistoryPage: React.FC = () => {
  const { t, i18n } = useTranslation("loba");
  const [rows, setRows] = useState<HistoryRow[] | null>(null);

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

  return (
    <div>
      <PageTitle title={t("history.title")} lede={t("history.lede")} />
      {rows.length === 0 && (
        <p className="py-8 text-[13px] text-soft">{t("history.empty")}</p>
      )}
      {rows.map((row, i) => (
        <React.Fragment key={row.id}>
          {(i === 0 ||
            dayKey(rows[i - 1].timestamp) !== dayKey(row.timestamp)) && (
            <h2 className="loba-label mt-8 mb-1 first:mt-0">
              {dayLabel(row.timestamp)}
            </h2>
          )}
          <Item row={row} />
        </React.Fragment>
      ))}
    </div>
  );
};

const Item: React.FC<{ row: HistoryRow }> = ({ row }) => {
  const { t, i18n } = useTranslation("loba");
  const [showRaw, setShowRaw] = useState(false);
  const [copied, setCopied] = useState<"final" | "raw" | null>(null);

  const final = row.cleaned ?? row.raw;
  const changed = final !== row.raw;
  const time = new Date(row.timestamp * 1000).toLocaleTimeString(
    i18n.language,
    {
      hour: "2-digit",
      minute: "2-digit",
    },
  );

  const copy = async (which: "final" | "raw") => {
    await loba.copy(which === "raw" ? row.raw : final);
    setCopied(which);
    setTimeout(() => setCopied(null), 1200);
  };

  return (
    <article className="group grid grid-cols-[3.25rem_1fr] gap-x-3 py-3 border-b border-rule last:border-b-0">
      <time className="pt-[3px] text-[12px] text-soft tabular-nums">
        {time}
      </time>
      <div className="min-w-0">
        <div className="flex items-center gap-2 text-[12px] text-soft">
          {row.outcome && (
            <span
              className={`loba-dot ${DOT[row.outcome] ?? "text-soft"}`}
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
          className="mt-1 w-full text-start text-[14.5px] leading-6 whitespace-pre-wrap break-words cursor-copy rounded-sm hover:bg-clay-wash/60 -mx-1 px-1 transition-colors"
          title={t("history.copy")}
        >
          {final}
        </button>

        <div className="mt-1 flex items-center gap-4 text-[12px] text-soft">
          <span
            className={`inline-flex items-center gap-1 transition-opacity ${
              copied ? "opacity-100 text-moss" : "opacity-0"
            }`}
            aria-live="polite"
          >
            <Check size={12} />
            {copied === "raw" ? t("history.copiedRaw") : t("history.copied")}
          </span>
          {changed && (
            <button
              type="button"
              onClick={() => setShowRaw((v) => !v)}
              className="hover:text-ink cursor-pointer"
            >
              {showRaw ? t("history.hideRaw") : t("history.showRaw")}
            </button>
          )}
        </div>

        {showRaw && (
          <div className="mt-2 flex items-start gap-2 ps-3 border-s-2 border-rule">
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
