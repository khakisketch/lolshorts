import { cn } from "@/lib/utils";
import { Button } from "./button";
import { LucideIcon } from "lucide-react";
import { ReactNode } from "react";

interface EmptyStateProps {
  icon?: LucideIcon;
  iconClassName?: string;
  title: string;
  description?: string | ReactNode;
  action?: {
    label: string;
    onClick: () => void;
    variant?: "default" | "secondary" | "outline" | "ghost";
  };
  secondaryAction?: {
    label: string;
    onClick: () => void;
  };
  children?: ReactNode;
  className?: string;
  size?: "sm" | "md" | "lg";
}

const sizeClasses = {
  sm: {
    container: "py-8",
    icon: "w-10 h-10",
    title: "text-base",
    description: "text-xs",
  },
  md: {
    container: "py-12",
    icon: "w-14 h-14",
    title: "text-lg",
    description: "text-sm",
  },
  lg: {
    container: "py-16",
    icon: "w-20 h-20",
    title: "text-xl",
    description: "text-base",
  },
};

export function EmptyState({
  icon: Icon,
  iconClassName,
  title,
  description,
  action,
  secondaryAction,
  children,
  className,
  size = "md",
}: EmptyStateProps) {
  const sizes = sizeClasses[size];

  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center text-center",
        sizes.container,
        className,
      )}
    >
      {Icon && (
        <div className="rounded-full bg-muted p-4 mb-4">
          <Icon
            className={cn("text-muted-foreground", sizes.icon, iconClassName)}
          />
        </div>
      )}
      <h3 className={cn("font-semibold mb-2", sizes.title)}>{title}</h3>
      {description && (
        <p
          className={cn(
            "text-muted-foreground max-w-sm mb-6",
            sizes.description,
          )}
        >
          {description}
        </p>
      )}
      {(action || secondaryAction) && (
        <div className="flex flex-col sm:flex-row items-center gap-3">
          {action && (
            <Button
              onClick={action.onClick}
              variant={action.variant || "default"}
            >
              {action.label}
            </Button>
          )}
          {secondaryAction && (
            <Button
              onClick={secondaryAction.onClick}
              variant="ghost"
              className="text-muted-foreground"
            >
              {secondaryAction.label}
            </Button>
          )}
        </div>
      )}
      {children}
    </div>
  );
}

// Preset empty states for common scenarios
interface PresetEmptyStateProps {
  onAction?: () => void;
  className?: string;
}

export function EmptyStateNoGames({
  onAction,
  className,
}: PresetEmptyStateProps) {
  return (
    <EmptyState
      icon={undefined}
      title="No games recorded"
      description="Start playing a game with League of Legends running, and your games will be automatically recorded here."
      action={
        onAction ? { label: "Go to Dashboard", onClick: onAction } : undefined
      }
      className={className}
    />
  );
}

export function EmptyStateNoClips({
  onAction,
  className,
}: PresetEmptyStateProps) {
  return (
    <EmptyState
      icon={undefined}
      title="No clips yet"
      description="Play a game to capture highlights, or select a game from your library to start editing."
      action={
        onAction ? { label: "Browse Games", onClick: onAction } : undefined
      }
      className={className}
    />
  );
}

export function EmptyStateNoResults({
  onAction,
  className,
}: PresetEmptyStateProps) {
  return (
    <EmptyState
      icon={undefined}
      title="No results found"
      description="Try adjusting your search or filter criteria."
      action={
        onAction ? { label: "Clear Filters", onClick: onAction } : undefined
      }
      className={className}
    />
  );
}
