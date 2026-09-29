import React from "react";

/**
 * L.O.B.A wordmark (replaces Handy's logo everywhere it was used: onboarding
 * and the Engine sidebar). The app icon's stroke — a scribble settling into
 * a line — then the clay full stop, then the name in the system serif.
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
      <circle cx="61" cy="32" r="5.4" fill="var(--loba-clay, #b4502b)" />
      <text
        x="82"
        y="44"
        fill="currentColor"
        fontFamily='ui-serif, "New York", "Iowan Old Style", Georgia, serif'
        fontSize="36"
        fontWeight="500"
        letterSpacing="2"
      >
        L.O.B.A
      </text>
    </svg>
  );
};

export default HandyTextLogo;
