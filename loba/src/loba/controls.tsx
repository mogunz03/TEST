import React from "react";

/** A group of settings on one card, with an optional label above it. */
export const Group: React.FC<{
  label?: string;
  children: React.ReactNode;
  className?: string;
}> = ({ label, children, className = "" }) => (
  <section className={`mt-7 first:mt-0 ${className}`}>
    {label && <h2 className="loba-label mb-2 ps-1">{label}</h2>}
    <div className="loba-card px-5">{children}</div>
  </section>
);

/** One setting: label and note on the left, control on the right. Rows on
 *  a card are split by a hairline, never boxed individually. */
export const Row: React.FC<{
  label: string;
  note?: React.ReactNode;
  children: React.ReactNode;
}> = ({ label, note, children }) => (
  <div className="flex items-center justify-between gap-8 py-4 border-b border-line last:border-b-0">
    <div className="min-w-0">
      <div className="text-[13.5px] font-medium text-ink">{label}</div>
      {note && (
        <div className="mt-1 text-[12.5px] leading-[18px] text-soft max-w-[380px]">
          {note}
        </div>
      )}
    </div>
    <div className="shrink-0">{children}</div>
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
    className={`relative h-[20px] w-[34px] rounded-full border transition-colors duration-200 cursor-pointer ${
      checked ? "bg-ink border-ink" : "bg-raised border-line-strong"
    }`}
  >
    {/* Slight overshoot on the thumb: it lands, it doesn't snap. */}
    <span
      className={`absolute top-[2px] start-[2px] h-[14px] w-[14px] rounded-full transition-[transform,background-color] duration-[260ms] ease-[cubic-bezier(0.34,1.56,0.64,1)] ${
        checked
          ? "bg-void translate-x-[14px] rtl:-translate-x-[14px]"
          : "bg-soft"
      }`}
    />
  </button>
);

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
}: {
  options: { id: T; label: string }[];
  value: T;
  onChange: (next: T) => void;
  label: string;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="flex rounded-[9px] bg-void border border-line p-[3px] text-[12.5px]"
    >
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          role="radio"
          aria-checked={value === o.id}
          onClick={() => onChange(o.id)}
          className={`px-3 h-[24px] rounded-[6px] transition-colors cursor-pointer ${
            value === o.id
              ? "bg-raised text-ink shadow-[0_1px_0_rgb(255_255_255/0.04)_inset]"
              : "text-soft hover:text-ink"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export const PageTitle: React.FC<{
  title: string;
  lede?: string;
  children?: React.ReactNode;
}> = ({ title, lede, children }) => (
  <header className="mb-7 flex items-end justify-between gap-6">
    <div>
      <h1 className="loba-title text-[24px] text-ink">{title}</h1>
      {lede && (
        <p className="mt-1.5 text-[13.5px] leading-5 text-soft max-w-[460px]">
          {lede}
        </p>
      )}
    </div>
    {children && <div className="shrink-0">{children}</div>}
  </header>
);

export const Empty: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <div className="loba-card px-6 py-10 text-center text-[13px] text-soft">
    {children}
  </div>
);
