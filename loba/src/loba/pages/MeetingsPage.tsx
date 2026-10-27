import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { ArrowLeft, Copy, RotateCw, Trash2 } from "lucide-react";
import { loba, type Meeting, type MeetingSummary, type Segment } from "../api";
import { Empty, PageTitle } from "../controls";
import { Markdown } from "../Markdown";
import { clock, useMeetingStatus } from "../meetingStatus";

export const MeetingsPage: React.FC = () => {
  const { t } = useTranslation("loba");
  const { status, elapsed } = useMeetingStatus();
  const [list, setList] = useState<MeetingSummary[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [live, setLive] = useState<Segment[]>([]);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    loba.meetings().then(setList);
  }, []);

  useEffect(() => {
    refresh();
    const a = listen("loba:meetings-changed", refresh);
    const b = listen<Segment>("loba:meeting-segment", (e) =>
      setLive((s) => [...s, e.payload].slice(-8)),
    );
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, [refresh]);

  const recording = status?.phase === "recording";

  const start = async () => {
    setBusy(true);
    setLive([]);
    try {
      await loba.meetingStart();
    } catch (e) {
      toast.error(t("meetings.startFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    setBusy(true);
    try {
      const id = await loba.meetingStop();
      setOpen(id);
    } finally {
      setBusy(false);
    }
  };

  if (open) {
    return (
      <Detail
        id={open}
        summarizing={status?.phase === "summarizing" && status.id === open}
        onBack={() => setOpen(null)}
      />
    );
  }

  return (
    <div>
      <PageTitle title={t("meetings.title")} lede={t("meetings.lede")}>
        {recording ? (
          <button
            type="button"
            className="loba-btn loba-btn-primary"
            onClick={stop}
            disabled={busy}
          >
            {t("meetings.stop")}
          </button>
        ) : (
          <button
            type="button"
            className="loba-btn loba-btn-primary"
            onClick={start}
            disabled={busy}
          >
            {t("meetings.start")}
          </button>
        )}
      </PageTitle>

      {recording && status && (
        <section className="loba-card px-5 py-4 mb-7">
          <div className="flex items-center gap-3 text-[13px]">
            <span className="loba-dot text-[#e5484d]" data-live="true" />
            <span className="text-ink font-medium">
              {t("meetings.recording")}
            </span>
            <span className="text-soft tabular-nums">{clock(elapsed)}</span>
            <span className="flex-1" />
            <span className="text-faint text-[12px]">
              {status.system_audio
                ? t("meetings.hearingBoth")
                : t("meetings.hearingYou")}
            </span>
          </div>
          {!status.system_audio && (
            <div className="mt-3 rounded-[9px] bg-void border border-line px-3.5 py-3 text-[12.5px] leading-[18px] text-soft flex items-start gap-3">
              <span className="flex-1">{t("meetings.noSystemAudio")}</span>
              <button
                type="button"
                className="loba-btn loba-btn-ghost h-[26px] shrink-0"
                onClick={() => loba.openScreenRecordingSettings()}
              >
                {t("meetings.openSettings")}
              </button>
            </div>
          )}
          <div className="mt-3 pt-3 border-t border-line flex flex-col gap-2">
            {live.length === 0 ? (
              <p className="text-[12.5px] text-faint">
                {t("meetings.listening")}
              </p>
            ) : (
              live.map((s, i) => <SegmentRow key={i} s={s} />)
            )}
          </div>
        </section>
      )}

      {list.length === 0 && !recording && <Empty>{t("meetings.empty")}</Empty>}
      {list.length > 0 && (
        <ul className="loba-card divide-y divide-line">
          {list.map((m) => (
            <li key={m.id}>
              <button
                type="button"
                onClick={() => setOpen(m.id)}
                className="w-full text-start px-5 py-3.5 hover:bg-raised/60 transition-colors cursor-pointer first:rounded-t-[14px] last:rounded-b-[14px]"
              >
                <div className="flex items-baseline gap-3">
                  <span className="text-[13.5px] font-medium text-ink">
                    <When ts={m.started_at} />
                  </span>
                  <span className="text-[12px] text-faint tabular-nums">
                    {t("meetings.minutes", {
                      count: Math.max(1, Math.round(m.duration_ms / 60000)),
                    })}
                  </span>
                  <span className="flex-1" />
                  {status?.phase === "summarizing" && status.id === m.id ? (
                    <span className="text-[11.5px] text-soft">
                      {t("meetings.writing")}
                    </span>
                  ) : (
                    !m.has_recap && (
                      <span className="text-[11.5px] text-faint">
                        {t("meetings.noRecap")}
                      </span>
                    )
                  )}
                </div>
                {m.preview && (
                  <p className="mt-1 text-[12.5px] leading-[18px] text-soft line-clamp-2">
                    {m.preview}
                  </p>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};

const When: React.FC<{ ts: number }> = ({ ts }) => {
  const { i18n } = useTranslation();
  return (
    <>
      {new Date(ts * 1000).toLocaleString(i18n.language, {
        weekday: "short",
        day: "numeric",
        month: "short",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      })}
    </>
  );
};

const SegmentRow: React.FC<{ s: Segment }> = ({ s }) => {
  const { t } = useTranslation("loba");
  return (
    <div className="grid grid-cols-[3.5rem_4.5rem_1fr] gap-x-3 text-[13px] leading-5">
      <span className="text-faint tabular-nums">{clock(s.start_ms)}</span>
      <span
        className={s.speaker === "you" ? "text-ink font-medium" : "text-soft"}
      >
        {t(`meetings.speaker.${s.speaker}`)}
      </span>
      <span className="text-ink select-text">{s.text}</span>
    </div>
  );
};

const Detail: React.FC<{
  id: string;
  summarizing: boolean;
  onBack: () => void;
}> = ({ id, summarizing, onBack }) => {
  const { t, i18n } = useTranslation("loba");
  const [m, setM] = useState<Meeting | null>(null);
  const [confirm, setConfirm] = useState(false);

  const load = useCallback(() => {
    loba
      .meeting(id)
      .then(setM)
      .catch(() => setM(null));
  }, [id]);

  useEffect(() => {
    load();
    const un = listen("loba:meetings-changed", load);
    return () => {
      un.then((f) => f());
    };
  }, [load]);

  if (!m) {
    return (
      <div>
        <Back onBack={onBack} />
        <Empty>{t("meetings.saving")}</Empty>
      </div>
    );
  }

  const title = t("meetings.docTitle", {
    when: new Date(m.started_at * 1000).toLocaleString(i18n.language, {
      dateStyle: "medium",
      timeStyle: "short",
    }),
  });

  const copy = async (what: "recap" | "all") => {
    const text =
      what === "recap" && m.recap
        ? m.recap
        : await loba.meetingMarkdown(m.id, title);
    await loba.copy(text);
    toast.success(
      t(what === "recap" ? "meetings.copiedRecap" : "meetings.copiedAll"),
    );
  };

  return (
    <div>
      <Back onBack={onBack} />
      <header className="mb-6 flex items-end justify-between gap-6">
        <div>
          <h1 className="loba-title text-[22px] text-ink">
            <When ts={m.started_at} />
          </h1>
          <p className="mt-1 text-[13px] text-soft">
            {t("meetings.minutes", {
              count: Math.max(1, Math.round(m.duration_ms / 60000)),
            })}
            {" · "}
            {m.system_audio
              ? t("meetings.hearingBoth")
              : t("meetings.hearingYou")}
          </p>
        </div>
        <div className="flex items-center gap-1.5">
          {m.recap && (
            <button
              type="button"
              className="loba-btn loba-btn-ghost"
              onClick={() => copy("recap")}
            >
              <Copy size={13} />
              {t("meetings.copyRecap")}
            </button>
          )}
          <button
            type="button"
            className="loba-btn loba-btn-quiet"
            onClick={() => copy("all")}
          >
            {t("meetings.copyAll")}
          </button>
        </div>
      </header>

      <section className="loba-card px-6 py-5">
        {summarizing ? (
          <p className="text-[13px] text-soft flex items-center gap-2.5">
            <span className="loba-dot text-steel" data-live="true" />
            {t("meetings.writing")}
          </p>
        ) : m.recap ? (
          <Markdown text={m.recap} />
        ) : (
          <div className="flex items-center gap-4">
            <p className="flex-1 text-[13px] text-soft">
              {m.recap_error
                ? t("meetings.recapFailed", { reason: m.recap_error })
                : t("meetings.noRecapYet")}
            </p>
          </div>
        )}
        {!summarizing && (
          <div className="mt-4 pt-3 border-t border-line flex items-center gap-1.5 -ms-2">
            <button
              type="button"
              className="loba-btn loba-btn-quiet"
              onClick={() => loba.meetingRecap(m.id)}
            >
              <RotateCw size={13} />
              {m.recap ? t("meetings.redoRecap") : t("meetings.writeRecap")}
            </button>
            <span className="flex-1" />
            <button
              type="button"
              className={`loba-btn ${confirm ? "loba-btn-ghost text-warn" : "loba-btn-quiet"}`}
              onClick={async () => {
                if (!confirm) {
                  setConfirm(true);
                  setTimeout(() => setConfirm(false), 3000);
                  return;
                }
                await loba.meetingDelete(m.id);
                onBack();
              }}
            >
              <Trash2 size={13} />
              {confirm ? t("meetings.confirmDelete") : t("meetings.delete")}
            </button>
          </div>
        )}
      </section>

      <h2 className="loba-label mt-8 mb-2 ps-1">{t("meetings.transcript")}</h2>
      {m.segments.length === 0 ? (
        <Empty>{t("meetings.nothingHeard")}</Empty>
      ) : (
        <div className="loba-card px-5 py-4 flex flex-col gap-2.5">
          {m.segments.map((s, i) => (
            <SegmentRow key={i} s={s} />
          ))}
        </div>
      )}
    </div>
  );
};

const Back: React.FC<{ onBack: () => void }> = ({ onBack }) => {
  const { t } = useTranslation("loba");
  return (
    <button
      type="button"
      onClick={onBack}
      className="mb-5 -ms-2 loba-btn loba-btn-quiet"
    >
      <ArrowLeft size={13} />
      {t("meetings.back")}
    </button>
  );
};
