import { useCallback, useEffect, useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { recordingApi, ReplayTargetReadiness } from "@/api/recording";
import { cmd } from "@/api/client";
import { useToast } from "@/components/ui/use-toast";
import { logger } from "@/lib/logger";

const setRecordingTarget = (summonerName: string | null) =>
  cmd<void>("set_recording_target", { summonerName });

interface ReplayTargetModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function ReplayTargetModal({ isOpen, onClose }: ReplayTargetModalProps) {
  const { t } = useTranslation();
  const [readiness, setReadiness] = useState<ReplayTargetReadiness | null>(
    null,
  );
  const { toast } = useToast();
  const pollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pollingActiveRef = useRef(false);
  const pollReadinessRef = useRef<(() => Promise<void>) | null>(null);

  const clearPollTimer = useCallback(() => {
    if (pollTimerRef.current) {
      clearTimeout(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }, []);

  const pollReadiness = useCallback(async () => {
    if (!pollingActiveRef.current) return;
    clearPollTimer();

    try {
      const result = await recordingApi.getReplayTargetReadiness();
      if (!pollingActiveRef.current) return;

      setReadiness(result);

      if (result.state === "loading" || result.state === "unavailable") {
        pollTimerRef.current = setTimeout(() => {
          void pollReadinessRef.current?.();
        }, 2000);
      }
    } catch (error) {
      if (!pollingActiveRef.current) return;
      logger.error("Failed to poll replay target readiness:", error);
      setReadiness({
        state: "failed",
        candidates: [],
        selectedTarget: null,
        error: "replayTarget.error",
        retryable: true,
      });
    }
  }, [clearPollTimer]);

  useEffect(() => {
    pollReadinessRef.current = pollReadiness;
  }, [pollReadiness]);

  useEffect(() => {
    if (!isOpen) {
      pollingActiveRef.current = false;
      clearPollTimer();
      setReadiness(null);
      return;
    }

    pollingActiveRef.current = true;
    setReadiness(null);
    void pollReadiness();

    return () => {
      pollingActiveRef.current = false;
      clearPollTimer();
    };
  }, [clearPollTimer, isOpen, pollReadiness]);

  const handleRetry = () => {
    pollingActiveRef.current = true;
    clearPollTimer();
    setReadiness(null);
    void pollReadiness();
  };

  const handleSelectTarget = async (summonerName: string) => {
    try {
      await setRecordingTarget(summonerName);
      toast({
        title: t("replayTarget.targetSet"),
        description: t("replayTarget.targetSetDesc", { name: summonerName }),
      });
      onClose();
    } catch (error: unknown) {
      const description =
        error instanceof Error ? error.message : t("replayTarget.error");
      toast({
        title: t("common.error"),
        description,
        variant: "destructive",
      });
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("replayTarget.title")}</DialogTitle>
          <DialogDescription>{t("replayTarget.description")}</DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-2 gap-4 py-4">
          {!readiness || readiness.state === "loading" ? (
            <div className="col-span-2 text-center py-4 flex flex-col items-center gap-2">
              <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-gaming-cyan" />
              {t("replayTarget.loading")}
            </div>
          ) : readiness.state === "unavailable" ? (
            <div className="col-span-2 text-center py-4 flex flex-col items-center gap-4">
              <p>{t("replayTarget.unavailable")}</p>
              <Button variant="outline" onClick={handleRetry}>
                {t("common.retry")}
              </Button>
            </div>
          ) : readiness.state === "empty" ? (
            <div className="col-span-2 text-center py-4 flex flex-col items-center gap-4">
              <p>{t("replayTarget.empty")}</p>
              <Button variant="outline" onClick={handleRetry}>
                {t("common.retry")}
              </Button>
            </div>
          ) : readiness.state === "failed" ? (
            <div className="col-span-2 text-center py-4 flex flex-col items-center gap-4">
              <p className="text-destructive">
                {readiness.error || t("replayTarget.error")}
              </p>
              {readiness.retryable && (
                <Button variant="outline" onClick={handleRetry}>
                  {t("common.retry")}
                </Button>
              )}
            </div>
          ) : (
            <>
              {readiness.candidates.map((player) => (
                <Button
                  key={player.summoner_name}
                  variant={
                    readiness.selectedTarget === player.summoner_name
                      ? "default"
                      : "outline"
                  }
                  className="h-auto py-3 flex flex-col items-start"
                  onClick={() => handleSelectTarget(player.summoner_name)}
                >
                  <span className="font-bold">{player.summoner_name}</span>
                  <span className="text-xs text-muted-foreground">
                    ID: {player.champion_id}
                  </span>
                </Button>
              ))}
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
