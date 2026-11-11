import React from "react";

interface BadgeProps {
  children: React.ReactNode;
  variant?: "primary" | "secondary";
  className?: string;
}

const Badge: React.FC<BadgeProps> = ({
  children,
  variant = "primary",
  className = "",
}) => {
  const variantClasses = {
    primary: "vt-badge--accent",
    secondary: "",
  };

  return (
    <span className={`vt-badge ${variantClasses[variant]} ${className}`}>
      {children}
    </span>
  );
};

export default Badge;
