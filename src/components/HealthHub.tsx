import {
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Loader2,
  Video,
  Play,
  Wand2,
  Upload,
  HardDrive,
  Activity,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

interface HealthStatus {
  status: "ok" | "warning" | "blocked" | "checking";
  label: string;
  message: string;
}

interface HealthHubProps {
  healthData: {
    capture: HealthStatus;
    replay: HealthStatus;
    autoEdit: HealthStatus;
    publish: HealthStatus;
    storage: HealthStatus;
    service: HealthStatus;
  };
}

const statusConfig = {
  ok: {
    color: "text-green-400 bg-green-500/20 border-green-500/50",
    icon: <CheckCircle2 className="w-3 h-3" />,
    labelKey: "dashboard.services.status.ready",
  },
  warning: {
    color: "text-yellow-400 bg-yellow-500/20 border-yellow-500/50",
    icon: <AlertTriangle className="w-3 h-3" />,
    labelKey: "dashboard.services.status.needsSetup",
  },
  blocked: {
    color: "text-red-400 bg-red-500/20 border-red-500/50",
    icon: <XCircle className="w-3 h-3" />,
    labelKey: "dashboard.services.status.blocked",
  },
  checking: {
    color: "text-muted-foreground bg-white/5 border-white/10",
    icon: <Loader2 className="w-3 h-3 animate-spin" />,
    labelKey: "dashboard.services.status.checking",
  },
} as const;

const categoryIcons = {
  capture: <Video className="w-4 h-4" />,
  replay: <Play className="w-4 h-4" />,
  autoEdit: <Wand2 className="w-4 h-4" />,
  publish: <Upload className="w-4 h-4" />,
  storage: <HardDrive className="w-4 h-4" />,
  service: <Activity className="w-4 h-4" />,
};

export function HealthHub({ healthData }: HealthHubProps) {
  const { t } = useTranslation();
  return (
    <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
      {Object.entries(healthData).map(([key, data]) => {
        const config = statusConfig[data.status];
        return (
          <div
            key={key}
            className="gaming-panel p-4 flex flex-col justify-between min-h-[120px] transition-all hover:border-white/20"
          >
            <div className="flex items-center gap-2 mb-3">
              <span className="text-gaming-cyan">
                {categoryIcons[key as keyof typeof categoryIcons]}
              </span>
              <span className="text-xs font-bold uppercase tracking-wider text-muted-foreground">
                {data.label}
              </span>
            </div>

            <div className="space-y-2">
              <div
                className={cn(
                  "inline-flex max-w-full items-center gap-1.5 rounded border px-2 py-1 text-[10px] font-bold uppercase leading-none tracking-normal",
                  config.color,
                )}
              >
                {config.icon}
                <span
                  className="whitespace-normal break-words"
                  style={{ wordBreak: "keep-all" }}
                >
                  {t(config.labelKey)}
                </span>
              </div>
              <p
                className="text-[11px] text-muted-foreground leading-tight"
                style={{ wordBreak: "keep-all" }}
              >
                {data.message}
              </p>
            </div>
          </div>
        );
      })}
    </div>
  );
}
