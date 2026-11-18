import React from "react";
import { useTranslation } from "react-i18next";
import {
  Cpu,
  FlaskConical,
  History,
  Home,
  Info,
  Lock,
  NotebookPen,
  Settings,
  Users,
  SlidersHorizontal,
  Sparkles,
} from "lucide-react";
import { useMeetingStatus } from "./notes/noteUtils";
import BolkeWordmark from "./icons/BolkeWordmark";
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
  /** "main" is the wireframe's Home / History / Settings; "more" holds the
   * rest of the settings pages, below them. */
  group: "main" | "more";
  /** `null` when App renders the section itself (Debug, with its handlers). */
  component: React.ComponentType | null;
  enabled: (settings: any) => boolean;
}

export const SECTIONS_CONFIG = {
  home: {
    labelKey: "sidebar.home",
    icon: Home,
    group: "main",
    component: null,
    enabled: () => true,
  },
  notes: {
    labelKey: "sidebar.notes",
    icon: NotebookPen,
    group: "main",
    component: null,
    enabled: () => true,
  },
  meetings: {
    labelKey: "sidebar.meetings",
    icon: Users,
    group: "main",
    component: null,
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    group: "main",
    component: HistorySettings,
    enabled: () => true,
  },
  general: {
    labelKey: "sidebar.settings",
    icon: Settings,
    group: "main",
    component: GeneralSettings,
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    group: "more",
    component: ModelsSettings,
    enabled: () => true,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: SlidersHorizontal,
    group: "more",
    component: AdvancedSettings,
    enabled: () => true,
  },
  postProcessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    group: "more",
    component: PostProcessingSettings,
    enabled: (settings) => settings?.post_process_enabled ?? false,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    group: "more",
    component: null,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    group: "more",
    component: AboutSettings,
    enabled: () => true,
  },
} as const satisfies Record<string, SectionConfig>;

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
  const meeting = useMeetingStatus();

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  const navButton = (section: (typeof availableSections)[number]) => {
    const Icon = section.icon;
    const isActive = activeSection === section.id;
    const main = section.group === "main";
    return (
      <button
        type="button"
        key={section.id}
        aria-current={isActive ? "page" : undefined}
        onClick={() => onSectionChange(section.id)}
        className={`flex items-center w-full rounded-full border border-transparent text-start cursor-pointer transition-colors ${
          main
            ? "gap-3 min-h-11 px-4 text-[14px] font-medium"
            : "gap-[10px] min-h-9 px-4 text-[13px]"
        } ${
          isActive
            ? "bg-linear-to-b from-[#3a78ff] to-[#2a5ff0] text-white shadow-[0_6px_24px_rgba(47,107,255,0.35)]"
            : "text-[#aeb9d6] hover:bg-[rgba(120,150,255,0.08)] hover:text-text"
        }`}
      >
        <Icon
          width={main ? 18 : 16}
          height={main ? 18 : 16}
          strokeWidth={1.8}
          className="shrink-0"
          aria-hidden="true"
        />
        <span className="truncate" title={t(section.labelKey)}>
          {t(section.labelKey)}
        </span>
      </button>
    );
  };

  const main = availableSections.filter((s) => s.group === "main");
  const more = availableSections.filter((s) => s.group === "more");

  return (
    <nav
      aria-label={t("sidebar.home")}
      className="flex flex-col w-[220px] shrink-0 h-full overflow-y-auto bg-sidebar border-e border-[rgba(140,170,255,0.08)] px-4 pt-5 pb-4 gap-[6px]"
    >
      <BolkeWordmark width={132} className="mx-2 mt-1 mb-5 block" />
      {main.map(navButton)}
      <p className="mt-4 mb-1 px-4 text-[11px] font-medium uppercase tracking-[0.06em] text-ink-3">
        {t("sidebar.more")}
      </p>
      {more.map(navButton)}
      <div className="grow min-h-6" />
      {meeting.recording && (
        <button
          type="button"
          onClick={() => onSectionChange("meetings")}
          className="flex items-center gap-[10px] mb-2 px-4 py-[14px] rounded-[18px] bg-[rgba(255,95,87,0.08)] border border-[rgba(255,120,110,0.25)] text-[13px] text-[#ffc9c4] text-start cursor-pointer hover:border-[rgba(255,120,110,0.5)]"
        >
          <span
            aria-hidden="true"
            className="w-[9px] h-[9px] rounded-full bg-[#ff6b61] shadow-[0_0_10px_#ff6b61] animate-pulse"
          />
          {t("notes.recording.sidebar")}
        </button>
      )}
      <div className="flex flex-col gap-[10px] p-4 rounded-[18px] bg-[rgba(20,30,64,0.55)] border border-[rgba(140,170,255,0.12)]">
        <div className="flex items-center gap-2 text-[13px] font-semibold">
          <Lock
            size={16}
            strokeWidth={1.8}
            className="text-accent-soft"
            aria-hidden="true"
          />
          {t("sidebar.onDeviceTitle")}
        </div>
        <p className="m-0 text-[12px] leading-[1.5] text-ink-2">
          {t("sidebar.onDeviceBody")}
        </p>
      </div>
    </nav>
  );
};
