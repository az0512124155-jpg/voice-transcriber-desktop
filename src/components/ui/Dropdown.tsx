import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export interface DropdownOption {
  value: string;
  label: React.ReactNode;
  description?: string;
  disabled?: boolean;
}

interface DropdownProps {
  options: DropdownOption[];
  className?: string;
  menuClassName?: string;
  selectedValue: string | null;
  onSelect: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  onOpen?: () => void;
  /** Where the menu opens: below the button (default), or above it. */
  placement?: "below" | "above";
  /** Highlight colour for hover and selection. */
  tone?: "brand" | "neutral";
}

/**
 * A select-shaped button that opens a list.
 *
 * The trigger is `.vt-field`, the same class the react-select control uses, so
 * every picker in the window is the same 30px box whether it is a native
 * react-select or this one. The `tone` prop survives for call sites that pass
 * it, but the menu now has one look: the brand tint marks the chosen row.
 */
export const Dropdown: React.FC<DropdownProps> = ({
  options,
  selectedValue,
  onSelect,
  className = "",
  menuClassName = "",
  placeholder = "Select an option...",
  disabled = false,
  onOpen,
  placement = "below",
}) => {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  const selectedOption = options.find(
    (option) => option.value === selectedValue,
  );

  const handleSelect = (value: string) => {
    onSelect(value);
    setIsOpen(false);
  };

  const handleToggle = () => {
    if (disabled) return;
    if (!isOpen) onOpen?.();
    setIsOpen(!isOpen);
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Escape" || !isOpen || disabled) return;
    event.preventDefault();
    event.stopPropagation();
    setIsOpen(false);
    triggerRef.current?.focus();
  };

  return (
    <div
      className={`vt-dropdown ${className}`}
      ref={dropdownRef}
      onKeyDown={handleKeyDown}
    >
      <button
        type="button"
        ref={triggerRef}
        className={`vt-field${isOpen ? " vt-field--open" : ""}`}
        onClick={handleToggle}
        disabled={disabled}
        aria-expanded={isOpen}
      >
        <span className="vt-field__value">
          {selectedOption?.label || placeholder}
        </span>
        <svg
          className="vt-field__chevron"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>
      {isOpen && !disabled && (
        <div
          className={`vt-menu ${placement === "above" ? "vt-menu--above" : "vt-menu--below"} ${menuClassName}`}
        >
          {options.length === 0 ? (
            <div className="vt-menu__empty">{t("common.noOptionsFound")}</div>
          ) : (
            options.map((option) => {
              const isChosen = selectedValue === option.value;
              return (
                <button
                  key={option.value}
                  type="button"
                  className={`vt-menu__option${
                    isChosen ? " vt-menu__option--chosen" : ""
                  }${option.disabled ? " vt-menu__option--disabled" : ""}`}
                  onClick={() => handleSelect(option.value)}
                  disabled={option.disabled}
                >
                  <span className="vt-menu__text">
                    <span className="vt-menu__label">{option.label}</span>
                    {option.description && (
                      <span className="vt-menu__desc">{option.description}</span>
                    )}
                  </span>
                  {isChosen && (
                    <svg
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth={2.2}
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      aria-hidden="true"
                      className="vt-menu__check"
                    >
                      <path d="M5 12.5l4.5 4.5L19 7.5" />
                    </svg>
                  )}
                </button>
              );
            })
          )}
        </div>
      )}
    </div>
  );
};
