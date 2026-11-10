/**
 * The product lockup: the neon speech-bubble mark from the phone app, plus
 * the name.
 *
 * The mark keeps its own cyan→violet gradient rather than following the UI
 * palette — it is the brand, not a themed control, and the glow is what makes
 * it recognisable at 24px in a sidebar.
 */

const MARK =
  "M9 4h14a6 6 0 0 1 6 6v7a6 6 0 0 1-6 6h-2.5L24 29l-7.5-6H9a6 6 0 0 1-6-6v-7a6 6 0 0 1 6-6Z";

/** Bars, centred on y = 14, tallest in the middle. */
const BARS: Array<[number, number, number]> = [
  [7.6, 11.6, 4.8],
  [11.2, 8.6, 10.8],
  [14.8, 6.6, 14.8],
  [18.4, 9.4, 9.2],
  [22.0, 11.8, 4.4],
];

interface AppLogoProps {
  /** Overall width of the lockup in px. */
  width?: number;
  className?: string;
  /** Stack the name under the mark instead of beside it. */
  stacked?: boolean;
  /** Hide the name and show only the mark. */
  markOnly?: boolean;
}

const AppLogo = ({
  width = 120,
  className,
  stacked = false,
  markOnly = false,
}: AppLogoProps) => {
  const markSize = markOnly ? width : stacked ? width * 0.55 : width * 0.2;
  const nameSize = stacked ? width * 0.15 : width * 0.1;

  const mark = (
    <svg
      width={markSize}
      height={markSize}
      viewBox="0 0 32 32"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      style={{ filter: "drop-shadow(0 0 6px rgba(122,160,255,0.45))" }}
      aria-hidden="true"
      className="shrink-0"
    >
      <defs>
        <linearGradient id="vt-logo-grad" x1="4" y1="6" x2="28" y2="27">
          <stop offset="0%" stopColor="#5fe0e8" />
          <stop offset="55%" stopColor="#7aa0ff" />
          <stop offset="100%" stopColor="#a66bf5" />
        </linearGradient>
      </defs>
      <path
        d={MARK}
        stroke="url(#vt-logo-grad)"
        strokeWidth="1.7"
        strokeLinejoin="round"
        fill="rgba(122,160,255,0.06)"
      />
      {BARS.map(([x, y, h]) => (
        <rect
          key={x}
          x={x}
          y={y}
          width={1.8}
          height={h}
          rx={0.9}
          fill="url(#vt-logo-grad)"
        />
      ))}
    </svg>
  );

  if (markOnly) {
    return <span className={className}>{mark}</span>;
  }

  const name = (
    <span
      className="font-semibold leading-none tracking-tight text-text"
      style={{ fontSize: nameSize }}
    >
      Voice Transcriber
    </span>
  );

  return (
    <span
      className={`inline-flex items-center ${stacked ? "flex-col gap-2" : "gap-2"} ${className ?? ""}`}
      aria-label="Voice Transcriber"
    >
      {mark}
      {name}
    </span>
  );
};

export default AppLogo;
