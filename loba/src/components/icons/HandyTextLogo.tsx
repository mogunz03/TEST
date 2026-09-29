import React from "react";

/**
 * L.O.B.A wordmark (replaces Handy's logo everywhere it was used). The app
 * icon's stroke, a scribble settling into a line, then the full stop, then
 * the name in the system sans.
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
      <path
        d="M4,32 C7.75,32 7.75,21 11.5,21 C15.25,21 15.25,54 19,54 C22.75,54 22.75,26 26.5,26 C30.25,26 30.25,32 34,32 L50,32"
        stroke="currentColor"
        strokeWidth="4.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="61" cy="32" r="5.4" fill="var(--loba-steel, #b9c4d2)" />
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
