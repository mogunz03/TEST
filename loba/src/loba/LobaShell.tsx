import React, { useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import {
  BookText,
  History,
  House,
  MessagesSquare,
  Settings2,
  Users,
  type LucideIcon,
} from "lucide-react";
import HandyTextLogo from "@/components/icons/HandyTextLogo";
import { Home } from "./pages/Home";
import { DictionaryPage } from "./pages/DictionaryPage";
import { StylesPage } from "./pages/StylesPage";
import { HistoryPage } from "./pages/HistoryPage";
import { MeetingsPage } from "./pages/MeetingsPage";
import { SettingsPage, type SettingsTab } from "./pages/SettingsPage";
import { useLoba } from "./useOllama";

type Page =
  | "home"
  | "history"
  | "meetings"
  | "dictionary"
  | "styles"
  | "settings";

const NAV: { id: Page; icon: LucideIcon }[] = [
  { id: "home", icon: House },
  { id: "history", icon: History },
  { id: "meetings", icon: Users },
  { id: "dictionary", icon: BookText },
  { id: "styles", icon: MessagesSquare },
];

interface Props {
  /** Handy's full settings surface, under Settings → Engine. */
  engine: ReactNode;
  /** Permission / secure-input banners that must stay visible everywhere. */
  banners: ReactNode;
}

export const LobaShell: React.FC<Props> = ({ engine, banners }) => {
  const { t } = useTranslation("loba");
  const [page, setPage] = useState<Page>("home");
  const [tab, setTab] = useState<SettingsTab>("general");

  // Command mode never pastes on failure, so say why instead of doing nothing.
  useEffect(() => {
    const un = listen<string>("loba:command-failed", (e) =>
      toast.error(t("command.failed"), { description: e.payload }),
    );
    return () => {
      un.then((f) => f());
    };
  }, [t]);

  const openSettings = (next: SettingsTab) => {
    setTab(next);
    setPage("settings");
  };

  return (
    <div className="loba-root h-screen flex bg-void text-ink select-none cursor-default">
      <nav className="w-[208px] shrink-0 flex flex-col px-3 pt-6 pb-3">
        <div className="px-3 mb-7 text-ink">
          <HandyTextLogo height={22} />
        </div>
        <ul className="flex flex-col gap-px">
          {NAV.map((item) => (
            <NavItem
              key={item.id}
              icon={item.icon}
              label={t(`nav.${item.id}`)}
              active={page === item.id}
              onClick={() => setPage(item.id)}
            />
          ))}
        </ul>

        <div className="mt-auto flex flex-col gap-2">
          <StatusCard onOpen={() => openSettings("cleanup")} />
          <ul>
            <NavItem
              icon={Settings2}
              label={t("nav.settings")}
              active={page === "settings"}
              onClick={() => setPage("settings")}
            />
          </ul>
        </div>
      </nav>

      {/* The working surface is one step lighter than the frame and inset,
          so the sidebar reads as the room and this as the desk. */}
      <main className="flex-1 min-w-0 my-2 me-2 flex flex-col rounded-[14px] bg-base border border-line overflow-hidden">
        <div className="flex flex-col gap-3 px-4 pt-4 empty:hidden">
          {banners}
        </div>
        {page === "settings" ? (
          <SettingsPage tab={tab} onTab={setTab} engine={engine} />
        ) : (
          <div className="loba-scroll flex-1 overflow-y-auto">
            <div
              key={page}
              className="loba-page max-w-[680px] mx-auto px-10 pt-9 pb-14"
            >
              {page === "home" && (
                <Home
                  onOpenHistory={() => setPage("history")}
                  onOpenCleanup={() => openSettings("cleanup")}
                />
              )}
              {page === "history" && <HistoryPage />}
              {page === "meetings" && <MeetingsPage />}
              {page === "dictionary" && <DictionaryPage />}
              {page === "styles" && <StylesPage />}
            </div>
          </div>
        )}
      </main>
    </div>
  );
};

const NavItem: React.FC<{
  icon: LucideIcon;
  label: string;
  active: boolean;
  onClick: () => void;
}> = ({ icon: Icon, label, active, onClick }) => (
  <li>
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={`w-full h-[32px] flex items-center gap-2.5 px-3 rounded-[8px] text-[13px] text-start transition-colors duration-150 cursor-pointer ${
        active ? "bg-raised text-ink" : "text-soft hover:text-ink hover:bg-card"
      }`}
    >
      <Icon size={15} strokeWidth={active ? 2 : 1.7} />
      {label}
    </button>
  </li>
);

/** Where Typeless shows your plan, this shows whether cleanup is working. */
const StatusCard: React.FC<{ onOpen: () => void }> = ({ onOpen }) => {
  const { t } = useTranslation("loba");
  const { config, ollama } = useLoba();
  if (!config) return null;
  const [color, key] = !config.cleanup_enabled
    ? ["text-faint", "off"]
    : ollama.kind === "ready"
      ? ["text-ok", "ready"]
      : ollama.kind === "checking"
        ? ["text-faint", "checking"]
        : ["text-warn", "problem"];
  return (
    <button
      type="button"
      onClick={onOpen}
      className="mx-1 rounded-[11px] border border-line bg-base px-3 py-2.5 text-start hover:border-line-strong transition-colors cursor-pointer"
    >
      <div className="flex items-center gap-2 text-[12px] text-ink">
        <span className={`loba-dot ${color}`} data-live={key === "ready"} />
        {t(`sidebar.${key}`)}
      </div>
      <div className="mt-1 text-[11px] leading-4 text-faint truncate">
        {config.cleanup_enabled ? config.model : t("sidebar.offNote")}
      </div>
      <div className="mt-2 pt-2 border-t border-line text-[10.5px] leading-[14px] text-faint">
        {t("sidebar.offline")}
      </div>
    </button>
  );
};
