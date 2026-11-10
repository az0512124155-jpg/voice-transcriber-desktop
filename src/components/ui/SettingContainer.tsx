import React from "react";
import { InfoTooltip } from "./InfoTooltip";

interface SettingContainerProps {
  title: string;
  description: string;
  children: React.ReactNode;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  layout?: "horizontal" | "stacked";
  disabled?: boolean;
  tooltipPosition?: "top" | "bottom";
  /** Horizontal only: the control takes the row's free width, for content that fits itself to it. */
  grow?: boolean;
}

/**
 * One settings row: a title (with an optional info bubble), an optional
 * description, and the control on the trailing edge.
 *
 * `grouped` is kept for call-site compatibility; the row is styled the same
 * either way, because the card around it now draws the dividers.
 */
export const SettingContainer: React.FC<SettingContainerProps> = ({
  title,
  description,
  children,
  descriptionMode = "tooltip",
  layout = "horizontal",
  disabled = false,
  tooltipPosition = "top",
  grow = false,
}) => {
  const info = (
    <span className="vt-row__info">
      <InfoTooltip
        text={description}
        placement={tooltipPosition === "bottom" ? "below" : "above"}
      />
    </span>
  );

  if (layout === "stacked") {
    return (
      <div
        className={`vt-row vt-row--stacked${disabled ? " vt-row--disabled" : ""}`}
      >
        <h3 className="vt-row__title">
          {title}
          {descriptionMode === "tooltip" && info}
        </h3>
        {descriptionMode === "inline" && (
          <p className="vt-row__description">{description}</p>
        )}
        <div className="vt-row__control vt-row__control--grow">{children}</div>
      </div>
    );
  }

  return (
    <div className={`vt-row${disabled ? " vt-row--disabled" : ""}`}>
      <div className="vt-row__text">
        <h3 className="vt-row__title">
          {title}
          {descriptionMode === "tooltip" && info}
        </h3>
        {descriptionMode === "inline" && (
          <p className="vt-row__description">{description}</p>
        )}
      </div>
      <div
        className={`vt-row__control${grow ? " vt-row__control--grow" : ""}`}
      >
        {children}
      </div>
    </div>
  );
};
