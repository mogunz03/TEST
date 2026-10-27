import React from "react";

/** Just enough Markdown for a recap: headings, bullets, numbered items,
 *  bold. Built as React elements, never as HTML strings. */
const inline = (text: string): React.ReactNode[] =>
  text
    .split(/(\*\*[^*]+\*\*)/g)
    .filter(Boolean)
    .map((part, i) =>
      part.startsWith("**") && part.endsWith("**") ? (
        <strong key={i} className="font-semibold text-ink">
          {part.slice(2, -2)}
        </strong>
      ) : (
        <React.Fragment key={i}>{part}</React.Fragment>
      ),
    );

export const Markdown: React.FC<{ text: string }> = ({ text }) => {
  const blocks: React.ReactNode[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;
  const flush = () => {
    if (!list) return;
    const Tag = list.ordered ? "ol" : "ul";
    blocks.push(
      <Tag
        key={blocks.length}
        className={`mt-2 mb-3 ps-5 flex flex-col gap-1.5 ${list.ordered ? "list-decimal" : "list-disc"} marker:text-faint`}
      >
        {list.items.map((it, i) => (
          <li key={i}>{inline(it)}</li>
        ))}
      </Tag>,
    );
    list = null;
  };
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    const bullet = line.match(/^[-*]\s+(.*)$/);
    const numbered = line.match(/^\d+[.)]\s+(.*)$/);
    if (bullet || numbered) {
      const ordered = !!numbered;
      if (list && list.ordered !== ordered) flush();
      list ??= { ordered, items: [] };
      list.items.push((bullet ?? numbered)![1]);
      continue;
    }
    flush();
    if (!line) continue;
    const heading = line.match(/^#{1,4}\s+(.*)$/);
    if (heading) {
      blocks.push(
        <h3
          key={blocks.length}
          className="mt-5 first:mt-0 mb-1 text-[12px] font-medium text-faint"
        >
          {heading[1]}
        </h3>,
      );
    } else {
      blocks.push(
        <p key={blocks.length} className="mb-2">
          {inline(line)}
        </p>,
      );
    }
  }
  flush();
  return (
    <div className="text-[13.5px] leading-[21px] text-ink select-text">
      {blocks}
    </div>
  );
};
