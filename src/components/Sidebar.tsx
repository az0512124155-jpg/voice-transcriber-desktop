import React from "react";
import { useTranslation } from "react-i18next";
import {
  Cog,
  FlaskConical,
  History,
  Info,
  Sparkles,
  Cpu,
  SlidersHorizontal,
} from "lucide-react";
import AppLogo from "./icons/AppLogo";
import { useSettings } from "../hooks/useSettings";
import {
  GeneralSettings,
  AdvancedSettings,
  HistorySettings,
  AboutSettings,
  PostProcessingSettings,
  ModelsSettings,
} from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  /** `null` when App renders the section itself (Debug, with its handlers). */
  component: React.ComponentType | null;
  enabled: (settings: any) => boolean;
}

export const SECTIONS_CONFIG = {
  general: {
    labelKey: "sidebar.general",
    icon: SlidersHorizontal,
    component: GeneralSettings,
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    // Fork: exactly one model ships inside the app and it is already
    // installed, so there is nothing to browse, download or delete.
    enabled: () => false,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    enabled: () => true,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    enabled: (settings) => settings?.post_process_enabled ?? false,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: null,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    enabled: () => true,
  },
} as const satisfies Record<string, SectionConfig>;

/** `ctrl+space` → `⌃ Space` on a Mac, `Ctrl+Space` everywhere else. */
const formatShortcut = (binding: string | undefined): string | null => {
  if (!binding) return null;
  const isMac = /mac/i.test(navigator.userAgent);
  const parts = binding
    .split("+")
    .map((part) => part.trim().toLowerCase())
    .filter(Boolean);
  if (parts.length === 0) return null;

  if (!isMac) {
    return parts
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join("+");
  }

  const glyphs: Record<string, string> = {
    ctrl: "⌃",
    control: "⌃",
    alt: "⌥",
    option: "⌥",
    shift: "⇧",
    cmd: "⌘",
    command: "⌘",
    super: "⌘",
  };
  return parts
    .map((part) => glyphs[part] ?? part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
};

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  const shortcut = formatShortcut(
    settings?.bindings?.transcribe?.current_binding,
  );

  return (
    <aside className="vt-sidebar">
      <div className="vt-sidebar__brand">
        <AppLogo width={132} />
      </div>

      <nav className="vt-sidebar__nav">
        {availableSections.map((section) => {
          const Icon = section.icon;
          const isActive = activeSection === section.id;

          return (
            <button
              key={section.id}
              type="button"
              className={`vt-nav-item${isActive ? " vt-nav-item--active" : ""}`}
              onClick={() => onSectionChange(section.id)}
              aria-current={isActive ? "page" : undefined}
            >
              <Icon className="vt-nav-item__icon" width={18} height={18} />
              <span className="vt-nav-item__label">{t(section.labelKey)}</span>
            </button>
          );
        })}
      </nav>

      <div className="vt-sidebar__spacer" />

      {shortcut && (
        <div className="vt-sidebar__hint">
          <span className="vt-kbd">{shortcut}</span>
        </div>
      )}
    </aside>
  );
};
