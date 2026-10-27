import React, { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { Keys } from "./Keys";

interface KeyEvent {
  modifiers: string[];
  key: string | null;
  is_key_down: boolean;
  hotkey_string: string;
}

export const SHORTCUT_IDS = ["transcribe", "loba_command"] as const;
export type ShortcutId = (typeof SHORTCUT_IDS)[number];

const MODIFIERS =
  /^(ctrl|control|option|alt|shift|command|cmd|meta|super|fn)(_left|_right)?$/;
const isModifier = (p: string) => MODIFIERS.test(p.toLowerCase());
/** A bare letter or Enter would fire every time you type it. F-keys and fn
 *  are fine on their own. */
const tooPlain = (combo: string) => {
  const parts = combo.split("+");
  return (
    parts.length === 1 && !isModifier(parts[0]) && !/^f\d{1,2}$/i.test(parts[0])
  );
};

type Problem = null | "plain" | "taken" | "failed";

/**
 * The Change dialog. Opens listening: hold the new combination, let go, and
 * it's shown back as keycaps. Nothing is saved until Save, so a wrong press
 * costs nothing. Try again as many times as you like.
 */
export const ShortcutDialog: React.FC<{
  id: ShortcutId;
  onClose: () => void;
}> = ({ id, onClose }) => {
  const { t } = useTranslation("loba");
  const { getSetting, updateBinding, resetBinding } = useSettings();
  const bindings = getSetting("bindings") ?? {};
  const current = bindings[id]?.current_binding ?? "";
  const fallback = bindings[id]?.default_binding ?? "";

  const [live, setLive] = useState("");
  const [captured, setCaptured] = useState("");
  const [problem, setProblem] = useState<Problem>(null);
  const [saving, setSaving] = useState(false);
  const keyed = useRef("");
  const modsOnly = useRef("");
  const closed = useRef(false);

  const close = () => {
    if (closed.current) return;
    closed.current = true;
    commands.stopHandyKeysRecording().catch(() => {});
    onClose();
  };

  const takenBy = (combo: string) =>
    Object.entries(bindings).find(
      ([other, b]) => other !== id && b?.current_binding === combo,
    )?.[0];

  const land = (combo: string) => {
    keyed.current = "";
    modsOnly.current = "";
    setLive("");
    setCaptured(combo);
    setProblem(tooPlain(combo) ? "plain" : takenBy(combo) ? "taken" : null);
  };

  useEffect(() => {
    let un: UnlistenFn | null = null;
    let cancelled = false;
    (async () => {
      const r = await commands.startHandyKeysRecording(id);
      if (cancelled) return;
      if (r.status === "error") {
        setProblem("failed");
        return;
      }
      un = await listen<KeyEvent>("handy-keys-event", (e) => {
        const { hotkey_string, is_key_down, key, modifiers } = e.payload;
        if (is_key_down && key === "escape" && modifiers.length === 0) {
          close();
        } else if (is_key_down && hotkey_string) {
          (key ? keyed : modsOnly).current = hotkey_string;
          setLive(hotkey_string);
        } else if (!is_key_down && key) {
          const combo = keyed.current || hotkey_string;
          if (combo) land(combo);
        } else if (!is_key_down && !key && modifiers.length === 0) {
          if (!keyed.current && modsOnly.current) land(modsOnly.current);
        }
      });
      if (cancelled) un();
    })();
    // The webview sees Esc too when it has focus; either path closes.
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => {
      cancelled = true;
      un?.();
      window.removeEventListener("keydown", onKey);
      commands.stopHandyKeysRecording().catch(() => {});
    };
    // Recording runs for the dialog's lifetime only.
  }, [id]);

  const save = async () => {
    if (!captured || problem) return;
    setSaving(true);
    try {
      await updateBinding(id, captured);
      close();
    } catch {
      setProblem("failed");
      setSaving(false);
    }
  };

  const reset = async () => {
    await resetBinding(id);
    close();
  };

  const shown = live || captured || current;
  const state = live ? "holding" : captured ? "captured" : "waiting";
  const name = t(`shortcuts.${id}.name`);

  return createPortal(
    <div
      className="loba-root loba-scrim fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-[2px]"
      onMouseDown={(e) => e.target === e.currentTarget && close()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("shortcuts.dialog.title", { name })}
        className="loba-dialog w-[420px] rounded-[18px] bg-card border border-line-strong shadow-[0_24px_60px_-12px_rgb(0_0_0/0.7)] p-6"
      >
        <h2 className="loba-title text-[17px] text-ink">
          {t("shortcuts.dialog.title", { name })}
        </h2>
        <p className="mt-1 text-[13px] leading-5 text-soft">
          {t("shortcuts.dialog.body")}
        </p>

        {/* The stage. A dashed well while waiting, solid once something
            is held or caught, so the state reads from across the room. */}
        <div
          className={`mt-5 h-[112px] rounded-[14px] flex flex-col items-center justify-center gap-3 transition-colors duration-150 ${
            state === "waiting"
              ? "border border-dashed border-line-strong bg-void"
              : "border border-line-strong bg-base"
          }`}
          aria-live="polite"
        >
          <Keys binding={shown} large pressed={state === "holding"} />
          <span
            className={`text-[12px] ${
              problem
                ? "text-warn"
                : state === "captured"
                  ? "text-ok"
                  : "text-faint"
            }`}
          >
            {problem
              ? t(`shortcuts.dialog.problem.${problem}`, {
                  other: t(
                    `shortcuts.${takenBy(captured) ?? "transcribe"}.name`,
                    {
                      defaultValue: takenBy(captured),
                    },
                  ),
                })
              : t(`shortcuts.dialog.${state}`)}
          </span>
        </div>

        <div className="mt-5 flex items-center gap-2">
          {current !== fallback && (
            <button
              type="button"
              className="loba-btn loba-btn-quiet -ms-2"
              onClick={reset}
            >
              {t("shortcuts.dialog.reset")}
              <Keys binding={fallback} className="scale-[0.85] origin-left" />
            </button>
          )}
          <span className="flex-1" />
          <button
            type="button"
            className="loba-btn loba-btn-ghost"
            onClick={close}
          >
            {t("shortcuts.dialog.cancel")}
          </button>
          <button
            type="button"
            className="loba-btn loba-btn-primary"
            disabled={!captured || !!problem || saving || captured === current}
            onClick={save}
          >
            {t("shortcuts.dialog.save")}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
};

/** Current shortcut as keycaps with a Change button next to it. */
export const ShortcutField: React.FC<{ id: ShortcutId; compact?: boolean }> = ({
  id,
  compact = false,
}) => {
  const { t } = useTranslation("loba");
  const { getSetting } = useSettings();
  const [open, setOpen] = useState(false);
  const binding = getSetting("bindings")?.[id]?.current_binding;

  // The Tauri key backend (Linux default) can't record chords this way; it
  // has its own recorder.
  const backend = getSetting("keyboard_implementation");
  if (backend && backend !== "handy_keys") {
    return <ShortcutInput shortcutId={id} />;
  }

  return (
    <div className={`flex items-center ${compact ? "gap-3" : "gap-4"}`}>
      <Keys binding={binding} />
      <button
        type="button"
        className="loba-btn loba-btn-ghost"
        onClick={() => setOpen(true)}
        aria-label={t("shortcuts.changeNamed", {
          name: t(`shortcuts.${id}.name`),
        })}
      >
        {t("shortcuts.change")}
      </button>
      {open && <ShortcutDialog id={id} onClose={() => setOpen(false)} />}
    </div>
  );
};
