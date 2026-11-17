import { useId } from "react";

/**
 * The Bolke orb, drawn in the orb's own 56-unit space (centre 0,0). This is
 * the exact orb from design/logo/; the wordmark's "o" and the app icon use
 * the same shapes, so change all of them together or not at all.
 *
 * `idPrefix` keeps gradient/filter ids unique when several orbs share a page.
 */
export const OrbShapes = ({
  idPrefix,
  glow = true,
  animated = false,
}: {
  idPrefix: string;
  glow?: boolean;
  /** Slowly turn the inner swirl (only for a standalone orb). */
  animated?: boolean;
}) => {
  const id = (name: string) => `${idPrefix}-${name}`;
  return (
    <>
      <defs>
        <radialGradient
          id={id("fill")}
          gradientUnits="userSpaceOnUse"
          cx="0"
          cy="6.7"
          r="84"
        >
          <stop offset="0" stopColor="#96CDFF" stopOpacity="0.95" />
          <stop offset="0.16" stopColor="#3F8CFF" stopOpacity="0.9" />
          <stop offset="0.42" stopColor="#1E46C8" stopOpacity="0.92" />
          <stop offset="0.74" stopColor="#0A1650" />
        </radialGradient>
        <radialGradient id={id("hi")} cx="0.5" cy="0.5" r="0.5">
          <stop offset="0" stopColor="#FFFFFF" stopOpacity="0.6" />
          <stop offset="0.7" stopColor="#FFFFFF" stopOpacity="0" />
        </radialGradient>
        <clipPath id={id("clip")}>
          <circle cx="0" cy="0" r="56" />
        </clipPath>
        <filter id={id("blur")} x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation="5" />
        </filter>
        <filter id={id("glow")} x="-100%" y="-100%" width="300%" height="300%">
          <feGaussianBlur stdDeviation="16" />
        </filter>
      </defs>
      {glow && (
        <circle
          r="56"
          fill="#2F6BFF"
          style={{ opacity: "var(--orb-glow, 0.5)" }}
          filter={`url(#${id("glow")})`}
        />
      )}
      <circle r="56" fill="#070A14" />
      <circle r="56" fill={`url(#${id("fill")})`} />
      <g clipPath={`url(#${id("clip")})`}>
        <circle
          r="56"
          fill="none"
          stroke="#82BEFF"
          strokeOpacity="0.4"
          strokeWidth="18"
          filter={`url(#${id("blur")})`}
        />
        <g
          fill="none"
          strokeLinecap="round"
          filter={`url(#${id("blur")})`}
          className={animated ? "orb-swirl" : undefined}
        >
          <path
            d="M-35 -22 C-15 -37 25 -33 37 -9"
            stroke="#96D2FF"
            strokeOpacity="0.6"
            strokeWidth="7"
          />
          <path
            d="M37 23 C21 43 -19 43 -33 19"
            stroke="#5A96FF"
            strokeOpacity="0.5"
            strokeWidth="7"
          />
          <path
            d="M-25 9 C-10 -7 13 -5 25 7"
            stroke="#B4DCFF"
            strokeOpacity="0.45"
            strokeWidth="4"
          />
        </g>
      </g>
      <ellipse
        cx="-13.4"
        cy="-35.3"
        rx="15.7"
        ry="8.4"
        fill={`url(#${id("hi")})`}
      />
    </>
  );
};

/** A safe id prefix from React's useId (which contains colons). */
export const useOrbId = () => `orb${useId().replace(/[^a-zA-Z0-9]/g, "")}`;

/** The orb on its own, e.g. as a small icon or onboarding guide. */
const BolkeOrb = ({
  width,
  height,
  className,
  glow = false,
  animated = false,
}: {
  width?: number | string;
  height?: number | string;
  className?: string;
  glow?: boolean;
  /** Breathe and turn the swirl, like the orb in the wireframes. */
  animated?: boolean;
}) => {
  const idPrefix = useOrbId();
  return (
    <svg
      width={width ?? 24}
      height={height ?? width ?? 24}
      className={`${animated ? "orb-breathe " : ""}${className ?? ""}`}
      viewBox="-60 -60 120 120"
      overflow="visible"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <OrbShapes idPrefix={idPrefix} glow={glow} animated={animated} />
    </svg>
  );
};

export default BolkeOrb;
