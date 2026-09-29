import React from "react";

const GLYPHS: Record<string, string> = {
  option: "⌥",
  alt: "⌥",
  command: "⌘",
  cmd: "⌘",
  meta: "⌘",
  super: "⌘",
  shift: "⇧",
  ctrl: "⌃",
  control: "⌃",
  fn: "fn",
  space: "Space",
  escape: "esc",
  enter: "↩",
  return: "↩",
  slash: "/",
};

const glyph = (part: string) => {
  const key = part.toLowerCase().replace(/_(left|right)$/, "");
  return GLYPHS[key] ?? (key.length === 1 ? key.toUpperCase() : part);
};

// macOS writes chords in this order: ⌃⌥⇧⌘, then the key.
const ORDER = [
  "ctrl",
  "control",
  "option",
  "alt",
  "shift",
  "command",
  "cmd",
  "meta",
  "super",
];
const rank = (part: string) => {
  const i = ORDER.indexOf(part.toLowerCase().replace(/_(left|right)$/, ""));
  return i === -1 ? ORDER.length : i;
};

/** "option+shift+space" rendered as physical keycaps. */
export const Keys: React.FC<{
  binding?: string;
  className?: string;
  large?: boolean;
  pressed?: boolean;
}> = ({ binding, className = "", large = false, pressed = false }) => {
  if (!binding) return null;
  const parts = binding
    .split("+")
    .filter(Boolean)
    .sort((a, b) => rank(a) - rank(b));
  return (
    <span
      className={`inline-flex items-center align-middle ${large ? "gap-2" : "gap-1"} ${className}`}
    >
      {parts.map((part, i) => (
        <kbd
          key={i}
          className={`loba-key transition-transform duration-75 ${large ? "loba-key-lg" : ""}`}
          data-pressed={pressed}
        >
          {glyph(part)}
        </kbd>
      ))}
    </span>
  );
};
