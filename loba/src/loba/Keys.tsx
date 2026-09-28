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
};

const glyph = (part: string) => {
  const key = part.toLowerCase().replace(/_(left|right)$/, "");
  return GLYPHS[key] ?? (key.length === 1 ? key.toUpperCase() : part);
};

/** "option+shift+space" rendered as physical keycaps. */
export const Keys: React.FC<{ binding?: string; className?: string }> = ({
  binding,
  className = "",
}) => {
  if (!binding) return null;
  return (
    <span
      className={`inline-flex items-center gap-1 align-middle ${className}`}
    >
      {binding.split("+").map((part, i) => (
        <kbd key={i} className="loba-key">
          {glyph(part)}
        </kbd>
      ))}
    </span>
  );
};
