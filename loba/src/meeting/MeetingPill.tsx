import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { loba } from "@/loba/api";
import { clock, useMeetingStatus } from "@/loba/meetingStatus";

/** Floating over every app while a meeting records: drag it anywhere,
 *  stop from here without finding the main window. */
export const MeetingPill: React.FC = () => {
  const { t } = useTranslation("loba");
  const { status, elapsed } = useMeetingStatus();
  const [stopping, setStopping] = useState(false);

  return (
    <div
      data-tauri-drag-region
      className="loba-root h-full w-full flex items-center gap-2.5 ps-4 pe-1.5 rounded-full bg-void/95 border border-line-strong text-ink select-none cursor-grab"
    >
      <span
        data-tauri-drag-region
        className="loba-dot text-[#e5484d]"
        data-live="true"
      />
      <span data-tauri-drag-region className="text-[12.5px] font-medium">
        {t("meetings.pill")}
      </span>
      <span
        data-tauri-drag-region
        className="text-[12.5px] text-soft tabular-nums"
      >
        {clock(elapsed)}
      </span>
      {status && !status.system_audio && (
        <span
          data-tauri-drag-region
          className="text-[11px] text-warn"
          title={t("meetings.noSystemAudio")}
        >
          {t("meetings.micOnly")}
        </span>
      )}
      <span data-tauri-drag-region className="flex-1" />
      <button
        type="button"
        disabled={stopping}
        onClick={async () => {
          setStopping(true);
          await loba.meetingStop().catch(() => setStopping(false));
        }}
        className="h-[34px] px-3.5 rounded-full bg-ink text-void text-[12.5px] font-medium cursor-pointer hover:bg-white disabled:opacity-50"
      >
        {t("meetings.stop")}
      </button>
    </div>
  );
};
