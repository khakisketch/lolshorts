import { Link } from "@tanstack/react-router";
import { Scissors, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";

type StudioMode = "auto" | "manual";

interface StudioModeNavProps {
  active: StudioMode;
}

/**
 * Keeps the two creation modes discoverable without promoting the low-level
 * editor route into a second global navigation item.
 */
export function StudioModeNav({ active }: StudioModeNavProps) {
  const { t } = useTranslation();
  const itemClass =
    "inline-flex min-h-10 items-center gap-2 rounded-md px-3 text-sm font-semibold transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-gaming-cyan";

  return (
    <nav
      aria-label={t("studio.modeNavigation")}
      className="flex w-fit items-center gap-1 rounded-lg border border-white/10 bg-gaming-sidebar/50 p-1"
    >
      <Link
        to="/auto-edit"
        className={`${itemClass} ${active === "auto" ? "bg-gaming-cyan/15 text-gaming-cyan" : "text-muted-foreground hover:bg-white/5 hover:text-foreground"}`}
        aria-current={active === "auto" ? "page" : undefined}
      >
        <Sparkles className="h-4 w-4" aria-hidden="true" />
        {t("studio.autoEdit")}
      </Link>
      <Link
        to="/editor"
        className={`${itemClass} ${active === "manual" ? "bg-gaming-cyan/15 text-gaming-cyan" : "text-muted-foreground hover:bg-white/5 hover:text-foreground"}`}
        aria-current={active === "manual" ? "page" : undefined}
      >
        <Scissors className="h-4 w-4" aria-hidden="true" />
        {t("studio.manualEdit")}
      </Link>
    </nav>
  );
}
