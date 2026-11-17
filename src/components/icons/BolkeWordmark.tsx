import { OrbShapes, useOrbId } from "./BolkeOrb";

/**
 * The Bolke wordmark (logo option B): lowercase "bolke" hanging from a
 * Hindi-style top bar, with the app orb as the "o". Copied from
 * design/logo/bolke-logo-*.svg; the letter ink follows the theme
 * (--color-wordmark-stroke) and the orb glow shows only in dark mode.
 */
const BolkeWordmark = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  const idPrefix = useOrbId();
  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox="0 0 560 200"
      overflow="visible"
      xmlns="http://www.w3.org/2000/svg"
      role="img"
      aria-label="bolke"
    >
      <g
        fill="none"
        stroke="var(--color-wordmark-stroke)"
        strokeWidth="13"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M40 16 V170" />
        <circle cx="93" cy="117" r="53" />
        <path d="M296 16 V170" />
        <path d="M326 16 V170" />
        <path d="M388 66 L328 120 L390 170" />
        <path d="M409 117 H515 A53 53 0 1 0 499.5 154.5" />
        <path d="M14 60 H546" />
      </g>
      <g transform="translate(215 117)">
        <OrbShapes idPrefix={idPrefix} />
      </g>
    </svg>
  );
};

export default BolkeWordmark;
