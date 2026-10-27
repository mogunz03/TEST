import React from "react";

/**
 * L.O.B.A wordmark (replaces Handy's logo everywhere it was used): the app
 * icon's mark, sound bars shrinking into a full stop (speech becoming
 * text), then the name in the system sans.
 */
const HandyTextLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox="0 0 300 64"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      role="img"
      aria-label="L.O.B.A"
    >
      <rect
        x="5"
        y="17.0"
        width="6"
        height="30"
        rx="3"
        fill="currentColor"
        fillOpacity={0.6}
      />
      <rect
        x="14"
        y="10.0"
        width="6"
        height="44"
        rx="3"
        fill="currentColor"
        fillOpacity={0.7}
      />
      <rect
        x="23"
        y="19.0"
        width="6"
        height="26"
        rx="3"
        fill="currentColor"
        fillOpacity={0.8}
      />
      <rect
        x="32"
        y="24.0"
        width="6"
        height="16"
        rx="3"
        fill="currentColor"
        fillOpacity={0.9}
      />
      <rect
        x="41"
        y="28.0"
        width="6"
        height="8"
        rx="3"
        fill="currentColor"
        fillOpacity={1.0}
      />
      <circle cx="56" cy="32" r="5" fill="var(--loba-steel, #b9c4d2)" />
      <text
        x="82"
        y="44"
        fill="currentColor"
        fontFamily='-apple-system, BlinkMacSystemFont, "SF Pro Display", "Inter", system-ui, sans-serif'
        fontSize="34"
        fontWeight="600"
        letterSpacing="3"
      >
        L.O.B.A
      </text>
    </svg>
  );
};

export default HandyTextLogo;
