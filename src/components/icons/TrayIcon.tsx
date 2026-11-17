/** The Bolke menu bar icon (design/logo/bolke-menubar-icon.svg and
 * src-tauri/resources/tray_*.png): the top bar with the orb hanging from it.
 * Uses currentColor like a macOS template image. */
type TrayIconState = "idle" | "recording" | "transcribing";

/** A circle as a path, so holes can be cut with the even-odd rule. */
const circle = (cx: number, cy: number, r: number) =>
  `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;

const ORB = { cx: 12, cy: 14.5 };

const TrayIcon = ({
  state,
  size,
}: {
  state: TrayIconState;
  size: number | string;
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <path
      d="M3 6.5H21"
      stroke="currentColor"
      strokeWidth="2.4"
      strokeLinecap="round"
    />
    {state === "idle" && (
      <circle
        cx={ORB.cx}
        cy={ORB.cy}
        r="5.6"
        stroke="currentColor"
        strokeWidth="2.4"
      />
    )}
    {state === "recording" && (
      <circle cx={ORB.cx} cy={ORB.cy} r="6.8" fill="currentColor" />
    )}
    {state === "transcribing" && (
      <path
        fill="currentColor"
        fillRule="evenodd"
        d={[
          circle(ORB.cx, ORB.cy, 6.8),
          circle(9, ORB.cy, 1.15),
          circle(12, ORB.cy, 1.15),
          circle(15, ORB.cy, 1.15),
        ].join("")}
      />
    )}
  </svg>
);

export default TrayIcon;
