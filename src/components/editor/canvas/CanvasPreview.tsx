import { useRef } from "react";
import { CanvasTemplate } from "@/types/autoEdit";
import { useTranslation } from "react-i18next";

interface CanvasPreviewProps {
  template: CanvasTemplate;
  selectedElementIndex: number | null;
  onCanvasClick: (e: React.MouseEvent<HTMLDivElement>) => void;
  onElementClick: (index: number) => void;
}

export function CanvasPreview({
  template,
  selectedElementIndex,
  onCanvasClick,
  onElementClick,
}: CanvasPreviewProps) {
  const { t } = useTranslation();
  const canvasRef = useRef<HTMLDivElement>(null);

  return (
    <div className="flex-1 p-6 flex items-center justify-center bg-muted/20">
      <div className="relative">
        {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions, jsx-a11y/click-events-have-key-events */}
        <div
          ref={canvasRef}
          className="relative bg-black rounded-lg overflow-hidden shadow-2xl cursor-crosshair"
          style={{
            width: "360px",
            height: "640px",
            aspectRatio: "9/16",
          }}
          onClick={onCanvasClick}
          data-testid="canvas-preview"
        >
          {/* Background layer */}
          <div
            className="absolute inset-0"
            style={{
              background:
                template.background.type === "Color"
                  ? template.background.value
                  : template.background.type === "Gradient"
                    ? `linear-gradient(${template.background.value.split(":").join(", ")})`
                    : undefined,
              backgroundImage:
                template.background.type === "Image"
                  ? `url(${template.background.path})`
                  : undefined,
              backgroundSize: "cover",
              backgroundPosition: "center",
            }}
          />

          {/* Elements layer */}
          {template.elements.map((element, index) => (
            <div
              key={index}
              role="button"
              tabIndex={0}
              aria-label={`Select ${element.type === "Text" ? "text" : "image"} element ${index + 1}`}
              className={`absolute cursor-pointer transition-all ${
                selectedElementIndex === index ? "ring-2 ring-primary" : ""
              }`}
              data-element-id={String(index)}
              style={{
                left: `${element.position.x}%`,
                top: `${element.position.y}%`,
                transform: "translate(-50%, -50%)",
              }}
              onClick={(e) => {
                e.stopPropagation();
                onElementClick(index);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  e.stopPropagation();
                  onElementClick(index);
                }
              }}
            >
              {element.type === "Text" ? (
                <div
                  style={{
                    fontFamily: element.font,
                    fontSize: `${element.size}px`,
                    color: element.color,
                    textShadow: element.outline
                      ? `0 0 4px ${element.outline}, 0 0 8px ${element.outline}`
                      : undefined,
                    whiteSpace: "nowrap",
                  }}
                >
                  {element.content}
                </div>
              ) : (
                <img
                  src={element.path}
                  alt="Canvas element"
                  style={{
                    width: `${element.width}px`,
                    height: `${element.height}px`,
                    objectFit: "contain",
                  }}
                />
              )}
            </div>
          ))}

          {/* Hint overlay */}
          {template.elements.length === 0 && (
            <div className="absolute inset-0 flex items-center justify-center">
              <div className="text-center text-white/50 p-4">
                <p className="text-sm">{t("editor.canvas.emptyHint")}</p>
                <p className="text-xs mt-1">
                  {t("editor.canvas.positionHint")}
                </p>
              </div>
            </div>
          )}
        </div>

        {/* Dimensions label */}
        <div className="text-center mt-2 text-xs text-muted-foreground">
          {t("editor.canvas.dimensions")}
        </div>
      </div>
    </div>
  );
}
