import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { Keys } from "../Keys";
import { PageTitle, Row } from "../controls";

interface KeyEvent {
  modifiers: string[];
  key: string | null;
  is_key_down: boolean;
  hotkey_string: string;
}

const SHORTCUTS = [
  {
    id: "transcribe",
    labelKey: "shortcuts.dictate",
    noteKey: "shortcuts.dictateNote",
  },
  {
    id: "loba_command",
    labelKey: "shortcuts.command",
    noteKey: "shortcuts.commandNote",
  },
] as const;

export const ShortcutsPage: React.FC = () => {
  const { t } = useTranslation("loba");
  const { getSetting, updateSetting } = useSettings();
  const usesHandyKeys = getSetting("keyboard_implementation") === "handy_keys";
  const activation = getSetting("shortcut_activation") ?? "push_to_talk";

  return (
    <div>
      <PageTitle title={t("shortcuts.title")} lede={t("shortcuts.lede")} />

      {SHORTCUTS.map((s) =>
        usesHandyKeys ? (
          <Recorder
            key={s.id}
            id={s.id}
            label={t(s.labelKey)}
            note={t(s.noteKey)}
          />
        ) : (
          // The Tauri key backend (Linux default) has its own recorder.
          <ShortcutInput key={s.id} shortcutId={s.id} />
        ),
      )}

      <Row
        label={t("shortcuts.mode.label")}
        note={t(`shortcuts.mode.${activation}Note`)}
      >
        <div className="flex rounded-full border border-rule p-0.5 text-[12.5px]">
          {(["push_to_talk", "toggle", "hold_or_toggle"] as const).map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => updateSetting("shortcut_activation", m)}
              className={`px-3 py-1 rounded-full transition-colors cursor-pointer ${
                activation === m
                  ? "bg-ink text-sheet"
                  : "text-soft hover:text-ink"
              }`}
            >
              {t(`shortcuts.mode.${m}`)}
            </button>
          ))}
        </div>
      </Row>
    </div>
  );
};

/**
 * Click, press the combination, let go. Modifier-only chords (⌘⌥) commit
 * when every key is up; keyed ones when the key comes up. Esc or a click
 * elsewhere cancels and restores the previous shortcut.
 */
const Recorder: React.FC<{ id: string; label: string; note: string }> = ({
  id,
  label,
  note,
}) => {
  const { t } = useTranslation("loba");
  const { getSetting, updateBinding, resetBinding } = useSettings();
  const binding = getSetting("bindings")?.[id];
  const [recording, setRecording] = useState(false);
  const [live, setLive] = useState("");
  const keyed = useRef("");
  const modsOnly = useRef("");
  const unlisten = useRef<UnlistenFn | null>(null);
  const box = useRef<HTMLDivElement>(null);

  const stop = async () => {
    unlisten.current?.();
    unlisten.current = null;
    await commands.stopHandyKeysRecording().catch(() => {});
    setRecording(false);
    setLive("");
    keyed.current = "";
    modsOnly.current = "";
  };

  const commit = async (combo: string) => {
    try {
      await updateBinding(id, combo);
    } catch (e) {
      toast.error(t("shortcuts.failed"), { description: String(e) });
    }
    await stop();
  };

  const start = async () => {
    if (recording) return;
    const r = await commands.startHandyKeysRecording(id);
    if (r.status === "error") {
      toast.error(t("shortcuts.failed"), { description: String(r.error) });
      return;
    }
    setRecording(true);
    unlisten.current = await listen<KeyEvent>("handy-keys-event", (e) => {
      const { hotkey_string, is_key_down, key, modifiers } = e.payload;
      if (is_key_down && key === "escape" && modifiers.length === 0) {
        stop();
      } else if (is_key_down && hotkey_string) {
        (key ? keyed : modsOnly).current = hotkey_string;
        setLive(hotkey_string);
      } else if (!is_key_down && key) {
        const combo = keyed.current || hotkey_string;
        if (combo) commit(combo);
      } else if (!is_key_down && !key && modifiers.length === 0) {
        if (!keyed.current && modsOnly.current) commit(modsOnly.current);
      }
    });
  };

  // Cancel on click-away and on unmount — never leave the backend listening.
  useEffect(() => {
    if (!recording) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) stop();
    };
    window.addEventListener("click", away);
    return () => window.removeEventListener("click", away);
  });
  useEffect(
    () => () => {
      unlisten.current?.();
      commands.stopHandyKeysRecording().catch(() => {});
    },
    [],
  );

  const isDefault = binding?.current_binding === binding?.default_binding;

  return (
    <Row label={label} note={note}>
      <div ref={box} className="flex flex-col items-end gap-1.5">
        <button
          type="button"
          onClick={start}
          aria-label={t("shortcuts.change", { name: label })}
          className={`min-w-[7.5rem] min-h-[2.4rem] flex justify-end items-center rounded-lg px-1.5 transition-colors cursor-pointer ${
            recording
              ? "bg-clay-wash ring-1 ring-clay"
              : "hover:bg-clay-wash/60"
          }`}
        >
          {recording && !live ? (
            <span className="px-2 text-[12.5px] text-clay">
              {t("shortcuts.listening")}
            </span>
          ) : (
            <Keys binding={recording ? live : binding?.current_binding} />
          )}
        </button>
        <div className="flex gap-3 text-[11.5px] text-soft">
          {recording ? (
            <span>{t("shortcuts.hint")}</span>
          ) : (
            !isDefault && (
              <button
                type="button"
                onClick={() => resetBinding(id)}
                className="hover:text-ink cursor-pointer"
              >
                {t("shortcuts.reset")}
              </button>
            )
          )}
        </div>
      </div>
    </Row>
  );
};
