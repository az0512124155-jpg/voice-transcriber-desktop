import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { Input } from "../ui/Input";
import { useSettings } from "../../hooks/useSettings";
import { RecordingRetentionPeriod } from "@/bindings";

interface RecordingRetentionPeriodProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

type SizeUnit = "KB" | "MB" | "GB" | "TB";

const SIZE_UNIT_EXPONENTS: Record<SizeUnit, number> = {
  KB: 1,
  MB: 2,
  GB: 3,
  TB: 4,
};

// Largest unit first, so a stored size reads back in the cleanest unit.
const SIZE_UNITS_DESC: SizeUnit[] = ["TB", "GB", "MB", "KB"];
const SIZE_UNITS: SizeUnit[] = ["KB", "MB", "GB", "TB"];

const DEFAULT_RETENTION_DAYS = 7;
const DEFAULT_RETENTION_MAX_BYTES = 5 * 1024 ** 3;

// Show 5 GiB as "5 GB" and 700 MiB as "700 MB" rather than "0.0049 GB".
const toDisplaySize = (bytes: number): { value: number; unit: SizeUnit } => {
  const safe = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;

  for (const unit of SIZE_UNITS_DESC) {
    const factor = 1024 ** SIZE_UNIT_EXPONENTS[unit];
    if (safe >= factor && safe % factor === 0) {
      return { value: safe / factor, unit };
    }
  }

  // Not evenly divisible (e.g. a value set outside this control): use the
  // largest unit that fits and round, so the field never shows a fraction.
  for (const unit of SIZE_UNITS_DESC) {
    const factor = 1024 ** SIZE_UNIT_EXPONENTS[unit];
    if (safe >= factor) {
      return { value: Math.max(1, Math.round(safe / factor)), unit };
    }
  }

  return { value: 1, unit: "KB" };
};

export const RecordingRetentionPeriodSelector: React.FC<RecordingRetentionPeriodProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const selectedRetentionPeriod =
      getSetting("recording_retention_period") || "never";
    const historyLimit = getSetting("history_limit") || 5;
    const storedDays =
      getSetting("recording_retention_days") ?? DEFAULT_RETENTION_DAYS;
    const storedBytes =
      getSetting("recording_retention_max_bytes") ??
      DEFAULT_RETENTION_MAX_BYTES;

    // Drafts are committed on blur / Enter, never per keystroke.
    const [daysDraft, setDaysDraft] = useState(String(storedDays));
    const [sizeDraft, setSizeDraft] = useState(
      String(toDisplaySize(storedBytes).value),
    );
    const [sizeUnit, setSizeUnit] = useState<SizeUnit>(
      toDisplaySize(storedBytes).unit,
    );

    // Keep the drafts in step with the backend (e.g. after a settings refresh).
    useEffect(() => {
      setDaysDraft(String(storedDays));
    }, [storedDays]);

    useEffect(() => {
      const display = toDisplaySize(storedBytes);
      setSizeDraft(String(display.value));
      setSizeUnit(display.unit);
    }, [storedBytes]);

    const handleRetentionPeriodSelect = async (period: string) => {
      await updateSetting(
        "recording_retention_period",
        period as RecordingRetentionPeriod,
      );
    };

    const commitDays = async () => {
      const parsed = Math.floor(Number(daysDraft));
      if (!Number.isFinite(parsed) || parsed < 1) {
        setDaysDraft(String(storedDays));
        return;
      }
      if (parsed !== storedDays) {
        await updateSetting("recording_retention_days", parsed);
      } else {
        setDaysDraft(String(parsed));
      }
    };

    const commitSize = async (unit: SizeUnit = sizeUnit) => {
      const parsed = Math.floor(Number(sizeDraft));
      if (!Number.isFinite(parsed) || parsed < 1) {
        const display = toDisplaySize(storedBytes);
        setSizeDraft(String(display.value));
        setSizeUnit(display.unit);
        return;
      }
      const bytes = parsed * 1024 ** SIZE_UNIT_EXPONENTS[unit];
      if (bytes !== storedBytes) {
        await updateSetting("recording_retention_max_bytes", bytes);
      }
    };

    const handleUnitSelect = async (unit: string) => {
      const nextUnit = unit as SizeUnit;
      setSizeUnit(nextUnit);
      await commitSize(nextUnit);
    };

    const commitOnEnter = (event: React.KeyboardEvent<HTMLInputElement>) => {
      if (event.key === "Enter") {
        event.currentTarget.blur();
      }
    };

    const retentionOptions = [
      { value: "never", label: t("settings.debug.recordingRetention.never") },
      {
        value: "preserve_limit",
        label: t("settings.debug.recordingRetention.preserveLimit", {
          count: Number(historyLimit),
        }),
      },
      { value: "days_3", label: t("settings.debug.recordingRetention.days3") },
      { value: "weeks_2", label: t("settings.debug.recordingRetention.weeks2") },
      {
        value: "months_3",
        label: t("settings.debug.recordingRetention.months3"),
      },
      {
        value: "custom_days",
        label: t("settings.debug.recordingRetention.customDays"),
      },
      {
        value: "custom_size",
        label: t("settings.debug.recordingRetention.customSize"),
      },
    ];

    const unitOptions = SIZE_UNITS.map((unit) => ({
      value: unit,
      label: unit,
    }));

    return (
      <SettingContainer
        title={t("settings.debug.recordingRetention.title")}
        description={t("settings.debug.recordingRetention.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex w-80 flex-col gap-2">
          <Dropdown
            options={retentionOptions}
            selectedValue={selectedRetentionPeriod}
            onSelect={handleRetentionPeriodSelect}
            placeholder={t("settings.debug.recordingRetention.placeholder")}
            disabled={isUpdating("recording_retention_period")}
          />

          {selectedRetentionPeriod === "custom_days" && (
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min={1}
                step={1}
                variant="compact"
                value={daysDraft}
                onChange={(event) => setDaysDraft(event.target.value)}
                onBlur={commitDays}
                onKeyDown={commitOnEnter}
                disabled={isUpdating("recording_retention_days")}
                className="w-24"
                aria-label={t("settings.debug.recordingRetention.customDays")}
              />
              <span className="text-sm text-ink-3">
                {t("settings.debug.recordingRetention.daysUnit")}
              </span>
            </div>
          )}

          {selectedRetentionPeriod === "custom_size" && (
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min={1}
                step={1}
                variant="compact"
                value={sizeDraft}
                onChange={(event) => setSizeDraft(event.target.value)}
                onBlur={() => commitSize()}
                onKeyDown={commitOnEnter}
                disabled={isUpdating("recording_retention_max_bytes")}
                className="w-24"
                aria-label={t("settings.debug.recordingRetention.customSize")}
              />
              <Dropdown
                options={unitOptions}
                selectedValue={sizeUnit}
                onSelect={handleUnitSelect}
                tone="neutral"
                disabled={isUpdating("recording_retention_max_bytes")}
              />
            </div>
          )}
        </div>
      </SettingContainer>
    );
  });

RecordingRetentionPeriodSelector.displayName =
  "RecordingRetentionPeriodSelector";
