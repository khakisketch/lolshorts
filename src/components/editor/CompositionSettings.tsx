import { useTranslation } from "react-i18next";
import { useEditorStore } from "@/stores/editorStore";
import type { AspectRatio, TransitionType } from "@/stores/editorStore";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { formatDuration } from "@/lib/utils";
import { Download, Settings2, Film } from "lucide-react";

interface CompositionSettingsProps {
  onExport?: () => void;
}

export function CompositionSettings({ onExport }: CompositionSettingsProps) {
  const { t } = useTranslation();
  const {
    compositionSettings,
    setAspectRatio,
    setTransitionType,
    setTransitionDuration,
    timelineClips,
    totalDuration,
  } = useEditorStore();

  const aspectRatioOptions: Array<{
    value: AspectRatio;
    label: string;
    description: string;
  }> = [
    {
      value: "9:16",
      label: "9:16",
      description: t("editor.composition.aspectRatios.vertical"),
    },
    {
      value: "16:9",
      label: "16:9",
      description: t("editor.composition.aspectRatios.horizontal"),
    },
    {
      value: "1:1",
      label: "1:1",
      description: t("editor.composition.aspectRatios.square"),
    },
  ];

  const transitionOptions: Array<{ value: TransitionType; label: string }> = [
    { value: "none", label: t("editor.composition.transitionTypes.none") },
    { value: "fade", label: t("editor.composition.transitionTypes.fade") },
    { value: "slide", label: t("editor.composition.transitionTypes.slide") },
  ];

  const handleExport = () => {
    if (timelineClips.length === 0 || !onExport) {
      return;
    }
    onExport();
  };

  const getAspectRatioIcon = (ratio: AspectRatio) => {
    switch (ratio) {
      case "9:16":
        return "📱";
      case "16:9":
        return "🖥️";
      case "1:1":
        return "⬜";
    }
  };

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="p-4 border-b">
        <div className="flex items-center gap-2">
          <Settings2 className="w-5 h-5" />
          <h3 className="font-semibold">{t("editor.composition.title")}</h3>
        </div>
      </div>

      {/* Settings Content */}
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {/* Aspect Ratio Section */}
        <div className="bg-black/40 rounded-lg border border-white/5 p-4">
          <div className="mb-4">
            <h3 className="text-lg font-semibold">
              {t("editor.composition.aspectRatio")}
            </h3>
          </div>
          <div>
            <RadioGroup
              value={compositionSettings.aspectRatio}
              onValueChange={(value) => setAspectRatio(value as AspectRatio)}
            >
              {aspectRatioOptions.map((option) => (
                <div
                  key={option.value}
                  className="flex items-center space-x-3 space-y-0"
                >
                  <RadioGroupItem value={option.value} id={option.value} />
                  <Label
                    htmlFor={option.value}
                    className="font-normal cursor-pointer flex-1"
                  >
                    <div className="flex items-center justify-between">
                      <div>
                        <span className="font-medium">{option.label}</span>
                        <p className="text-xs text-muted-foreground">
                          {option.description}
                        </p>
                      </div>
                      <span className="text-xl">
                        {getAspectRatioIcon(option.value)}
                      </span>
                    </div>
                  </Label>
                </div>
              ))}
            </RadioGroup>
          </div>
        </div>

        {/* Transition Section */}
        <div className="bg-black/40 rounded-lg border border-white/5 p-4">
          <div className="mb-4">
            <h3 className="text-lg font-semibold">
              {t("editor.composition.transitions")}
            </h3>
          </div>
          <div className="space-y-4">
            {/* Transition Type */}
            <div className="space-y-2">
              <Label htmlFor="transition-type">
                {t("editor.composition.type")}
              </Label>
              <Select
                value={compositionSettings.transitionType}
                onValueChange={(value) =>
                  setTransitionType(value as TransitionType)
                }
              >
                <SelectTrigger id="transition-type">
                  <SelectValue placeholder="Select transition" />
                </SelectTrigger>
                <SelectContent>
                  {transitionOptions.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {/* Transition Duration */}
            {compositionSettings.transitionType !== "none" && (
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label htmlFor="transition-duration">
                    {t("editor.composition.duration")}
                  </Label>
                  <span className="text-sm text-muted-foreground">
                    {compositionSettings.transitionDuration.toFixed(1)}s
                  </span>
                </div>
                <Slider
                  id="transition-duration"
                  min={0.1}
                  max={2.0}
                  step={0.1}
                  value={[compositionSettings.transitionDuration]}
                  onValueChange={(value) => setTransitionDuration(value[0])}
                  className="w-full"
                />
              </div>
            )}
          </div>
        </div>

        {/* Summary Section */}
        <div className="bg-black/40 rounded-lg border border-white/5 p-4">
          <div className="mb-4">
            <h3 className="text-lg font-semibold flex items-center gap-2">
              <Film className="w-4 h-4" />
              {t("editor.composition.summary")}
            </h3>
          </div>
          <div className="space-y-3">
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">
                {t("editor.composition.totalClips")}
              </span>
              <Badge variant="secondary">{timelineClips.length}</Badge>
            </div>
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">
                {t("editor.composition.totalDuration")}
              </span>
              <Badge variant="outline">{formatDuration(totalDuration)}</Badge>
            </div>
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">
                {t("editor.composition.aspectRatio")}
              </span>
              <Badge variant="outline">{compositionSettings.aspectRatio}</Badge>
            </div>
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">
                {t("editor.composition.transitions")}
              </span>
              <Badge variant="outline">
                {compositionSettings.transitionType === "none"
                  ? t("editor.composition.transitionTypes.none")
                  : `${compositionSettings.transitionType} (${compositionSettings.transitionDuration}s)`}
              </Badge>
            </div>
          </div>
        </div>
      </div>

      <Separator />

      {/* Export Button */}
      <div className="p-4">
        <Button
          size="lg"
          className="w-full"
          onClick={handleExport}
          disabled={timelineClips.length === 0}
        >
          <Download className="w-4 h-4 mr-2" />
          {t("editor.composition.exportVideo")}
        </Button>
        {timelineClips.length === 0 && (
          <p className="text-xs text-muted-foreground text-center mt-2">
            {t("editor.composition.addClipsToExport")}
          </p>
        )}
      </div>
    </div>
  );
}
