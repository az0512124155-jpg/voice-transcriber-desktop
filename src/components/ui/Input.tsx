import React from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

export const Input: React.FC<InputProps> = ({
  className = "",
  variant = "default",
  disabled,
  ...props
}) => (
  <input
    className={`vt-input${variant === "compact" ? " vt-input--compact" : ""} ${className}`}
    disabled={disabled}
    {...props}
  />
);
