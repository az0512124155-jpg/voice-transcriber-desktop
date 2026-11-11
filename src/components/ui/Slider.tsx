import React from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "./SettingContainer";
import { ResetButton } from "./ResetButton";

interface SliderProps {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  disabled?: boolean;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  showValue?: boolean;
  formatValue?: (value: number) => string;
  onReset?: () => void;
  isResetting?: boolean;
}

export const Slider: React.FC<SliderProps> = ({
  value,
  onChange,
  min,
  max,
  step = 0.01,
  disabled = false,
  label,
  description,
  descriptionMode = "tooltip",
  grouped = false,
  showValue = true,
  formatValue = (v) => v.toFixed(2),
  onReset,
  isResetting = false,
}) => {
  const { t } = useTranslation();
  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    onChange(parseFloat(e.target.value));
  };

  // The filled part of the track is a plain two-stop gradient on the element
  // itself — no color-mix, so it survives an old engine.
  const percent = max === min ? 0 : ((value - min) / (max - min)) * 100;

  return (
    <SettingContainer
      title={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="horizontal"
      disabled={disabled}
    >
      <div className="vt-range-row">
        <input
          type="range"
          aria-label={label}
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={handleChange}
          disabled={disabled}
          className="vt-range"
          style={{
            background: `linear-gradient(to right, var(--color-background-ui) ${percent}%, var(--color-line) ${percent}%)`,
          }}
        />
        {showValue && (
          <span className="vt-range-value">{formatValue(value)}</span>
        )}
        {onReset && (
          <ResetButton
            onClick={onReset}
            disabled={disabled || isResetting}
            ariaLabel={t("common.resetSetting", { setting: label })}
          />
        )}
      </div>
    </SettingContainer>
  );
};
