import React from "react";

/** A setting as a sentence: label and note on the left, control on the right,
 *  separated from the next by a hairline rather than boxed. */
export const Row: React.FC<{
  label: string;
  note?: React.ReactNode;
  children: React.ReactNode;
}> = ({ label, note, children }) => (
  <div className="flex items-start justify-between gap-8 py-5 border-b border-rule last:border-b-0">
    <div className="min-w-0">
      <div className="text-[14px] font-medium">{label}</div>
      {note && (
        <div className="mt-1 text-[12.5px] leading-5 text-soft">{note}</div>
      )}
    </div>
    <div className="shrink-0 pt-0.5">{children}</div>
  </div>
);

export const Switch: React.FC<{
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
}> = ({ checked, onChange, label }) => (
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    onClick={() => onChange(!checked)}
    className={`relative h-[18px] w-[30px] rounded-full transition-colors duration-200 cursor-pointer ${
      checked ? "bg-clay" : "bg-rule"
    }`}
  >
    {/* Slight overshoot on the thumb: it lands, it doesn't snap. */}
    <span
      className={`absolute top-[2px] start-[2px] h-[14px] w-[14px] rounded-full bg-sheet shadow-sm transition-transform duration-[260ms] ease-[cubic-bezier(0.34,1.56,0.64,1)] ${
        checked ? "translate-x-[12px] rtl:-translate-x-[12px]" : ""
      }`}
    />
  </button>
);

export const PageTitle: React.FC<{ title: string; lede?: string }> = ({
  title,
  lede,
}) => (
  <header className="mb-6">
    <h1 className="loba-display text-[28px]">{title}</h1>
    {lede && (
      <p className="mt-2 text-[14px] leading-6 text-soft max-w-[440px]">
        {lede}
      </p>
    )}
  </header>
);
