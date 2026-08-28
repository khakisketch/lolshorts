import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Download, Loader2, RotateCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { useAppUpdateStore } from "@/stores/appUpdateStore";

const isVisibleStatus = (status: string) =>
  ["available", "downloading", "installing", "failed"].includes(status);

export function AppUpdateDialog() {
  const { t } = useTranslation();
  const { snapshot, initialize, check, install } = useAppUpdateStore();
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null);
  const primaryActionRef = useRef<HTMLButtonElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    let unlisten = () => {};
    let disposed = false;
    void initialize().then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten();
    };
  }, [initialize]);

  const busy =
    snapshot.status === "downloading" || snapshot.status === "installing";
  const open =
    isVisibleStatus(snapshot.status) &&
    (busy || snapshot.available_version !== dismissedVersion);
  const errorText = useMemo(
    () =>
      snapshot.error_code
        ? t(`appUpdater.errors.${snapshot.error_code}`, {
            defaultValue: t("appUpdater.errors.unknown"),
          })
        : null,
    [snapshot.error_code, t],
  );

  const defer = () =>
    setDismissedVersion(snapshot.available_version ?? "failed");
  const retry = () => {
    if (snapshot.available_version) void install();
    else void check();
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !next && !busy && defer()}>
      <DialogContent
        className="max-h-[min(90vh,42rem)] w-[calc(100vw-2rem)] max-w-lg overflow-y-auto break-words"
        data-testid="app-update-dialog"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          returnFocusRef.current = document.activeElement as HTMLElement | null;
          primaryActionRef.current?.focus();
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocusRef.current?.focus();
          returnFocusRef.current = null;
        }}
      >
        <DialogHeader>
          <DialogTitle>{t("appUpdater.title")}</DialogTitle>
          <DialogDescription>
            {snapshot.status === "failed"
              ? errorText
              : t("appUpdater.description", {
                  current: snapshot.current_version,
                  available: snapshot.available_version,
                })}
          </DialogDescription>
        </DialogHeader>

        {snapshot.notes && (
          <section
            aria-labelledby="app-update-notes"
            className="min-w-0 rounded-md bg-muted p-3"
          >
            <h3 id="app-update-notes" className="text-sm font-semibold">
              {t("appUpdater.releaseNotes")}
            </h3>
            <p className="mt-1 whitespace-pre-wrap text-sm text-muted-foreground">
              {snapshot.notes}
            </p>
          </section>
        )}

        {busy && (
          <div className="space-y-2" aria-live="polite">
            <div className="flex items-center justify-between gap-3 text-sm">
              <span>
                {snapshot.status === "downloading"
                  ? t("appUpdater.downloading")
                  : t("appUpdater.installing")}
              </span>
              <span>{Math.round(snapshot.progress_percentage)}%</span>
            </div>
            <Progress
              value={snapshot.progress_percentage}
              aria-label={t("appUpdater.progressLabel")}
              aria-valuetext={`${Math.round(snapshot.progress_percentage)}%`}
            />
          </div>
        )}

        {snapshot.status === "installing" && (
          <p className="text-sm text-amber-300">
            {t("appUpdater.windowsExitNotice")}
          </p>
        )}

        <DialogFooter className="gap-2 sm:space-x-0">
          {!busy && (
            <Button type="button" variant="outline" onClick={defer}>
              {t("appUpdater.later")}
            </Button>
          )}
          {snapshot.status === "available" && (
            <Button
              ref={primaryActionRef}
              type="button"
              onClick={() => void install()}
            >
              <Download className="mr-2 h-4 w-4" aria-hidden="true" />
              {t("appUpdater.install")}
            </Button>
          )}
          {snapshot.status === "failed" && (
            <Button ref={primaryActionRef} type="button" onClick={retry}>
              <RotateCw className="mr-2 h-4 w-4" aria-hidden="true" />
              {t("appUpdater.retry")}
            </Button>
          )}
          {busy && (
            <Button ref={primaryActionRef} type="button" disabled>
              <Loader2
                className="mr-2 h-4 w-4 animate-spin"
                aria-hidden="true"
              />
              {t("appUpdater.working")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
