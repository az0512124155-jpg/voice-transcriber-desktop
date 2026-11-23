import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { PhoneLinkCard } from "../PhoneLinkCard";
import { MicrophoneSelector } from "../MicrophoneSelector";
import { LanguageModeSetting } from "../LanguageMode";
import { VoiceActivityDetection } from "../VoiceActivityDetection";
import { FillerWordRemoval } from "../FillerWordRemoval";
import { PostProcessingSettingsApi } from "../post-processing/PostProcessingSettings";

/**
 * One place for everything the lecture notebook needs.
 *
 * These controls also exist in General, Advanced and Post-processing, where
 * they belong to dictation. Duplicating them here is deliberate: a student
 * setting up a lecture should not have to know that the microphone lives in
 * General, silence handling in Advanced and the AI provider in Post-processing.
 * Both copies write the same setting, so they never drift.
 */
export const LectureNotebookSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="vt-stack">
      <SettingsGroup title={t("settings.notebook.link.title")}>
        <PhoneLinkCard />
      </SettingsGroup>

      <SettingsGroup title={t("settings.notebook.recording.title")}>
        <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
        <LanguageModeSetting descriptionMode="tooltip" grouped={true} />
        <VoiceActivityDetection descriptionMode="tooltip" grouped={true} />
        <FillerWordRemoval descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.notebook.ai.title")}>
        <PostProcessingSettingsApi />
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
