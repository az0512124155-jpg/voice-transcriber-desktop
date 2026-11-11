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

/** The variant names are the call sites' vocabulary; the classes are the kit's. */
const VARIANT_CLASS: Record<NonNullable<ButtonProps["variant"]>, string> = {
  primary: "vt-btn--primary",
  // "Soft" used to be a tinted fill; on the current palette a bordered field
  // reads better than a wash, and it keeps contrast in both themes.
  "primary-soft": "vt-btn--secondary",
  secondary: "vt-btn--secondary",
  outline: "vt-btn--outline",
  warning: "vt-btn--warning",
  danger: "vt-btn--danger",
  "danger-ghost": "vt-btn--danger-ghost",
  ghost: "vt-btn--ghost",
};

const SIZE_CLASS: Record<NonNullable<ButtonProps["size"]>, string> = {
  sm: "vt-btn--sm",
  md: "",
  lg: "vt-btn--lg",
};

export const Button: React.FC<ButtonProps> = ({
  children,
  className = "",
  variant = "primary",
  size = "md",
  ...props
}) => (
  <button
    className={`vt-btn ${VARIANT_CLASS[variant]} ${SIZE_CLASS[size]} ${className}`}
    {...props}
  >
    {children}
  </button>
);
