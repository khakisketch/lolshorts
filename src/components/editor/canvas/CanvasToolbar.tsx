import { Button } from "@/components/ui/button";
import { FolderOpen, Save } from "lucide-react";
import { useTranslation } from "react-i18next";

interface CanvasToolbarProps {
  onLoadTemplate: () => void;
  onSaveTemplate: () => void;
}

export function CanvasToolbar({
  onLoadTemplate,
  onSaveTemplate,
}: CanvasToolbarProps) {
  const { t } = useTranslation();

  return (
    <div className="p-4 border-b">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="font-semibold text-lg">
            {t("autoEdit.canvasEditor")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("autoEdit.canvasEditorDescription")}
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={onLoadTemplate}
            data-testid="load-template-button"
          >
            <FolderOpen className="w-4 h-4 mr-2" />
            {t("autoEdit.loadTemplate")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={onSaveTemplate}
            data-testid="save-template-button"
          >
            <Save className="w-4 h-4 mr-2" />
            {t("autoEdit.saveTemplate")}
          </Button>
        </div>
      </div>
    </div>
  );
}
