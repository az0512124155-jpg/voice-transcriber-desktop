import React from "react";
import { AlertCircle, AlertTriangle, Info, CheckCircle } from "lucide-react";

type AlertVariant = "error" | "warning" | "info" | "success";

interface AlertProps {
  variant?: AlertVariant;
  /** When true, removes rounded corners for use inside containers */
  contained?: boolean;
  children: React.ReactNode;
  className?: string;
}

const variantIcons: Record<AlertVariant, React.ElementType> = {
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
  success: CheckCircle,
};

export const Alert: React.FC<AlertProps> = ({
  variant = "error",
  contained = false,
  children,
  className = "",
}) => {
  const Icon = variantIcons[variant];

  return (
    <div
      className={`vt-alert vt-alert--${variant}${contained ? " vt-alert--contained" : ""} ${className}`}
    >
      <Icon className="vt-alert__icon" />
      <p className="vt-alert__body">{children}</p>
    </div>
  );
};
