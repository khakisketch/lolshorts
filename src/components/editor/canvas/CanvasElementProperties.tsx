import { CanvasElement } from "@/types/autoEdit";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useTranslation } from "react-i18next";

interface CanvasElementPropertiesProps {
  element: CanvasElement;
  elementIndex: number;
  onUpdate: (index: number, updates: Partial<CanvasElement>) => void;
}

export function CanvasElementProperties({
  element,
  elementIndex,
  onUpdate,
}: CanvasElementPropertiesProps) {
  const { t } = useTranslation();

  return (
    <div className="space-y-3">
      <Label>{t("editor.canvas.elementProperties")}</Label>

      {element.type === "Text" && (
        <>
          <div className="space-y-2">
            <Label>{t("editor.canvas.textContent")}</Label>
            <Input
              value={element.content}
              onChange={(e) =>
                onUpdate(elementIndex, {
                  content: e.target.value,
                } as Partial<CanvasElement>)
              }
              data-testid="text-content-input"
            />
          </div>
          <div className="space-y-2">
            <Label>{t("editor.canvas.fontSize")}</Label>
            <Input
              type="number"
              value={element.size}
              onChange={(e) =>
                onUpdate(elementIndex, {
                  size: parseInt(e.target.value),
                } as Partial<CanvasElement>)
              }
              data-testid="text-size-input"
            />
          </div>
          <div className="space-y-2">
            <Label>{t("editor.canvas.color")}</Label>
            <Input
              type="color"
              value={element.color}
              onChange={(e) =>
                onUpdate(elementIndex, {
                  color: e.target.value,
                } as Partial<CanvasElement>)
              }
              data-testid="text-color-input"
            />
          </div>
          <div className="space-y-2">
            <Label>{t("editor.canvas.outlineColor")}</Label>
            <Input
              type="color"
              value={element.outline || "#000000"}
              onChange={(e) =>
                onUpdate(elementIndex, {
                  outline: e.target.value,
                } as Partial<CanvasElement>)
              }
              data-testid="text-outline-input"
            />
          </div>
        </>
      )}

      {element.type === "Image" && (
        <>
          <div className="space-y-2">
            <Label>{t("editor.canvas.width")}</Label>
            <Input
              type="number"
              value={element.width}
              onChange={(e) =>
                onUpdate(elementIndex, {
                  width: parseInt(e.target.value),
                } as Partial<CanvasElement>)
              }
              data-testid="image-width-input"
            />
          </div>
          <div className="space-y-2">
            <Label>{t("editor.canvas.height")}</Label>
            <Input
              type="number"
              value={element.height}
              onChange={(e) =>
                onUpdate(elementIndex, {
                  height: parseInt(e.target.value),
                } as Partial<CanvasElement>)
              }
              data-testid="image-height-input"
            />
          </div>
        </>
      )}

      <div className="grid grid-cols-2 gap-2">
        <div className="space-y-2">
          <Label>{t("editor.canvas.xPosition")}</Label>
          <Input
            type="number"
            min="0"
            max="100"
            value={Math.round(element.position.x)}
            onChange={(e) =>
              onUpdate(elementIndex, {
                position: { ...element.position, x: parseInt(e.target.value) },
              } as Partial<CanvasElement>)
            }
            data-testid="position-x-input"
          />
        </div>
        <div className="space-y-2">
          <Label>{t("editor.canvas.yPosition")}</Label>
          <Input
            type="number"
            min="0"
            max="100"
            value={Math.round(element.position.y)}
            onChange={(e) =>
              onUpdate(elementIndex, {
                position: { ...element.position, y: parseInt(e.target.value) },
              } as Partial<CanvasElement>)
            }
            data-testid="position-y-input"
          />
        </div>
      </div>
    </div>
  );
}
