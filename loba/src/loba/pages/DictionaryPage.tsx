import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { loba, type Dictionary, type DictionaryEntry } from "../api";
import { Empty, PageTitle } from "../controls";

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
        className="flex items-stretch gap-2 mb-5"
      >
        <input
          className="loba-input text-[13.5px] select-text"
          placeholder={t("dictionary.addPlaceholder")}
          value={draft}
          spellCheck={false}
          onChange={(e) => setDraft(e.target.value)}
        />
        <button
          type="submit"
          disabled={!draft.trim()}
          className="loba-btn loba-btn-primary !h-auto px-4"
        >
          {t("dictionary.add")}
        </button>
      </form>

      {rows.length === 0 && <Empty>{t("dictionary.empty")}</Empty>}

      <ul className="loba-card divide-y divide-line empty:hidden">
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
      className={`group grid grid-cols-[minmax(0,11rem)_1fr_auto] items-center gap-x-5 px-5 py-2.5 first:rounded-t-[14px] last:rounded-b-[14px] transition-colors duration-[1200ms] ${
        highlight ? "bg-raised" : "bg-transparent"
      }`}
    >
      <span
        className="text-[13.5px] font-medium text-ink truncate select-text"
        title={entry.term}
      >
        {entry.term}
      </span>
      <label className="flex items-center gap-2 min-w-0 text-[12.5px]">
        <span className="shrink-0 text-faint">{t("dictionary.heardAs")}</span>
        <input
          ref={heardRef}
          className="loba-input !bg-transparent !border-transparent hover:!border-line focus:!border-steel !py-1 !px-2 text-[12.5px] select-text"
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
        className="opacity-0 group-hover:opacity-100 focus:opacity-100 text-faint hover:text-ink transition-opacity cursor-pointer"
      >
        <X size={14} />
      </button>
    </li>
  );
};
