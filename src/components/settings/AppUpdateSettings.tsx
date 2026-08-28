import { useTranslation } from "react-i18next";
import { Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useAppUpdateStore } from "@/stores/appUpdateStore";

export function AppUpdateSettings() {
  const { t } = useTranslation();
  const { snapshot, check } = useAppUpdateStore();
  const checking = snapshot.status === "checking";

  return (
    <section
      className="gaming-panel min-w-0 p-6"
      aria-labelledby="app-update-settings-title"
    >
      <div className="flex min-w-0 flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0">
          <h3 id="app-update-settings-title" className="font-semibold">
            {t("appUpdater.settingsTitle")}
          </h3>
          <p
            className="mt-1 break-words text-sm text-muted-foreground"
            aria-live="polite"
          >
            {snapshot.status === "disabled"
              ? t("appUpdater.disabled")
              : snapshot.status === "up_to_date"
                ? t("appUpdater.upToDate", {
                    version: snapshot.current_version,
                  })
                : t("appUpdater.currentVersion", {
                    version: snapshot.current_version || "—",
                  })}
          </p>
        </div>
        <Button
          type="button"
          variant="outline"
          className="shrink-0"
          onClick={() => void check()}
          disabled={checking || snapshot.status === "disabled"}
          data-testid="check-app-update"
        >
          {checking ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" aria-hidden="true" />
          ) : (
            <RefreshCw className="mr-2 h-4 w-4" aria-hidden="true" />
          )}
          {checking ? t("appUpdater.checking") : t("appUpdater.check")}
        </Button>
      </div>
    </section>
  );
}
