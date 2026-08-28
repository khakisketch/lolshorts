import { cn } from "@/lib/utils";
import { Loader2, LucideProps } from "lucide-react";
import { forwardRef } from "react";

type SpinnerSize = "xs" | "sm" | "md" | "lg" | "xl";

interface SpinnerProps extends Omit<LucideProps, "size"> {
  size?: SpinnerSize;
  label?: string;
}

const sizeMap: Record<SpinnerSize, { icon: number; text: string }> = {
  xs: { icon: 12, text: "text-xs" },
  sm: { icon: 16, text: "text-sm" },
  md: { icon: 24, text: "text-base" },
  lg: { icon: 32, text: "text-lg" },
  xl: { icon: 48, text: "text-xl" },
};

const Spinner = forwardRef<SVGSVGElement, SpinnerProps>(
  ({ className, size = "md", label, ...props }, ref) => {
    const { icon: iconSize, text: textSize } = sizeMap[size];

    return (
      <span className="inline-flex items-center gap-2">
        <Loader2
          ref={ref}
          className={cn("animate-spin text-muted-foreground", className)}
          size={iconSize}
          {...props}
        />
        {label && (
          <span className={cn("text-muted-foreground", textSize)}>{label}</span>
        )}
      </span>
    );
  },
);

Spinner.displayName = "Spinner";

// Centered spinner for use in containers
function SpinnerCenter({
  size = "lg",
  label,
  className,
}: {
  size?: SpinnerSize;
  label?: string;
  className?: string;
}) {
  return (
    <div className={cn("flex items-center justify-center py-12", className)}>
      <Spinner size={size} label={label} />
    </div>
  );
}

// Inline spinner for buttons and text
function SpinnerInline({
  size = "sm",
  className,
}: {
  size?: SpinnerSize;
  className?: string;
}) {
  return <Spinner size={size} className={className} />;
}

export { Spinner, SpinnerCenter, SpinnerInline };
