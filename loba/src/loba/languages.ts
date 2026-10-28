import type { ModelInfo } from "@/bindings";

/** Mirrors loba_core::languages::OFFERED. Each language is shown in its own
 *  name, so someone scanning for theirs finds it without translating. */
export const LANGUAGES: { code: string; name: string }[] = [
  { code: "en", name: "English" },
  { code: "fr", name: "Français" },
  { code: "zh", name: "中文" },
  { code: "es", name: "Español" },
  { code: "de", name: "Deutsch" },
  { code: "it", name: "Italiano" },
  { code: "pt", name: "Português" },
  { code: "ja", name: "日本語" },
  { code: "ko", name: "한국어" },
];

/** What History offers when nobody has said which languages they speak. */
export const FALLBACK = ["en", "fr", "zh"];

export const nameOf = (code: string) =>
  LANGUAGES.find((l) => l.code === code)?.name ?? code;

/** The model that hears every offered language. */
export const ALL_ROUNDER = "turbo";

export const hears = (model: ModelInfo | undefined, code: string) =>
  !model ||
  model.supported_languages.length === 0 ||
  model.supported_languages.some((l) => l.split(/[-_]/)[0] === code);
