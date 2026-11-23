import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { PhoneLinkCard } from "../PhoneLinkCard";

/**
 * Dedicated section for the phone bridge.
 *
 * It used to live at the bottom of Advanced settings, behind the experimental
 * toggle and below ten other groups, so the start button sat past the fold.
 * A section of its own puts the code and the button one click from the sidebar.
 */
export const LectureNotebookSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="vt-stack">
      <SettingsGroup title={t("settings.notebook.link.title")}>
        <PhoneLinkCard />
      </SettingsGroup>

      <SettingsGroup title={t("settings.notebook.steps.title")}>
        {(["one", "two", "three"] as const).map((step) => (
          <div className="vt-row vt-row--stacked" key={step}>
            <p className="vt-row__title">
              {t(`settings.notebook.steps.${step}.title`)}
            </p>
            <p className="vt-row__description">
              {t(`settings.notebook.steps.${step}.description`)}
            </p>
          </div>
        ))}
        <div className="vt-row vt-row--stacked">
          <p className="vt-row__description">
            {t("settings.notebook.steps.note")}
          </p>
        </div>
      </SettingsGroup>
    </div>
  );
};
