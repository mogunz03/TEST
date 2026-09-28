import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { loba, type Dictionary, type DictionaryEntry } from "../api";
import { PageTitle } from "../controls";

const splitHeard = (s: string) =>
  s
    .split(",")
    .map((x) => x.trim())
    .filter(Boolean);

export const DictionaryPage: React.FC = () => {
  const { t } = useTranslation("loba");
  const [dict, setDict] = useState<Dictionary | null>(null);
  const [draft, setDraft] = useState("");
  const [fresh, setFresh] = useState<string | null>(null);

  useEffect(() => {
    loba.getDictionary().then(setDict);
  }, []);

  // The wash on a new row fades out on its own (1.2 s colour transition).
  useEffect(() => {
    if (!fresh) return;
    const id = setTimeout(() => setFresh(null), 1600);
    return () => clearTimeout(id);
  }, [fresh]);

  const save = async (entries: DictionaryEntry[]) => {
    setDict({ entries }); // optimistic; the backend normalises and echoes
    setDict(await loba.setDictionary({ entries }));
  };

  const add = () => {
    const term = draft.trim();
    if (!term || !dict) return;
    setDraft("");
    setFresh(term);
    save([...dict.entries, { term, sounds_like: [] }]);
  };

  if (!dict) return null;
  // Stored oldest-first (newest terms lead the Whisper prompt); shown
  // newest-first so what you just added is where your eyes already are.
  const rows = dict.entries.map((entry, index) => ({ entry, index })).reverse();

  return (
    <div>
      <PageTitle title={t("dictionary.title")} lede={t("dictionary.lede")} />

      <form
        onSubmit={(e) => {
          e.preventDefault();
          add();
        }}
        className="flex items-center gap-3 mb-4"
      >
        <input
          className="loba-input text-[15px] select-text"
          placeholder={t("dictionary.addPlaceholder")}
          value={draft}
          spellCheck={false}
          onChange={(e) => setDraft(e.target.value)}
        />
        <button
          type="submit"
          disabled={!draft.trim()}
          className="text-[13px] font-medium text-clay disabled:text-soft disabled:opacity-50 transition-opacity cursor-pointer disabled:cursor-default"
        >
          {t("dictionary.add")}
        </button>
      </form>

      {rows.length === 0 && (
        <p className="py-8 text-[13px] text-soft">{t("dictionary.empty")}</p>
      )}

      <ul>
        {rows.map(({ entry, index }) => (
          <EntryRow
            key={`${entry.term}-${index}`}
            entry={entry}
            highlight={entry.term === fresh}
            onChange={(next) =>
              save(dict.entries.map((e, i) => (i === index ? next : e)))
            }
            onRemove={() => save(dict.entries.filter((_, i) => i !== index))}
          />
        ))}
      </ul>
    </div>
  );
};

const EntryRow: React.FC<{
  entry: DictionaryEntry;
  highlight: boolean;
  onChange: (next: DictionaryEntry) => void;
  onRemove: () => void;
}> = ({ entry, highlight, onChange, onRemove }) => {
  const { t } = useTranslation("loba");
  const [heard, setHeard] = useState(entry.sounds_like.join(", "));
  const heardRef = useRef<HTMLInputElement>(null);

  useEffect(() => setHeard(entry.sounds_like.join(", ")), [entry.sounds_like]);

  // A term you just added has no mishearings yet: that's the next thing to
  // type, so the cursor goes there.
  useEffect(() => {
    if (highlight) heardRef.current?.focus();
  }, [highlight]);

  const commit = () => {
    const next = splitHeard(heard);
    if (next.join("\u0000") !== entry.sounds_like.join("\u0000")) {
      onChange({ ...entry, sounds_like: next });
    }
  };

  return (
    <li
      className={`group grid grid-cols-[minmax(0,11rem)_1fr_auto] items-baseline gap-x-5 py-3 border-b border-rule last:border-b-0 rounded-sm transition-colors duration-[1200ms] ${
        highlight ? "bg-clay-wash" : "bg-transparent"
      }`}
    >
      <span
        className="text-[15px] font-medium truncate select-text"
        title={entry.term}
      >
        {entry.term}
      </span>
      <label className="flex items-baseline gap-2 min-w-0 text-[13px]">
        <span className="shrink-0 text-soft">{t("dictionary.heardAs")}</span>
        <input
          ref={heardRef}
          className="loba-input !border-transparent focus:!border-clay py-0 text-[13px] select-text"
          placeholder={t("dictionary.heardPlaceholder")}
          value={heard}
          spellCheck={false}
          onChange={(e) => setHeard(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
      </label>
      <button
        type="button"
        onClick={onRemove}
        aria-label={t("dictionary.remove", { term: entry.term })}
        className="opacity-0 group-hover:opacity-100 focus:opacity-100 text-soft hover:text-clay transition-opacity cursor-pointer"
      >
        <X size={14} />
      </button>
    </li>
  );
};
