import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { YouTubeUpload } from "@/components/youtube/YouTubeUpload";
import { YouTubeAuth } from "@/components/youtube/YouTubeAuth";
import { YouTubeHistory } from "@/components/youtube/YouTubeHistory";
import { QuotaDisplay } from "@/components/youtube/QuotaDisplay";
import { FormErrorBoundary } from "@/components/ErrorBoundary";
import { ProtectedFeature } from "@/components/auth/ProtectedFeature";

interface ShareDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Absolute path of the video that should be pre-filled in the upload form. */
  videoPath?: string;
  /** Auto-edit result id, so the upload can be written back onto the result. */
  resultId?: string;
}

/**
 * Sharing is no longer a top-level screen: it is entered from a single item in
 * the results library. This dialog hosts the existing YouTube upload UI
 * (upload / account / history) without sending the user to another page.
 *
 * Uploading is free for signed-in users — only scheduled and batch uploads are
 * PRO — so nothing here is gated behind an entitlement.
 */
export function ShareDialog({
  open,
  onOpenChange,
  videoPath,
  resultId,
}: ShareDialogProps) {
  const { t } = useTranslation();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-w-4xl max-h-[85vh] overflow-y-auto"
        data-testid="share-dialog"
      >
        <DialogHeader>
          <DialogTitle>{t("results.shareDialog.title")}</DialogTitle>
          <DialogDescription style={{ wordBreak: "keep-all" }}>
            {t("results.shareDialog.description")}
          </DialogDescription>
        </DialogHeader>

        <ProtectedFeature requiresPro={false} featureName="YouTube upload">
          <Tabs defaultValue="upload" className="mt-2">
            <TabsList className="grid w-full grid-cols-3 h-auto">
              <TabsTrigger value="upload" className="min-h-[44px]">
                {t("results.shareDialog.tabs.upload")}
              </TabsTrigger>
              <TabsTrigger value="account" className="min-h-[44px]">
                {t("results.shareDialog.tabs.account")}
              </TabsTrigger>
              <TabsTrigger value="history" className="min-h-[44px]">
                {t("results.shareDialog.tabs.history")}
              </TabsTrigger>
            </TabsList>

            <TabsContent value="upload" className="mt-4 space-y-4">
              <FormErrorBoundary>
                <YouTubeUpload initialPath={videoPath} resultId={resultId} />
              </FormErrorBoundary>
              <QuotaDisplay />
            </TabsContent>

            <TabsContent value="account" className="mt-4 space-y-4">
              <YouTubeAuth />
            </TabsContent>

            <TabsContent value="history" className="mt-4">
              <YouTubeHistory />
            </TabsContent>
          </Tabs>
        </ProtectedFeature>
      </DialogContent>
    </Dialog>
  );
}
