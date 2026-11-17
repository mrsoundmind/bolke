import React from "react";
import {
  bindingKeys,
  isOnlyModifiers,
  keycapLabel,
  keycapLabels,
  type OSType,
} from "@/lib/utils/keyboard";

interface KeycapsProps {
  /** The binding to draw, e.g. "option+space". */
  binding: string;
  osType: OSType;
  /** sm: standalone; screen: Demo's drawn screen; line: inline in body text;
   * hero: Home's "Your shortcut" card. */
  size?: "sm" | "screen" | "line" | "hero";
  /** Draw every key as held down. */
  pressed?: boolean;
  /** A combination still being captured: dashed, no drop. */
  partial?: boolean;
  /** Raise the keys on hover/focus of a parent `group/keycaps` control. */
  lift?: boolean;
  className?: string;
}

const SIZES = {
  sm: "h-[30px] min-w-[30px] px-[9px] rounded-[7px] text-small",
  screen: "h-[24px] min-w-[24px] px-[7px] rounded-[6px] text-[11px]",
  line: "h-[25px] min-w-[25px] px-[7px] rounded-[6px] text-[13px]",
  hero: "h-[46px] min-w-[52px] px-[16px] rounded-[12px] text-[17px]",
};

/** A shortcut binding drawn as physical keycaps. */
export const Keycaps: React.FC<KeycapsProps> = ({
  binding,
  osType,
  size = "sm",
  pressed = false,
  partial = false,
  lift = false,
  className = "",
}) => {
  const labels = keycapLabels(binding, osType);
  // Bolke keycaps: raised navy keys with a soft blue edge (design/wireframes).
  const state = pressed
    ? "translate-y-[2px] shadow-none bg-background-ui text-white border-background-ui"
    : partial
      ? "translate-y-[2px] shadow-none border-dashed bg-raised text-text border-accent-soft"
      : `shadow-[0_2px_0_0_rgba(140,170,255,0.3)] bg-raised text-text border-[rgba(140,170,255,0.25)] ${
          lift
            ? "group-hover/keycaps:-translate-y-[2px] group-hover/keycaps:shadow-[0_4px_0_0_rgba(140,170,255,0.35)] group-focus-visible/keycaps:-translate-y-[2px] group-focus-visible/keycaps:shadow-[0_4px_0_0_rgba(140,170,255,0.35)]"
            : ""
        }`;

  return (
    <span
      className={`inline-flex items-center gap-[4px] align-middle ${className}`}
      role="img"
      aria-label={keycapLabel(binding, osType)}
    >
      {labels.map((label, i) => (
        <span
          key={i}
          aria-hidden="true"
          className={`relative inline-flex items-center justify-center border font-display font-medium leading-none select-none transition-[transform,box-shadow,background-color] duration-[70ms] ease-out ${SIZES[size]} ${state}`}
        >
          {label}
        </span>
      ))}
    </span>
  );
};

const SLOT_SIZES = {
  sm: "h-[30px] min-w-[88px] px-[9px] rounded-[7px] text-small",
  line: "h-[25px] min-w-[64px] px-[8px] rounded-[6px] text-[13px]",
};

/** Placeholder key shown while a shortcut is being captured. */
export const KeySlot: React.FC<
  React.HTMLAttributes<HTMLSpanElement> & { size?: "sm" | "line" }
> = ({ size = "sm", className = "", children, ...rest }) => (
  <span
    aria-hidden={children ? undefined : true}
    {...rest}
    className={`inline-flex translate-y-[3px] items-center justify-center whitespace-nowrap border border-dashed border-plum-pick font-display font-semibold leading-none text-ink-2 animate-key-slot motion-reduce:animate-none ${SLOT_SIZES[size]} ${className}`}
  >
    {children}
  </span>
);

/** The instruction while capturing: modifiers alone still need a key. */
export const captureHintKey = (partial: string) =>
  isOnlyModifiers(bindingKeys(partial))
    ? "onboarding.demo.addKey"
    : "onboarding.demo.pressKeys";

/** While capturing: the keys pressed so far, dashed, or an empty slot before any. */
export const CapturedKeys: React.FC<{
  partial: string;
  osType: OSType;
  size?: "sm" | "line";
  /** On the keys and the slot alike. */
  className?: string;
  keysClassName?: string;
  slotClassName?: string;
  /** Words in the empty slot, announced. */
  slotLabel?: string;
}> = ({
  partial,
  osType,
  size = "sm",
  className = "",
  keysClassName = "",
  slotClassName = "",
  slotLabel,
}) =>
  bindingKeys(partial).length ? (
    <Keycaps
      binding={partial}
      osType={osType}
      size={size}
      partial
      className={`${className} ${keysClassName}`}
    />
  ) : (
    <KeySlot
      size={size}
      role={slotLabel ? "status" : undefined}
      className={`${className} ${slotClassName}`}
    >
      {slotLabel}
    </KeySlot>
  );
