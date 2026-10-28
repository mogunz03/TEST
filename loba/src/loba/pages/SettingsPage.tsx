import React, { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useSettings } from "@/hooks/useSettings";
import { Group, PageTitle, Row, Segmented, Switch } from "../controls";
import { ShortcutField } from "../ShortcutDialog";
import { useLoba } from "../useOllama";
import { commands } from "@/bindings";
import { Cleanup } from "./Cleanup";
import { LanguagesGroup } from "../LanguagesGroup";

export type SettingsTab = "general" | "cleanup" | "engine";
const TABS: SettingsTab[] = ["general", "cleanup", "engine"];

export const SettingsPage: React.FC<{
  tab: SettingsTab;
  onTab: (tab: SettingsTab) => void;
  engine: ReactNode;
}> = ({ tab, onTab, engine }) => {
  const { t } = useTranslation("loba");
  return (
    <div className="h-full flex flex-col">
      <div className="w-full max-w-[680px] mx-auto px-10 pt-9 shrink-0">
        <h1 className="loba-title text-[24px] text-ink">
          {t("settings.title")}
        </h1>
        {/* Underline tabs: the page stays one surface, not a stack of pills. */}
        <div role="tablist" className="mt-5 flex gap-6 border-b border-line">
          {TABS.map((id) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              onClick={() => onTab(id)}
              className={`relative pb-2.5 text-[13px] cursor-pointer transition-colors ${
                tab === id ? "text-ink" : "text-soft hover:text-ink"
              }`}
            >
              {t(`settings.tabs.${id}`)}
              <span
                className={`absolute inset-x-0 -bottom-px h-[2px] rounded-full transition-colors ${
                  tab === id ? "bg-ink" : "bg-transparent"
                }`}
              />
            </button>
          ))}
        </div>
      </div>
      {tab === "engine" ? (
        <div key="engine" className="loba-page flex-1 min-h-0 mt-1">
          {engine}
        </div>
      ) : (
        <div className="loba-scroll flex-1 overflow-y-auto">
          <div
            key={tab}
            className="loba-page max-w-[680px] mx-auto px-10 pt-7 pb-14"
          >
            {tab === "general" ? <General /> : <Cleanup />}
          </div>
        </div>
      )}
    </div>
  );
};

const General: React.FC = () => {
  const { t } = useTranslation("loba");
  const { getSetting, updateSetting } = useSettings();
  const activation = getSetting("shortcut_activation") ?? "push_to_talk";
  const { config, save } = useLoba();
  // Mute is Handy's setting, pause is L.O.B.A's; the UI keeps them exclusive.
  const media: "leave" | "mute" | "pause" = config?.pause_media
    ? "pause"
    : getSetting("mute_while_recording")
      ? "mute"
      : "leave";
  const setMedia = (next: typeof media) => {
    updateSetting("mute_while_recording", next === "mute");
    if (config) save({ pause_media: next === "pause" });
  };
  const soundsOn = getSetting("audio_feedback") ?? true;
  const volume = getSetting("audio_feedback_volume") ?? 1;

  return (
    <>
      <Group label={t("settings.shortcuts")}>
        <Row
          label={t("shortcuts.transcribe.name")}
          note={t("shortcuts.transcribe.note")}
        >
          <ShortcutField id="transcribe" />
        </Row>
        <Row
          label={t("shortcuts.loba_command.name")}
          note={t("shortcuts.loba_command.note")}
        >
          <ShortcutField id="loba_command" />
        </Row>
        <Row
          label={t("shortcuts.mode.label")}
          note={t(`shortcuts.mode.${activation}Note`)}
        >
          <Segmented
            label={t("shortcuts.mode.label")}
            value={activation}
            onChange={(m) => updateSetting("shortcut_activation", m)}
            options={(
              ["push_to_talk", "toggle", "hold_or_toggle"] as const
            ).map((id) => ({ id, label: t(`shortcuts.mode.${id}`) }))}
          />
        </Row>
      </Group>

      <LanguagesGroup />

      <Group label={t("settings.app")}>
        <Row label={t("settings.sound.label")} note={t("settings.sound.note")}>
          <Switch
            label={t("settings.sound.label")}
            checked={getSetting("audio_feedback") ?? true}
            onChange={(v) => updateSetting("audio_feedback", v)}
          />
        </Row>
        {soundsOn && (
          <Row
            label={t("settings.volume.label")}
            note={t("settings.volume.note")}
          >
            <div className="flex items-center gap-3">
              <input
                type="range"
                min={0.05}
                max={1}
                step={0.05}
                value={volume}
                onChange={(e) =>
                  updateSetting("audio_feedback_volume", Number(e.target.value))
                }
                onMouseUp={() => commands.playTestSound("start")}
                onKeyUp={() => commands.playTestSound("start")}
                className="w-28 accent-[var(--loba-ink)] cursor-pointer"
                aria-label={t("settings.volume.label")}
              />
              <span className="w-10 text-end text-[13px] tabular-nums">
                {Math.round(volume * 100)}%
              </span>
            </div>
          </Row>
        )}
        <Row
          label={t("settings.media.label")}
          note={t(`settings.media.${media}Note`)}
        >
          <Segmented
            label={t("settings.media.label")}
            value={media}
            onChange={setMedia}
            options={(["leave", "mute", "pause"] as const).map((id) => ({
              id,
              label: t(`settings.media.${id}`),
            }))}
          />
        </Row>
        <Row label={t("settings.login.label")} note={t("settings.login.note")}>
          <Switch
            label={t("settings.login.label")}
            checked={getSetting("autostart_enabled") ?? false}
            onChange={(v) => updateSetting("autostart_enabled", v)}
          />
        </Row>
      </Group>

      <About />
    </>
  );
};

/** Version and the credit Handy's MIT license asks for. The license text
 *  itself ships in the app bundle (Resources/LICENSE-Handy.txt). */
const About: React.FC = () => {
  const { t } = useTranslation("loba");
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => {});
  }, []);

  return (
    <Group label={t("settings.about.label")}>
      <Row label={t("settings.about.version")}>
        <span className="text-[13px] text-soft tabular-nums select-text">
          {version}
        </span>
      </Row>
      <Row
        label={t("settings.about.handy.label")}
        note={t("settings.about.handy.note")}
      >
        <button
          type="button"
          className="loba-btn loba-btn-ghost"
          onClick={() =>
            openUrl("https://github.com/cjpais/Handy").catch(() => {})
          }
        >
          {t("settings.about.handy.source")}
        </button>
      </Row>
    </Group>
  );
};
