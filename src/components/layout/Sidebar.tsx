import { useState } from "react";
import {
  Home,
  Film,
  Video,
  Settings,
  LogOut,
  Sparkles,
  Youtube,
  Grid3x3,
  RotateCcw,
} from "lucide-react";
import { Link } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { useAuthStore } from "@/lib/auth";
import { useTranslation } from "react-i18next";
import { AuthModal } from "@/components/auth/AuthModal";

interface SidebarProps {
  className?: string;
}

export function Sidebar({ className = "" }: SidebarProps) {
  const { user, entitlement, isAuthenticated, logout } = useAuthStore();
  const { t } = useTranslation();
  const [authModalOpen, setAuthModalOpen] = useState(false);
  const hasProEntitlement =
    entitlement?.tier === "PRO" && entitlement.status === "active";
  const tierLabel = hasProEntitlement ? "PRO" : "FREE";

  const navItems = [
    {
      path: "/",
      label: t("nav.dashboard"),
      icon: Home,
      testId: "nav-dashboard",
    },
    { path: "/games", label: t("nav.games"), icon: Film, testId: "nav-games" },
    {
      path: "/replays",
      label: "Replays",
      icon: RotateCcw,
      testId: "nav-library",
    },
    {
      path: "/editor",
      label: t("nav.editor"),
      icon: Video,
      testId: "nav-editor",
    },
    {
      path: "/auto-edit",
      label: t("nav.autoEdit"),
      icon: Sparkles,
      testId: "nav-auto-edit",
    },
    {
      path: "/results",
      label: t("nav.results"),
      icon: Grid3x3,
      testId: "nav-results",
    },
    {
      path: "/youtube",
      label: t("nav.youtube"),
      icon: Youtube,
      badge: t("nav.pro"),
      proRequired: true,
      testId: "nav-youtube",
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
      className={`w-64 bg-gaming-sidebar border-r border-white/5 flex flex-col shadow-[5px_0_15px_rgba(0,0,0,0.5)] ${className}`}
      aria-label={t("nav.sidebarLabel")}
    >
      {/* Logo */}
      <div className="p-6 flex items-center gap-3">
        <div
          className="w-8 h-8 rounded bg-gradient-to-br from-gaming-cyan to-gaming-purple flex items-center justify-center font-bold text-black -skew-x-12"
          aria-hidden="true"
        >
          LS
        </div>
        <span className="text-2xl font-black italic tracking-wider bg-gradient-to-r from-white to-gray-400 bg-clip-text text-transparent">
          LoLShorts
        </span>
      </div>

      {/* Navigation */}
      <nav
        className="flex-1 px-3 space-y-1 mt-2 overflow-y-auto"
        role="navigation"
        aria-label={t("nav.mainNavigation")}
      >
        {navItems.map((item) => {
          const isProRequired = item.proRequired && !hasProEntitlement;
          return (
            <Link
              key={item.path}
              to={item.path}
              data-testid={item.testId}
              className={`
                gaming-nav-item flex items-center gap-3 px-4 py-3 text-sm font-bold tracking-wide
                ${isProRequired ? "opacity-40" : ""}
              `}
              activeProps={{
                className:
                  "gaming-nav-item active flex items-center gap-3 px-4 py-3 text-sm font-bold tracking-wide",
              }}
              aria-label={
                isProRequired
                  ? `${item.label} (${t("nav.proRequired")})`
                  : item.label
              }
              aria-disabled={isProRequired ? "true" : undefined}
              onClick={
                isProRequired
                  ? (e) => {
                      e.preventDefault();
                      setAuthModalOpen(true);
                    }
                  : undefined
              }
            >
              <item.icon className="w-5 h-5" aria-hidden="true" />
              <span className="flex-1">{item.label}</span>
              {item.badge && (
                <Badge
                  variant="outline"
                  className={`text-[10px] ${
                    isProRequired
                      ? "border-yellow-600/50 text-yellow-600"
                      : "border-gaming-cyan/50 text-gaming-cyan"
                  }`}
                  aria-label={item.badge}
                >
                  {item.badge}
                </Badge>
              )}
            </Link>
          );
        })}
      </nav>

      {/* User Profile / Auth */}
      <div
        className="p-4 border-t border-white/5"
        role="region"
        aria-label={t("nav.userProfile")}
      >
        {isAuthenticated && user ? (
          <div className="space-y-3">
            <div
              className="flex items-center gap-3 bg-white/5 p-3 rounded-lg"
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
              <div className="flex-1 min-w-0">
                <p className="text-sm font-bold truncate">{user.email}</p>
                <p
                  className={`text-xs font-bold ${hasProEntitlement ? "text-accent-pro" : "text-muted-foreground"}`}
                >
                  {tierLabel}
                </p>
              </div>
            </div>
            {!hasProEntitlement && (
              <Button
                variant="outline"
                size="sm"
                className="w-full text-xs border-accent-pro text-accent-pro hover:bg-accent-pro/10"
                onClick={() => setAuthModalOpen(true)}
              >
                {t("auth.upgradeToPro")}
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              className="w-full text-muted-foreground hover:text-gaming-magenta hover:bg-gaming-magenta/10"
              onClick={() => logout()}
              data-testid="logout-button"
            >
              <LogOut className="w-4 h-4 mr-2" />
              {t("auth.logout")}
            </Button>
          </div>
        ) : (
          <Button
            className="w-full bg-gaming-cyan text-black font-bold hover:bg-gaming-cyan/90"
            onClick={() => setAuthModalOpen(true)}
            data-testid="sidebar-login-button"
          >
            {t("auth.loginSignup")}
          </Button>
        )}
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
