import React from "react";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?:
    | "primary"
    | "primary-soft"
    | "secondary"
    | "outline"
    | "warning"
    | "danger"
    | "danger-ghost"
    | "ghost";
  size?: "sm" | "md" | "lg";
}

export const Button: React.FC<ButtonProps> = ({
  children,
  className = "",
  variant = "primary",
  size = "md",
  ...props
}) => {
  const baseClasses =
    "font-medium rounded-full border inline-flex items-center justify-center gap-2 focus:outline-none transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer";

  const variantClasses = {
    primary:
      "text-white bg-linear-to-b from-[#3a78ff] to-[#2a5ff0] border-transparent shadow-[0_8px_30px_rgba(47,107,255,0.4)] hover:brightness-110 focus-visible:ring-2 focus-visible:ring-accent-soft",
    "primary-soft":
      "text-text bg-logo-primary/20 border-transparent hover:bg-logo-primary/20 focus:ring-1 focus:ring-logo-primary",
    secondary:
      "text-text bg-[rgba(20,30,64,0.6)] border-edge hover:border-[rgba(140,170,255,0.4)] focus-visible:ring-2 focus-visible:ring-accent-soft",
    // For screens where secondary's grey fill reads as disabled (onboarding)
    outline:
      "text-text bg-[rgba(20,30,64,0.6)] border-edge hover:border-[rgba(140,170,255,0.4)] focus-visible:ring-2 focus-visible:ring-accent-soft",
    // Secondary's neutral resting look, but hover/focus use the semantic
    // --color-warning token (theme.css) instead of the pink accent — for
    // buttons sitting on warning surfaces like SecureInputWarning
    warning:
      "text-text bg-wash border-line hover:bg-warning-wash hover:border-warning focus:ring-1 focus:ring-warning",
    danger:
      "text-background bg-error border-error hover:bg-error/80 hover:border-error/80 focus:ring-1 focus:ring-error",
    "danger-ghost":
      "text-error border-transparent hover:bg-error-wash focus:bg-error-wash",
    ghost:
      "text-current border-transparent hover:bg-wash hover:border-logo-primary focus:bg-line",
  };

  const sizeClasses = {
    sm: "px-3 py-1 text-xs",
    md: "px-4 py-[6px] text-sm",
    lg: "px-[22px] min-h-[44px] text-[14px]",
  };

  return (
    <button
      className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
};
