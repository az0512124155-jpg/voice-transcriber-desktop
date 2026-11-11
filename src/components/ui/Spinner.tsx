import React from "react";

/** Small spinner for waiting on the system. */
export const Spinner: React.FC<{ className?: string }> = ({
  className = "",
}) => <span aria-hidden="true" className={`vt-spinner ${className}`} />;
