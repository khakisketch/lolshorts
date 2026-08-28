import { useState, useCallback, useMemo } from "react";
import { CanvasTemplate, CanvasElement } from "@/types/autoEdit";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { AlertCircle, Save, Loader2 } from "lucide-react";
import { useAutoEdit } from "@/hooks/useAutoEdit";
import { TemplateLibrary } from "../TemplateLibrary";
import { CanvasToolbar } from "./CanvasToolbar";
import { CanvasPreview } from "./CanvasPreview";
import { CanvasControlsPanel } from "./CanvasControlsPanel";
import { useTranslation } from "react-i18next";
import { getErrorMessage } from "@/lib/utils";
import { logger } from "@/lib/logger";

interface CanvasEditorProps {
  template: CanvasTemplate | null;
  onTemplateChange: (template: CanvasTemplate) => void;
}

export function CanvasEditor({
  template,
  onTemplateChange,
}: CanvasEditorProps) {
  const { t } = useTranslation();
  const { saveCanvasTemplate } = useAutoEdit();
  const [selectedElementIndex, setSelectedElementIndex] = useState<
    number | null
  >(null);
  const [showLibrary, setShowLibrary] = useState(false);
  const [showSaveDialog, setShowSaveDialog] = useState(false);
  const [saveTemplateName, setSaveTemplateName] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const currentTemplate: CanvasTemplate = useMemo(
    () =>
      template || {
        id: `template_${Date.now()}`,
        name: "New Template",
        background: { type: "Color", value: "#000000" },
        elements: [],
      },
    [template],
  );

  const updateTemplate = useCallback(
    (updates: Partial<CanvasTemplate>) => {
      onTemplateChange({ ...currentTemplate, ...updates });
    },
    [currentTemplate, onTemplateChange],
  );

  const handleCanvasClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (selectedElementIndex === null) return;
      const canvas = e.currentTarget;
      const rect = canvas.getBoundingClientRect();
      const x = ((e.clientX - rect.left) / rect.width) * 100;
      const y = ((e.clientY - rect.top) / rect.height) * 100;

      const newElements = [...currentTemplate.elements];
      newElements[selectedElementIndex] = {
        ...newElements[selectedElementIndex],
        position: {
          x: Math.max(0, Math.min(100, x)),
          y: Math.max(0, Math.min(100, y)),
        },
      } as CanvasElement;
      onTemplateChange({ ...currentTemplate, elements: newElements });
    },
    [selectedElementIndex, currentTemplate, onTemplateChange],
  );

  const handleSaveTemplate = useCallback(async () => {
    setSaveError(null);
    setIsSaving(true);
    try {
      const templateToSave: CanvasTemplate = {
        ...currentTemplate,
        name: saveTemplateName || currentTemplate.name,
      };
      await saveCanvasTemplate(templateToSave);
      setShowSaveDialog(false);
      setSaveTemplateName("");
    } catch (err) {
      logger.error("Failed to save template:", err);
      setSaveError(getErrorMessage(err));
    } finally {
      setIsSaving(false);
    }
  }, [currentTemplate, saveTemplateName, saveCanvasTemplate]);

  const openSaveDialog = useCallback(() => {
    setSaveTemplateName(currentTemplate.name);
    setSaveError(null);
    setShowSaveDialog(true);
  }, [currentTemplate.name]);

  const handleLoadTemplate = useCallback(
    (loadedTemplate: CanvasTemplate) => {
      onTemplateChange(loadedTemplate);
    },
    [onTemplateChange],
  );

  return (
    <div
      className="flex flex-col h-full overflow-hidden"
      data-testid="canvas-editor"
    >
      <CanvasToolbar
        onLoadTemplate={() => setShowLibrary(true)}
        onSaveTemplate={openSaveDialog}
      />

      <div className="flex flex-1 overflow-hidden">
        <CanvasPreview
          template={currentTemplate}
          selectedElementIndex={selectedElementIndex}
          onCanvasClick={handleCanvasClick}
          onElementClick={setSelectedElementIndex}
        />

        <CanvasControlsPanel
          template={currentTemplate}
          selectedElementIndex={selectedElementIndex}
          onSelectElement={setSelectedElementIndex}
          onUpdateTemplate={updateTemplate}
        />
      </div>

      <TemplateLibrary
        open={showLibrary}
        onOpenChange={setShowLibrary}
        onTemplateSelect={handleLoadTemplate}
      />

      <Dialog open={showSaveDialog} onOpenChange={setShowSaveDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("autoEdit.saveTemplate")}</DialogTitle>
            <DialogDescription>
              {t("autoEdit.saveTemplateDescription")}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="template-name">
                {t("autoEdit.templateName")}
              </Label>
              <Input
                id="template-name"
                placeholder={t("autoEdit.templateNamePlaceholder")}
                value={saveTemplateName}
                onChange={(e) => setSaveTemplateName(e.target.value)}
                data-testid="template-name-input"
              />
            </div>

            {saveError && (
              <Alert variant="destructive">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>{saveError}</AlertDescription>
              </Alert>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowSaveDialog(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              onClick={handleSaveTemplate}
              disabled={isSaving || !saveTemplateName.trim()}
              data-testid="confirm-save-button"
            >
              {isSaving ? (
                <>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  {t("common.saving")}
                </>
              ) : (
                <>
                  <Save className="w-4 h-4 mr-2" />
                  {t("common.save")}
                </>
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
