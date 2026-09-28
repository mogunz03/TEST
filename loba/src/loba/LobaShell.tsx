import React, { useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { Home } from "./pages/Home";
import { Cleanup } from "./pages/Cleanup";
import { DictionaryPage } from "./pages/DictionaryPage";
import { StylesPage } from "./pages/StylesPage";

type Page = "home" | "dictionary" | "styles" | "cleanup" | "engine";

interface Section {
  id: Page;
  labelKey: string;
}

// Order is the order a new user needs them: talk first, tune second,
// the underlying engine last.
const SECTIONS: Section[] = [
  { id: "home", labelKey: "nav.home" },
  { id: "dictionary", labelKey: "nav.dictionary" },
  { id: "styles", labelKey: "nav.styles" },
  { id: "cleanup", labelKey: "nav.cleanup" },
  { id: "engine", labelKey: "nav.engine" },
];

interface Props {
  /** Handy's full settings surface, kept one click away under "Engine". */
  engine: ReactNode;
  /** Permission / secure-input banners that must stay visible everywhere. */
  banners: ReactNode;
}

export const LobaShell: React.FC<Props> = ({ engine, banners }) => {
  const { t } = useTranslation("loba");
  const [page, setPage] = useState<Page>("home");

  // Command mode never pastes on failure, so say why instead of doing nothing.
  useEffect(() => {
    const un = listen<string>("loba:command-failed", (e) =>
      toast.error(t("home.command.failed"), { description: e.payload }),
    );
    return () => {
      un.then((f) => f());
    };
  }, [t]);

  return (
    <div className="h-screen flex bg-paper text-ink select-none cursor-default">
      <nav className="w-44 shrink-0 flex flex-col pt-7 pb-5 ps-6 pe-3">
        <span className="loba-display text-[19px] tracking-[0.06em] mb-9">
          {t("brand")}
        </span>
        <ul className="flex flex-col gap-0.5">
          {SECTIONS.map((s) => {
            const active = s.id === page;
            return (
              <li key={s.id}>
                <button
                  type="button"
                  onClick={() => setPage(s.id)}
                  className={`group w-full flex items-center gap-2.5 py-1.5 text-[14px] text-start transition-colors duration-150 cursor-pointer ${
                    active ? "text-ink" : "text-soft hover:text-ink"
                  }`}
                >
                  {/* The active mark is a dot, not a filled pill: the
                      sidebar stays a list of words. */}
                  <span
                    className={`h-[5px] w-[5px] rounded-full transition-all duration-200 ${
                      active
                        ? "bg-clay scale-100"
                        : "bg-transparent scale-50 group-hover:bg-rule group-hover:scale-100"
                    }`}
                  />
                  {t(s.labelKey)}
                </button>
              </li>
            );
          })}
        </ul>
        <p className="mt-auto text-[11px] leading-4 text-soft">
          {t("footer.offline")}
        </p>
      </nav>

      <main className="flex-1 min-w-0 flex flex-col border-s border-rule bg-sheet">
        <div className="flex flex-col gap-3 px-4 pt-4 empty:hidden">
          {banners}
        </div>
        {page === "engine" ? (
          <div key="engine" className="loba-page flex-1 min-h-0">
            {engine}
          </div>
        ) : (
          <div className="flex-1 overflow-y-auto">
            {/* Content sits left of centre with a wide margin, like a page
                of notes — not a centred dashboard column. */}
            <div
              key={page}
              className="loba-page max-w-[560px] ps-12 pe-8 pt-12 pb-16"
            >
              {page === "home" && (
                <Home onOpenCleanup={() => setPage("cleanup")} />
              )}
              {page === "dictionary" && <DictionaryPage />}
              {page === "styles" && <StylesPage />}
              {page === "cleanup" && <Cleanup />}
            </div>
          </div>
        )}
      </main>
    </div>
  );
};
