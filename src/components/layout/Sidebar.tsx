import { useState } from "react";
import {
  Grid3x3,
  Home,
  LogIn,
  LogOut,
  PanelLeftClose,
  PanelLeftOpen,
  Power,
  Settings,
  WandSparkles,
} from "lucide-react";
import { Link, useRouterState } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { useAuthStore } from "@/lib/auth";
import { logger } from "@/lib/logger";
import { useTranslation } from "react-i18next";
import { AuthModal } from "@/components/auth/AuthModal";

interface SidebarProps {
  className?: string;
  /** Mobile navigation keeps the full destination labels. */
  expanded?: boolean;
}

export function Sidebar({ className = "", expanded }: SidebarProps) {
  const { user, entitlement, isAuthenticated, logout } = useAuthStore();
  const { t } = useTranslation();
  const [authModalOpen, setAuthModalOpen] = useState(false);
  const [desktopExpanded, setDesktopExpanded] = useState(false);
  const hasProEntitlement =
    entitlement?.tier === "PRO" && entitlement.status === "active";
  const tierLabel = hasProEntitlement ? "PRO" : "FREE";
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });
  const isCompact = expanded !== true && !desktopExpanded;
  const isStudioRoute =
    pathname.startsWith("/auto-edit") || pathname.startsWith("/editor");
  const sidebarId =
    expanded === true ? "mobile-sidebar-content" : "desktop-sidebar-content";

  const requestAppExit = () => {
    // This intentionally requests AppHandle::exit instead of closing the main
    // window. The latter can minimize to tray; the app command always goes
    // through the single safe recording-cleanup path in Rust.
    void invoke("quit_app").catch((error) => {
      logger.error("Failed to request application exit:", error);
    });
  };

  // The primary product flow is record → find → create. Every desktop route
  // starts with the same compact rail; users can expand it explicitly when
  // they need the labels. Mobile navigation remains expanded by the shell.
  const navItems = [
    {
      path: "/",
      label: t("nav.recording"),
      icon: Home,
      // Kept as `nav-dashboard` so existing e2e navigation steps keep working.
      testId: "nav-dashboard",
    },
    {
      path: "/results",
      label: t("nav.library"),
      icon: Grid3x3,
      testId: "nav-library",
    },
    {
      path: "/auto-edit",
      label: t("nav.studio"),
      icon: WandSparkles,
      testId: "nav-studio",
      active: isStudioRoute,
    },
    {
      path: "/settings",
      label: t("nav.settings"),
      icon: Settings,
      testId: "nav-settings",
    },
  ];

  return (
    <aside
      id={sidebarId}
      className={`${isCompact ? "w-16" : "w-64"} bg-gaming-sidebar border-r border-white/5 flex flex-col shadow-[5px_0_15px_rgba(0,0,0,0.5)] transition-[width] duration-200 ${className}`}
      aria-label={t("nav.sidebarLabel")}
    >
      {/* Logo */}
      <div
        className={`${isCompact ? "p-4 justify-center flex-col gap-2" : "p-6"} flex items-center gap-3`}
      >
        <div
          className="w-8 h-8 rounded bg-gradient-to-br from-gaming-cyan to-gaming-purple flex items-center justify-center font-bold text-black -skew-x-12"
          aria-hidden="true"
        >
          LS
        </div>
        <span
          className={`${isCompact ? "sr-only" : ""} text-2xl font-black italic tracking-wider bg-gradient-to-r from-white to-gray-400 bg-clip-text text-transparent`}
        >
          LoLShorts
        </span>
        {expanded !== true && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className={`${isCompact ? "" : "ml-auto"} h-8 w-8 text-muted-foreground hover:text-foreground`}
            onClick={() => setDesktopExpanded((open) => !open)}
            aria-expanded={!isCompact}
            aria-controls={sidebarId}
            aria-label={
              isCompact
                ? t("nav.expandSidebar", "사이드바 펼치기")
                : t("nav.collapseSidebar", "사이드바 접기")
            }
            data-testid="sidebar-toggle"
            title={
              isCompact
                ? t("nav.expandSidebar", "사이드바 펼치기")
                : t("nav.collapseSidebar", "사이드바 접기")
            }
          >
            {isCompact ? (
              <PanelLeftOpen className="h-4 w-4" aria-hidden="true" />
            ) : (
              <PanelLeftClose className="h-4 w-4" aria-hidden="true" />
            )}
          </Button>
        )}
      </div>

      {/* Navigation */}
      <nav
        className={`${isCompact ? "px-2" : "px-3"} flex-1 space-y-1 mt-2 overflow-y-auto`}
        role="navigation"
        aria-label={t("nav.mainNavigation")}
      >
        {navItems.map((item) => {
          const itemClassName = `gaming-nav-item flex items-center ${isCompact ? "justify-center px-2" : "gap-3 px-4"} py-3 text-sm font-bold tracking-wide`;
          return (
            <Link
              key={item.path}
              to={item.path}
              data-testid={item.testId}
              className={
                item.active ? `${itemClassName} active` : itemClassName
              }
              activeProps={{ className: `${itemClassName} active` }}
              aria-label={item.label}
              title={isCompact ? item.label : undefined}
            >
              <item.icon className="w-5 h-5" aria-hidden="true" />
              <span className={isCompact ? "sr-only" : "flex-1"}>
                {item.label}
              </span>
            </Link>
          );
        })}
      </nav>

      {/* User Profile / Auth */}
      <div
        className={`${isCompact ? "p-2" : "p-4"} border-t border-white/5`}
        role="region"
        aria-label={t("nav.userProfile")}
      >
        {isAuthenticated && user ? (
          <div className={`${isCompact ? "space-y-2" : "space-y-3"}`}>
            <div
              className={`${isCompact ? "justify-center p-2" : "gap-3 p-3"} flex items-center bg-white/5 rounded-lg`}
              aria-label={`${user.email}, ${tierLabel}`}
            >
              <div
                className="w-10 h-10 rounded-full bg-gray-700 flex items-center justify-center border border-gaming-cyan overflow-hidden"
                aria-hidden="true"
              >
                <span className="text-sm font-bold text-gaming-cyan">
                  {user.email?.charAt(0)?.toUpperCase()}
                </span>
              </div>
              <div className={`${isCompact ? "sr-only" : "flex-1 min-w-0"}`}>
                <p className="text-sm font-bold truncate">{user.email}</p>
                <p
                  className={`text-xs font-bold ${hasProEntitlement ? "text-accent-pro" : "text-muted-foreground"}`}
                >
                  {tierLabel}
                </p>
              </div>
            </div>
            {!hasProEntitlement && entitlement?.payment_available === true && (
              <Button
                variant="outline"
                size="sm"
                className={`${isCompact ? "hidden" : "w-full"} text-xs border-accent-pro text-accent-pro hover:bg-accent-pro/10`}
                onClick={() => setAuthModalOpen(true)}
              >
                {t("auth.upgradeToPro")}
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              className={`${isCompact ? "w-full px-0" : "w-full"} text-muted-foreground hover:text-gaming-magenta hover:bg-gaming-magenta/10`}
              onClick={() => logout()}
              data-testid="logout-button"
            >
              <LogOut className={`w-4 h-4 ${isCompact ? "" : "mr-2"}`} />
              <span className={isCompact ? "sr-only" : ""}>
                {t("auth.logout")}
              </span>
            </Button>
          </div>
        ) : (
          <Button
            className={`${isCompact ? "w-full px-0" : "w-full"} bg-gaming-cyan text-black font-bold hover:bg-gaming-cyan/90`}
            onClick={() => setAuthModalOpen(true)}
            data-testid="sidebar-login-button"
            aria-label={t("auth.loginSignup")}
            title={isCompact ? t("auth.loginSignup") : undefined}
          >
            {isCompact && <LogIn className="h-4 w-4" aria-hidden="true" />}
            <span className={isCompact ? "sr-only" : ""}>
              {t("auth.loginSignup")}
            </span>
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          className={`${isCompact ? "mt-2 w-full px-0" : "mt-3 w-full"} text-muted-foreground hover:bg-gaming-magenta/10 hover:text-gaming-magenta`}
          onClick={requestAppExit}
          data-testid="sidebar-quit-button"
          aria-label={t("nav.quit")}
          title={isCompact ? t("nav.quit") : undefined}
        >
          <Power
            className={`h-4 w-4 ${isCompact ? "" : "mr-2"}`}
            aria-hidden="true"
          />
          <span className={isCompact ? "sr-only" : ""}>{t("nav.quit")}</span>
        </Button>
      </div>

      {/* Auth Modal */}
      <AuthModal
        open={authModalOpen}
        onClose={() => setAuthModalOpen(false)}
        defaultMode="login"
      />
    </aside>
  );
}
