import React from "react";

interface MascotProps {
  /** Height in px. */
  size: number;
  /** Wave once on arrival, or high-five (re-key it to play again). */
  animation?: "wave" | "slap" | null;
}

/** The mark's own box, so the wave's pivot and the layout box match it. */
const BOX_ASPECT = 32 / 34;

/** Capsule of the microphone. */
const CAPSULE = { x: 10, y: 4.5, w: 12, h: 16, r: 6 };

/** The cradle: a U that holds the capsule, with the stand and foot below it. */
const CRADLE = "M7 15.5v2.2a9 9 0 0 0 18 0v-2.2";
const STAND = "M16 26.7v3.4";
const FOOT = "M12.4 30.1h7.2";

/**
 * The app's mascot: a microphone with a face, optionally animated.
 *
 * Deliberately not a speech bubble — the Try-It screen already draws one next
 * to it as its callout, and two bubbles side by side read as a mistake.
 */
export const Mascot: React.FC<MascotProps> = ({ size, animation = null }) => (
  <span
    style={{ width: size * BOX_ASPECT, height: size }}
    className={`relative block origin-[50%_90%] rotate-[-8deg] select-none text-logo-primary ${
      animation === "wave"
        ? "animate-wave"
        : animation === "slap"
          ? "animate-slap"
          : ""
    }`}
  >
    <svg
      viewBox="0 0 32 34"
      className="absolute inset-0 h-full w-full"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <g
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <rect
          x={CAPSULE.x}
          y={CAPSULE.y}
          width={CAPSULE.w}
          height={CAPSULE.h}
          rx={CAPSULE.r}
        />
        <path d={CRADLE} />
        <path d={STAND} />
        <path d={FOOT} />
      </g>
      <circle cx="13.9" cy="11.2" r="1.2" fill="currentColor" />
      <circle cx="18.1" cy="11.2" r="1.2" fill="currentColor" />
      <path
        d="M14.4 14.9c.45.62 1 .93 1.6.93s1.15-.31 1.6-.93"
        stroke="currentColor"
        strokeWidth="1.35"
        strokeLinecap="round"
      />
    </svg>
  </span>
);
