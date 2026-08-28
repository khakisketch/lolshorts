import { useCallback } from "react";
import { CanvasTemplate, CanvasElement } from "@/types/autoEdit";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Separator } from "@/components/ui/separator";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Plus,
  Trash2,
  Type,
  Image as ImageIcon,
  Palette,
  Upload,
  AlertCircle,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { CanvasElementProperties } from "./CanvasElementProperties";

interface CanvasControlsPanelProps {
  template: CanvasTemplate;
  selectedElementIndex: number | null;
  onSelectElement: (index: number | null) => void;
  onUpdateTemplate: (updates: Partial<CanvasTemplate>) => void;
}

export function CanvasControlsPanel({
  template,
  selectedElementIndex,
  onSelectElement,
  onUpdateTemplate,
}: CanvasControlsPanelProps) {
  const { t } = useTranslation();

  const setBackgroundColor = useCallback(
    (color: string) => {
      onUpdateTemplate({ background: { type: "Color", value: color } });
    },
    [onUpdateTemplate],
  );

  const setBackgroundGradient = useCallback(
    (color1: string, color2: string) => {
      onUpdateTemplate({
        background: { type: "Gradient", value: `${color1}:${color2}` },
      });
    },
    [onUpdateTemplate],
  );

  const setBackgroundImage = useCallback(
    (path: string) => {
      onUpdateTemplate({ background: { type: "Image", path } });
    },
    [onUpdateTemplate],
  );

  const addTextElement = useCallback(() => {
    const newElement: CanvasElement = {
      type: "Text",
      content: "New Text",
      font: "Arial",
      size: 48,
      color: "#FFFFFF",
      position: { x: 50, y: 50 },
    };
    onUpdateTemplate({ elements: [...template.elements, newElement] });
    onSelectElement(template.elements.length);
  }, [template, onUpdateTemplate, onSelectElement]);

  const addImageElement = useCallback(
    (path: string) => {
      const newElement: CanvasElement = {
        type: "Image",
        path,
        width: 200,
        height: 200,
        position: { x: 50, y: 50 },
      };
      onUpdateTemplate({ elements: [...template.elements, newElement] });
      onSelectElement(template.elements.length);
    },
    [template, onUpdateTemplate, onSelectElement],
  );

  const updateElement = useCallback(
    (index: number, updates: Partial<CanvasElement>) => {
      const newElements = [...template.elements];
      newElements[index] = {
        ...newElements[index],
        ...updates,
      } as CanvasElement;
      onUpdateTemplate({ elements: newElements });
    },
    [template, onUpdateTemplate],
  );

  const deleteElement = useCallback(
    (index: number) => {
      const newElements = template.elements.filter((_, i) => i !== index);
      onUpdateTemplate({ elements: newElements });
      if (selectedElementIndex === index) {
        onSelectElement(null);
      }
    },
    [template, selectedElementIndex, onUpdateTemplate, onSelectElement],
  );

  const selectedElement =
    selectedElementIndex !== null
      ? template.elements[selectedElementIndex]
      : null;

  return (
    <div className="w-80 border-l bg-card overflow-y-auto">
      <Tabs defaultValue="background" className="h-full">
        <TabsList className="w-full justify-start rounded-none border-b">
          <TabsTrigger value="background" className="flex-1">
            <Palette className="w-4 h-4 mr-1" />
            {t("editor.canvas.background")}
          </TabsTrigger>
          <TabsTrigger value="elements" className="flex-1">
            <Plus className="w-4 h-4 mr-1" />
            {t("editor.canvas.elements")}
          </TabsTrigger>
        </TabsList>

        {/* Background Tab */}
        <TabsContent value="background" className="p-4 space-y-4">
          <div className="space-y-2">
            <Label>{t("editor.canvas.backgroundType")}</Label>
            <Select
              value={template.background.type}
              onValueChange={(value: "Color" | "Gradient" | "Image") => {
                if (value === "Color") setBackgroundColor("#000000");
                else if (value === "Gradient")
                  setBackgroundGradient("#000000", "#333333");
              }}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="Color">
                  {t("editor.canvas.bgSolid")}
                </SelectItem>
                <SelectItem value="Gradient">
                  {t("editor.canvas.bgGradient")}
                </SelectItem>
                <SelectItem value="Image">
                  {t("editor.canvas.bgImage")}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          {template.background.type === "Color" && (
            <div className="space-y-2">
              <Label>{t("editor.canvas.color")}</Label>
              <Input
                type="color"
                value={template.background.value}
                onChange={(e) => setBackgroundColor(e.target.value)}
              />
            </div>
          )}

          {template.background.type === "Gradient" && (
            <>
              <div className="space-y-2">
                <Label>{t("editor.canvas.color1")}</Label>
                <Input
                  type="color"
                  value={
                    template.background.type === "Gradient"
                      ? template.background.value.split(":")[0]
                      : "#000000"
                  }
                  onChange={(e) => {
                    if (template.background.type === "Gradient") {
                      const color2 = template.background.value.split(":")[1];
                      setBackgroundGradient(e.target.value, color2);
                    }
                  }}
                />
              </div>
              <div className="space-y-2">
                <Label>{t("editor.canvas.color2")}</Label>
                <Input
                  type="color"
                  value={
                    template.background.type === "Gradient"
                      ? template.background.value.split(":")[1]
                      : "#000000"
                  }
                  onChange={(e) => {
                    if (template.background.type === "Gradient") {
                      const color1 = template.background.value.split(":")[0];
                      setBackgroundGradient(color1, e.target.value);
                    }
                  }}
                />
              </div>
            </>
          )}

          {template.background.type === "Image" && (
            <div className="space-y-2">
              <Label>{t("editor.canvas.imagePath")}</Label>
              <div className="flex gap-2">
                <Input
                  placeholder={t("editor.canvas.imagePathPlaceholder")}
                  value={template.background.path || ""}
                  onChange={(e) => setBackgroundImage(e.target.value)}
                />
                <Button size="icon" variant="outline">
                  <Upload className="w-4 h-4" />
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                {t("editor.canvas.imageHint")}
              </p>
            </div>
          )}
        </TabsContent>

        {/* Elements Tab */}
        <TabsContent value="elements" className="p-4 space-y-4">
          <div className="space-y-2">
            <Button
              onClick={addTextElement}
              className="w-full"
              variant="outline"
              data-testid="add-text-button"
            >
              <Type className="w-4 h-4 mr-2" />
              {t("editor.canvas.addText")}
            </Button>
            <Button
              onClick={() => {
                const path = prompt(t("editor.canvas.enterImagePath"));
                if (path) addImageElement(path);
              }}
              className="w-full"
              variant="outline"
              data-testid="add-image-button"
            >
              <ImageIcon className="w-4 h-4 mr-2" />
              {t("editor.canvas.addImage")}
            </Button>
          </div>

          <Separator />

          {/* Element list */}
          <div className="space-y-2">
            <Label>
              {t("editor.canvas.elementsCount", {
                count: template.elements.length,
              })}
            </Label>
            {template.elements.length === 0 ? (
              <Alert>
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>
                  {t("editor.canvas.noElements")}
                </AlertDescription>
              </Alert>
            ) : (
              <div className="space-y-2">
                {template.elements.map((element, index) => (
                  <div
                    key={index}
                    className={`bg-black/40 rounded-lg border border-white/5 p-3 cursor-pointer transition-all ${
                      selectedElementIndex === index
                        ? "ring-2 ring-primary"
                        : ""
                    }`}
                    role="button"
                    tabIndex={0}
                    onClick={() => onSelectElement(index)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        onSelectElement(index);
                      }
                    }}
                    data-testid={`element-${index}`}
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        {element.type === "Text" ? (
                          <Type className="w-4 h-4" />
                        ) : (
                          <ImageIcon className="w-4 h-4" />
                        )}
                        <span className="text-sm truncate">
                          {element.type === "Text"
                            ? element.content
                            : t("editor.canvas.bgImage")}
                        </span>
                      </div>
                      <Button
                        size="icon"
                        variant="ghost"
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteElement(index);
                        }}
                        data-testid={`delete-element-${index}`}
                      >
                        <Trash2 className="w-4 h-4 text-destructive" />
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Selected element properties */}
          {selectedElement && selectedElementIndex !== null && (
            <>
              <Separator />
              <CanvasElementProperties
                element={selectedElement}
                elementIndex={selectedElementIndex}
                onUpdate={updateElement}
              />
            </>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
