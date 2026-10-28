import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useModelStore } from "@/stores/modelStore";
import { Group } from "./controls";
import { useLoba } from "./useOllama";
import { ALL_ROUNDER, LANGUAGES, hears, nameOf } from "./languages";

/** Which languages the user speaks, and whether the speech model can hear
 *  them. Lives at the top of General: it decides whether anything else works. */
export const LanguagesGroup: React.FC = () => {
  const { t } = useTranslation("loba");
  const { config, save } = useLoba();
  const chosen = config?.languages ?? [];

  const toggle = (code: string) =>
    save({
      languages: chosen.includes(code)
        ? chosen.filter((c) => c !== code)
        : [...chosen, code],
    });

  const note =
    chosen.length === 0
      ? t("languages.noneNote")
      : chosen.length === 1
        ? t("languages.oneNote", { lang: nameOf(chosen[0]) })
        : t("languages.manyNote");

  return (
    <Group label={t("languages.group")}>
      <div className="py-4">
        <div className="text-[13.5px] font-medium text-ink">
          {t("languages.label")}
        </div>
        <p className="mt-1 text-[12.5px] leading-[18px] text-soft max-w-[460px]">
          {note}
        </p>
        <div
          role="group"
          aria-label={t("languages.label")}
          className="mt-3.5 flex flex-wrap gap-1.5"
        >
          {LANGUAGES.map(({ code, name }) => {
            const on = chosen.includes(code);
            return (
              <button
                key={code}
                type="button"
                aria-pressed={on}
                onClick={() => toggle(code)}
                className={`h-[28px] ps-2.5 pe-3 inline-flex items-center gap-2 rounded-full border text-[12.5px] transition-[color,background-color,border-color] duration-150 cursor-pointer ${
                  on
                    ? "bg-raised border-line-strong text-ink"
                    : "border-line text-soft hover:text-ink hover:border-line-strong"
                }`}
              >
                {/* The dot fills in rather than a checkmark appearing: the chip
                    stays the same width, so the row never reflows. */}
                <span
                  className={`h-[6px] w-[6px] rounded-full transition-colors duration-150 ${
                    on ? "bg-steel" : "bg-line-strong"
                  }`}
                />
                {name}
              </button>
            );
          })}
        </div>
      </div>
      <ModelFit chosen={chosen} />
    </Group>
  );
};

/** Says so, plainly, when the speech model can't hear a chosen language,
 *  and offers the one model that hears them all. */
const ModelFit: React.FC<{ chosen: string[] }> = ({ chosen }) => {
  const { t } = useTranslation("loba");
  const { models, currentModel, downloadModel, selectModel, downloadProgress } =
    useModelStore();
  const [switching, setSwitching] = useState(false);

  const current = models.find((m) => m.id === currentModel);
  const target = models.find((m) => m.id === ALL_ROUNDER);
  const missing = chosen.filter((c) => !hears(current, c));

  // Once the download lands, finish the switch without another click.
  useEffect(() => {
    if (!switching || !target?.is_downloaded) return;
    selectModel(ALL_ROUNDER).then((ok) => {
      setSwitching(false);
      if (ok)
        toast.success(t("languages.fit.switched", { model: target.name }));
    });
  }, [switching, target?.is_downloaded, target?.name, selectModel, t]);

  if (!current || missing.length === 0 || current.id === ALL_ROUNDER) {
    return null;
  }

  const progress = downloadProgress[ALL_ROUNDER]?.percentage;
  const start = async () => {
    setSwitching(true);
    if (!target?.is_downloaded && !(await downloadModel(ALL_ROUNDER))) {
      setSwitching(false);
      toast.error(t("languages.fit.failed"));
    }
  };

  return (
    <div className="py-4 border-t border-line flex items-start gap-6">
      <div className="flex-1 min-w-0 text-[12.5px] leading-[18px]">
        <p className="text-ink">
          {t("languages.fit.cant", {
            model: current.name,
            langs: missing.map(nameOf).join(", "),
          })}
        </p>
        <p className="mt-1 text-soft">
          {target?.is_downloaded
            ? t("languages.fit.ready", { model: target?.name ?? "" })
            : t("languages.fit.download", {
                model: target?.name ?? "",
                size: ((target?.size_mb ?? 1549) / 1000).toFixed(1),
              })}
        </p>
      </div>
      <button
        type="button"
        className="loba-btn loba-btn-primary shrink-0 tabular-nums"
        onClick={start}
        disabled={switching}
      >
        {switching
          ? progress !== undefined
            ? t("languages.fit.downloading", { pct: Math.round(progress) })
            : t("languages.fit.switching")
          : t("languages.fit.switch", { model: target?.name ?? "" })}
      </button>
    </div>
  );
};
