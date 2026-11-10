import React from "react";

interface SettingsGroupProps {
  title?: string;
  description?: string;
  children: React.ReactNode;
}

/** A titled card of settings rows. The card draws its own dividers, so the
 *  rows inside do not need a border of their own. */
export const SettingsGroup: React.FC<SettingsGroupProps> = ({
  title,
  description,
  children,
}) => {
  return (
    <section className="vt-group">
      {title && <h2 className="vt-group__title">{title}</h2>}
      {description && <p className="vt-group__description">{description}</p>}
      <div className="vt-group__card">{children}</div>
    </section>
  );
};
