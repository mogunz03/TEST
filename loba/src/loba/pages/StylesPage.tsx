import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { loba, type Probe, type Styles } from "../api";
import { PageTitle, Switch } from "../controls";

type MapKey = "apps" | "window_titles";

export const StylesPage: React.FC = () => {
  const { t } = useTranslation("loba");
  const [styles, setStyles] = useState<Styles | null>(null);

  useEffect(() => {
    loba.getStyles().then(setStyles);
  }, []);

  if (!styles) return null;

  const save = async (next: Styles) => {
    setStyles(next);
    setStyles(await loba.setStyles(next));
  };
  const ids = Object.keys(styles.styles);

  return (
    <div>
      <PageTitle title={t("styles.title")} lede={t("styles.lede")}>
        <Detect />
      </PageTitle>

      <h2 className="loba-label mb-2 ps-1">{t("styles.voices")}</h2>
      <ul className="grid grid-cols-2 gap-2.5">
        {ids.map((id) => (
          <StyleRow
            key={id}
            id={id}
            isDefault={styles.default === id}
            style={styles.styles[id]}
            onChange={(style) =>
              save({ ...styles, styles: { ...styles.styles, [id]: style } })
            }
          />
        ))}
      </ul>

      <Mapping
        title={t("styles.apps")}
        note={t("styles.appsNote")}
        placeholder={t("styles.appPlaceholder")}
        map={styles.apps}
        ids={ids}
        onChange={(apps) => save({ ...styles, apps })}
      />
      <Mapping
        title={t("styles.titles")}
        note={t("styles.titlesNote")}
        placeholder={t("styles.titlePlaceholder")}
        map={styles.window_titles}
        ids={ids}
        onChange={(window_titles) => save({ ...styles, window_titles })}
      />
    </div>
  );
};

const StyleRow: React.FC<{
  id: string;
  isDefault: boolean;
  style: Styles["styles"][string];
  onChange: (next: Styles["styles"][string]) => void;
}> = ({ id, isDefault, style, onChange }) => {
  const { t } = useTranslation("loba");
  const [tone, setTone] = useState(style.tone);
  useEffect(() => setTone(style.tone), [style.tone]);

  return (
    <li className="loba-card px-4 py-3.5">
      <div className="flex items-center justify-between gap-4">
        <span className="text-[14px] font-medium text-ink capitalize">
          {id}
          {isDefault && (
            <span className="ms-2 rounded-[5px] bg-raised border border-line px-1.5 py-px text-[10.5px] font-normal text-soft normal-case">
              {t("styles.default")}
            </span>
          )}
        </span>
        <label className="flex items-center gap-2 text-[11.5px] text-faint">
          {style.rewrite ? t("styles.rewrites") : t("styles.literal")}
          <Switch
            label={t("styles.rewrites")}
            checked={style.rewrite}
            onChange={(rewrite) => onChange({ ...style, rewrite })}
          />
        </label>
      </div>
      {style.rewrite ? (
        <textarea
          rows={2}
          value={tone}
          spellCheck={false}
          placeholder={t("styles.tonePlaceholder")}
          onChange={(e) => setTone(e.target.value)}
          onBlur={() => tone !== style.tone && onChange({ ...style, tone })}
          className="mt-2.5 w-full resize-none bg-transparent text-[12.5px] leading-[18px] text-soft focus:text-ink outline-none rounded-[8px] border border-transparent hover:border-line focus:border-steel focus:bg-base -mx-2 px-2 py-1.5 transition-colors select-text [field-sizing:content] min-h-[3.2em]"
        />
      ) : (
        <p className="mt-2.5 py-1.5 text-[12.5px] leading-[18px] text-faint">
          {t("styles.literalNote")}
        </p>
      )}
    </li>
  );
};

const Mapping: React.FC<{
  title: string;
  note: string;
  placeholder: string;
  map: Record<string, string>;
  ids: string[];
  onChange: (next: Record<string, string>) => void;
}> = ({ title, note, placeholder, map, ids, onChange }) => {
  const { t } = useTranslation("loba");
  const [draft, setDraft] = useState("");
  const [draftStyle, setDraftStyle] = useState(ids[0] ?? "");
  const entries = Object.entries(map).sort(([a], [b]) => a.localeCompare(b));

  const add = () => {
    const key = draft.trim();
    if (!key) return;
    onChange({ ...map, [key]: draftStyle });
    setDraft("");
  };

  return (
    <section className="mt-8">
      <h2 className="loba-label ps-1">{title}</h2>
      <p className="mt-1 mb-2 ps-1 text-[12px] text-faint">{note}</p>
      <div className="loba-card px-5 py-3">
        <ul className="columns-2 gap-8">
          {entries.map(([key, id]) => (
            <li
              key={key}
              className="group break-inside-avoid flex items-center justify-between gap-2 py-1.5 text-[13px] text-ink"
            >
              <span className="truncate select-text" title={key}>
                {key}
              </span>
              <span className="flex items-center gap-1.5 shrink-0">
                <StyleSelect
                  ids={ids}
                  value={id}
                  onChange={(v) => onChange({ ...map, [key]: v })}
                />
                <button
                  type="button"
                  aria-label={t("styles.removeMapping", { key })}
                  onClick={() => {
                    const next = { ...map };
                    delete next[key];
                    onChange(next);
                  }}
                  className="opacity-0 group-hover:opacity-100 focus:opacity-100 text-faint hover:text-ink transition-opacity cursor-pointer"
                >
                  <X size={12} />
                </button>
              </span>
            </li>
          ))}
        </ul>
        <form
          className="mt-3 pt-3 border-t border-line flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            add();
          }}
        >
          <input
            className="loba-input !py-1.5 text-[13px] select-text"
            placeholder={placeholder}
            value={draft}
            spellCheck={false}
            onChange={(e) => setDraft(e.target.value)}
          />
          <StyleSelect ids={ids} value={draftStyle} onChange={setDraftStyle} />
          <button
            type="submit"
            disabled={!draft.trim()}
            className="loba-btn loba-btn-ghost"
          >
            {t("styles.map")}
          </button>
        </form>
      </div>
    </section>
  );
};

const StyleSelect: React.FC<{
  ids: string[];
  value: string;
  onChange: (v: string) => void;
}> = ({ ids, value, onChange }) => (
  <select
    value={value}
    onChange={(e) => onChange(e.target.value)}
    className="loba-select"
  >
    {ids.map((id) => (
      <option key={id} value={id}>
        {id}
      </option>
    ))}
  </select>
);

/** "Switch to an app, L.O.B.A will tell you what it sees." */
const Detect: React.FC = () => {
  const { t } = useTranslation("loba");
  const [phase, setPhase] = useState<"idle" | "waiting" | Probe>("idle");

  const run = async () => {
    setPhase("waiting");
    setPhase(await loba.probeFrontmost(3000));
  };

  return (
    <div className="flex flex-col items-end gap-1.5 text-[12px]">
      <button
        type="button"
        onClick={run}
        disabled={phase === "waiting"}
        className="loba-btn loba-btn-ghost"
      >
        {phase === "waiting" ? t("styles.detecting") : t("styles.detect")}
      </button>
      {typeof phase === "object" && (
        <span className="text-soft select-text">
          {t("styles.detected", {
            app: phase.app.name ?? t("styles.unknownApp"),
            style: phase.style.id,
            by: phase.style.matched_by ?? t("styles.fallback"),
          })}
        </span>
      )}
    </div>
  );
};
