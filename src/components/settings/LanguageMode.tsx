import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { LanguageMode } from "@/bindings";

interface LanguageModeProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const LanguageModeSetting: React.FC<LanguageModeProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const options = [
      {
        value: "hebrew",
        label: t("settings.general.languageMode.options.hebrew.label"),
        description: t(
          "settings.general.languageMode.options.hebrew.description",
        ),
      },
      {
        value: "auto",
        label: t("settings.general.languageMode.options.auto.label"),
        description: t(
          "settings.general.languageMode.options.auto.description",
        ),
      },
    ];

    const selected = (getSetting("language_mode") || "auto") as LanguageMode;

    return (
      <SettingContainer
        title={t("settings.general.languageMode.title")}
        description={t("settings.general.languageMode.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={options}
          menuClassName="right-0 w-80 max-w-[calc(100vw-2rem)]"
          selectedValue={selected}
          onSelect={(value) =>
            updateSetting("language_mode", value as LanguageMode)
          }
          disabled={isUpdating("language_mode")}
        />
      </SettingContainer>
    );
  },
);

LanguageModeSetting.displayName = "LanguageModeSetting";
