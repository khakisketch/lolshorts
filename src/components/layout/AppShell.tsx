import { ReactNode, useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Sidebar } from "./Sidebar";
import { Button } from "@/components/ui/button";
import { Menu, X, Minus, Square, XIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useRouterState } from "@tanstack/react-router";

let appWindow: ReturnType<typeof getCurrentWindow> | null = null;
function getAppWindow() {
  if (!appWindow) {
    try {
      appWindow = getCurrentWindow();
    } catch {
      /* non-Tauri env */
    }
  }
  return appWindow;
}

interface AppShellProps {
  children: ReactNode;
}

export function AppShell({ children }: AppShellProps) {
  const { t } = useTranslation();
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });
  const isClipWorkspace = pathname === "/results";

  // Auto-focus first [data-autofocus] element on navigation
  useEffect(() => {
    const timer = requestAnimationFrame(() => {
      const el = document.querySelector("[data-autofocus]") as HTMLElement;
      if (el) el.focus();
    });
    return () => cancelAnimationFrame(timer);
  }, [children]);

  return (
    <div
      data-testid="app-shell"
      className="flex h-screen w-screen overflow-hidden"
    >
      {/* Skip to main content link for keyboard users */}
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:absolute focus:z-[100] focus:top-2 focus:left-2 focus:px-4 focus:py-2 focus:bg-gaming-cyan focus:text-black focus:rounded focus:font-bold"
      >
        {t("common.skipToContent", "Skip to main content")}
      </a>

      {/* Desktop Sidebar */}
      <Sidebar className="hidden md:flex" />

      {/* Mobile Menu Button */}
      <div className="md:hidden fixed top-4 left-4 z-50">
        <Button
          variant="outline"
          size="sm"
          onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
          className="bg-gaming-sidebar/80 backdrop-blur-sm border border-white/10 text-foreground"
          aria-label={mobileMenuOpen ? t("common.close") : t("common.menu")}
          aria-expanded={mobileMenuOpen}
          aria-controls="mobile-sidebar"
        >
          {mobileMenuOpen ? (
            <X className="h-4 w-4" aria-hidden="true" />
          ) : (
            <Menu className="h-4 w-4" aria-hidden="true" />
          )}
        </Button>
      </div>

      {/* Mobile Sidebar Overlay */}
      {mobileMenuOpen && (
        <>
          <div
            className="md:hidden fixed inset-0 bg-black/60 z-40"
            onClick={() => setMobileMenuOpen(false)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setMobileMenuOpen(false);
            }}
            role="button"
            tabIndex={0}
            aria-label={t("common.closeMobileMenu")}
          />
          <div
            id="mobile-sidebar"
            className="md:hidden fixed inset-y-0 left-0 z-50 w-64"
          >
            <Sidebar className="flex" expanded />
          </div>
        </>
      )}

      {/* Main Content */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* Custom Title Bar */}
        <div
          data-tauri-drag-region
          role="toolbar"
          aria-label={t("common.windowControls")}
          className="h-8 flex items-center justify-end shrink-0 bg-gaming-sidebar border-b border-white/5 select-none"
        >
          <button
            onClick={() => getAppWindow()?.minimize()}
            className="h-8 w-10 inline-flex items-center justify-center hover:bg-white/10 transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-gaming-cyan"
            aria-label={t("common.minimize")}
          >
            <Minus
              className="h-3.5 w-3.5 text-muted-foreground"
              aria-hidden="true"
            />
          </button>
          <button
            onClick={() => getAppWindow()?.toggleMaximize()}
            className="h-8 w-10 inline-flex items-center justify-center hover:bg-white/10 transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-gaming-cyan"
            aria-label={t("common.maximize")}
          >
            <Square
              className="h-3 w-3 text-muted-foreground"
              aria-hidden="true"
            />
          </button>
          <button
            onClick={() => getAppWindow()?.close()}
            className="h-8 w-10 inline-flex items-center justify-center hover:bg-gaming-magenta/80 hover:text-white transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-gaming-magenta"
            aria-label={t("common.closeWindow")}
          >
            <XIcon className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        </div>

        <main
          id="main-content"
          className="flex-1 overflow-y-auto bg-background relative"
          role="main"
        >
          {/* Grid Background Pattern */}
          <div
            className="absolute inset-0 gaming-grid-bg pointer-events-none"
            aria-hidden="true"
          />

          <div
            className={`relative z-10 container mx-auto p-4 md:p-6 ${isClipWorkspace ? "max-w-none" : "max-w-7xl"}`}
          >
            {/* Mobile Header */}
            <div className="md:hidden mb-6 pt-8" aria-hidden="true">
              <h1 className="text-2xl font-black italic tracking-wider bg-gradient-to-r from-white to-gray-400 bg-clip-text text-transparent">
                LoLShorts
              </h1>
              <p className="text-sm text-muted-foreground">
                Auto League of Legends Highlights
              </p>
            </div>
            {children}
          </div>
        </main>
      </div>
    </div>
  );
}
