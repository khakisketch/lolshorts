import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "@/components/ui/use-toast";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Slider } from "@/components/ui/slider";
import { Separator } from "@/components/ui/separator";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Loader2, Gauge, Palette, Type, Sparkles, Layers } from "lucide-react";
import { getErrorMessage } from "@/lib/utils";
import { logger } from "@/lib/logger";

interface EffectsPanelProps {
  inputPath: string | null;
  outputDir: string | null;
}

export function EffectsPanel({ inputPath, outputDir }: EffectsPanelProps) {
  const { t } = useTranslation();
  const { toast } = useToast();

  // Slow motion state
  const [slowMotionSpeed, setSlowMotionSpeed] = useState(0.5);
  const [isApplyingSlowMotion, setIsApplyingSlowMotion] = useState(false);

  // Color grading state
  const [brightness, setBrightness] = useState(0.0);
  const [contrast, setContrast] = useState(1.0);
  const [saturation, setSaturation] = useState(1.0);
  const [isApplyingColorGrading, setIsApplyingColorGrading] = useState(false);

  // Text overlay state
  const [overlayText, setOverlayText] = useState("");
  const [textPosition, setTextPosition] = useState("center");
  const [textSize, setTextSize] = useState(48);
  const [textColor, setTextColor] = useState("WHITE");
  const [isApplyingText, setIsApplyingText] = useState(false);

  // Chained effects state
  const [isApplyingAll, setIsApplyingAll] = useState(false);

  const generateOutputPath = (suffix: string) => {
    if (!inputPath || !outputDir) return null;
    const timestamp = Date.now();
    return `${outputDir}/effect_${suffix}_${timestamp}.mp4`;
  };

  const handleApplySlowMotion = async () => {
    const output = generateOutputPath("slowmo");
    if (!inputPath || !output) return;

    setIsApplyingSlowMotion(true);
    try {
      const result = await invoke<string>("apply_slow_motion_cmd", {
        input: inputPath,
        output,
        speedFactor: slowMotionSpeed,
      });
      toast({
        title: t("editor.effects.slowMotionApplied", "Slow motion applied"),
        description: result,
      });
    } catch (error) {
      logger.error("Failed to apply slow motion:", error);
      toast({
        title: t("toast.error"),
        description: getErrorMessage(error),
        variant: "destructive",
      });
    } finally {
      setIsApplyingSlowMotion(false);
    }
  };

  const handleApplyColorGrading = async () => {
    const output = generateOutputPath("color");
    if (!inputPath || !output) return;

    setIsApplyingColorGrading(true);
    try {
      const result = await invoke<string>("apply_color_grading_cmd", {
        input: inputPath,
        output,
        brightness,
        contrast,
        saturation,
      });
      toast({
        title: t("editor.effects.colorGradingApplied", "Color grading applied"),
        description: result,
      });
    } catch (error) {
      logger.error("Failed to apply color grading:", error);
      toast({
        title: t("toast.error"),
        description: getErrorMessage(error),
        variant: "destructive",
      });
    } finally {
      setIsApplyingColorGrading(false);
    }
  };

  const handleApplyTextOverlay = async () => {
    const output = generateOutputPath("text");
    if (!inputPath || !output || !overlayText.trim()) return;

    setIsApplyingText(true);
    try {
      const result = await invoke<string>("apply_text_overlay_cmd", {
        input: inputPath,
        output,
        text: overlayText,
        position: textPosition,
        size: textSize,
        color: textColor,
      });
      toast({
        title: t("editor.effects.textOverlayApplied", "Text overlay applied"),
        description: result,
      });
    } catch (error) {
      logger.error("Failed to apply text overlay:", error);
      toast({
        title: t("toast.error"),
        description: getErrorMessage(error),
        variant: "destructive",
      });
    } finally {
      setIsApplyingText(false);
    }
  };

  const handleApplyAllEffects = async () => {
    if (!hasAnyEffect) return;
    const output = generateOutputPath("chained");
    if (!inputPath || !output) return;

    setIsApplyingAll(true);
    try {
      const effects: Record<string, unknown> = {};

      // Include slow motion if not at default (1.0x means no slow motion)
      if (slowMotionSpeed < 1.0) {
        effects.slow_motion = slowMotionSpeed;
      }

      // Include color grading if any value differs from default
      if (brightness !== 0.0 || contrast !== 1.0 || saturation !== 1.0) {
        effects.color_grading = {
          brightness,
          contrast,
          saturation,
          gamma: 1.0,
          vignette: false,
        };
      }

      // Include text overlay if text is provided
      if (overlayText.trim()) {
        effects.text_overlay = {
          text: overlayText,
          position: textPosition,
          size: textSize,
          color: textColor,
        };
      }

      const result = await invoke<string>("apply_chained_effects_cmd", {
        input: inputPath,
        output,
        effects,
      });
      toast({
        title: t("editor.effects.allEffectsApplied", "All effects applied"),
        description: result,
      });
    } catch (error) {
      logger.error("Failed to apply chained effects:", error);
      toast({
        title: t("toast.error"),
        description: getErrorMessage(error),
        variant: "destructive",
      });
    } finally {
      setIsApplyingAll(false);
    }
  };

  const hasAnyEffect =
    slowMotionSpeed < 1.0 ||
    brightness !== 0.0 ||
    contrast !== 1.0 ||
    saturation !== 1.0 ||
    overlayText.trim().length > 0;

  const isDisabled = !inputPath || !outputDir;

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="p-4 border-b">
        <div className="flex items-center gap-2">
          <Sparkles className="w-5 h-5" />
          <h3 className="font-semibold">
            {t("editor.effects.title", "Video Effects")}
          </h3>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {isDisabled && (
          <Alert>
            <AlertDescription>
              {t(
                "editor.effects.selectClipFirst",
                "Select a clip to apply effects.",
              )}
            </AlertDescription>
          </Alert>
        )}

        {/* Slow Motion */}
        <div className="bg-black/40 rounded-lg border border-white/5 p-4 space-y-4">
          <div className="flex items-center gap-2">
            <Gauge className="w-4 h-4" />
            <h4 className="font-medium">
              {t("editor.effects.slowMotion", "Slow Motion")}
            </h4>
          </div>
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label htmlFor="slow-motion-speed">
                {t("editor.effects.speed", "Speed")}
              </Label>
              <span className="text-sm text-muted-foreground">
                {slowMotionSpeed.toFixed(2)}x
              </span>
            </div>
            <Slider
              id="slow-motion-speed"
              min={0.25}
              max={0.75}
              step={0.05}
              value={[slowMotionSpeed]}
              onValueChange={(value) => setSlowMotionSpeed(value[0])}
              disabled={isDisabled}
            />
            <p className="text-xs text-muted-foreground">
              {t(
                "editor.effects.slowMotionHint",
                "0.25x = very slow, 0.75x = slightly slow",
              )}
            </p>
          </div>
          <Button
            size="sm"
            onClick={handleApplySlowMotion}
            disabled={isDisabled || isApplyingSlowMotion}
            className="w-full"
          >
            {isApplyingSlowMotion ? (
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Gauge className="w-4 h-4 mr-2" />
            )}
            {t("editor.effects.applySlowMotion", "Apply Slow Motion")}
          </Button>
        </div>

        <Separator />

        {/* Color Grading */}
        <div className="bg-black/40 rounded-lg border border-white/5 p-4 space-y-4">
          <div className="flex items-center gap-2">
            <Palette className="w-4 h-4" />
            <h4 className="font-medium">
              {t("editor.effects.colorGrading", "Color Grading")}
            </h4>
          </div>
          <div className="space-y-3">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label htmlFor="brightness">
                  {t("editor.effects.brightness", "Brightness")}
                </Label>
                <span className="text-sm text-muted-foreground">
                  {brightness.toFixed(2)}
                </span>
              </div>
              <Slider
                id="brightness"
                min={-1.0}
                max={1.0}
                step={0.05}
                value={[brightness]}
                onValueChange={(value) => setBrightness(value[0])}
                disabled={isDisabled}
              />
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label htmlFor="contrast">
                  {t("editor.effects.contrast", "Contrast")}
                </Label>
                <span className="text-sm text-muted-foreground">
                  {contrast.toFixed(2)}
                </span>
              </div>
              <Slider
                id="contrast"
                min={0.0}
                max={2.0}
                step={0.05}
                value={[contrast]}
                onValueChange={(value) => setContrast(value[0])}
                disabled={isDisabled}
              />
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label htmlFor="saturation">
                  {t("editor.effects.saturation", "Saturation")}
                </Label>
                <span className="text-sm text-muted-foreground">
                  {saturation.toFixed(2)}
                </span>
              </div>
              <Slider
                id="saturation"
                min={0.0}
                max={2.0}
                step={0.05}
                value={[saturation]}
                onValueChange={(value) => setSaturation(value[0])}
                disabled={isDisabled}
              />
            </div>
          </div>
          <Button
            size="sm"
            onClick={handleApplyColorGrading}
            disabled={isDisabled || isApplyingColorGrading}
            className="w-full"
          >
            {isApplyingColorGrading ? (
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Palette className="w-4 h-4 mr-2" />
            )}
            {t("editor.effects.applyColorGrading", "Apply Color Grading")}
          </Button>
        </div>

        <Separator />

        {/* Text Overlay */}
        <div className="bg-black/40 rounded-lg border border-white/5 p-4 space-y-4">
          <div className="flex items-center gap-2">
            <Type className="w-4 h-4" />
            <h4 className="font-medium">
              {t("editor.effects.textOverlay", "Text Overlay")}
            </h4>
          </div>
          <div className="space-y-3">
            <div className="space-y-2">
              <Label htmlFor="overlay-text">
                {t("editor.effects.text", "Text")}
              </Label>
              <Input
                id="overlay-text"
                value={overlayText}
                onChange={(e) => setOverlayText(e.target.value)}
                placeholder={t(
                  "editor.effects.textPlaceholder",
                  "Enter overlay text...",
                )}
                disabled={isDisabled}
              />
            </div>
            <div className="flex gap-3">
              <div className="flex-1 space-y-2">
                <Label htmlFor="text-position">
                  {t("editor.effects.position", "Position")}
                </Label>
                <Select
                  value={textPosition}
                  onValueChange={setTextPosition}
                  disabled={isDisabled}
                >
                  <SelectTrigger id="text-position" className="h-8 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="topleft">
                      {t("editor.effects.positions.topLeft", "Top Left")}
                    </SelectItem>
                    <SelectItem value="topright">
                      {t("editor.effects.positions.topRight", "Top Right")}
                    </SelectItem>
                    <SelectItem value="center">
                      {t("editor.effects.positions.center", "Center")}
                    </SelectItem>
                    <SelectItem value="bottomleft">
                      {t("editor.effects.positions.bottomLeft", "Bottom Left")}
                    </SelectItem>
                    <SelectItem value="bottomright">
                      {t(
                        "editor.effects.positions.bottomRight",
                        "Bottom Right",
                      )}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="flex-1 space-y-2">
                <Label htmlFor="text-color">
                  {t("editor.effects.color", "Color")}
                </Label>
                <Select
                  value={textColor}
                  onValueChange={setTextColor}
                  disabled={isDisabled}
                >
                  <SelectTrigger id="text-color" className="h-8 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="WHITE">White</SelectItem>
                    <SelectItem value="YELLOW">Yellow</SelectItem>
                    <SelectItem value="RED">Red</SelectItem>
                    <SelectItem value="GREEN">Green</SelectItem>
                    <SelectItem value="BLUE">Blue</SelectItem>
                    <SelectItem value="BLACK">Black</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label htmlFor="text-size">
                  {t("editor.effects.fontSize", "Font Size")}
                </Label>
                <span className="text-sm text-muted-foreground">
                  {textSize}px
                </span>
              </div>
              <Slider
                id="text-size"
                min={16}
                max={128}
                step={4}
                value={[textSize]}
                onValueChange={(value) => setTextSize(value[0])}
                disabled={isDisabled}
              />
            </div>
          </div>
          <Button
            size="sm"
            onClick={handleApplyTextOverlay}
            disabled={isDisabled || isApplyingText || !overlayText.trim()}
            className="w-full"
          >
            {isApplyingText ? (
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Type className="w-4 h-4 mr-2" />
            )}
            {t("editor.effects.applyTextOverlay", "Apply Text Overlay")}
          </Button>
        </div>

        <Separator />

        {/* Apply All Effects */}
        <div className="bg-black/40 rounded-lg border border-white/5 p-4 space-y-2">
          <div className="flex items-center gap-2">
            <Layers className="w-4 h-4" />
            <h4 className="font-medium">
              {t("editor.effects.chainedEffects", "Chained Effects")}
            </h4>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              "editor.effects.chainedHint",
              "Apply slow motion, color grading, and text overlay in a single pass for better performance.",
            )}
          </p>
          <Button
            size="sm"
            onClick={handleApplyAllEffects}
            disabled={isDisabled || isApplyingAll || !hasAnyEffect}
            className="w-full"
            variant="secondary"
          >
            {isApplyingAll ? (
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Layers className="w-4 h-4 mr-2" />
            )}
            {t("editor.effects.applyAllEffects", "Apply All Effects")}
          </Button>
        </div>
      </div>
    </div>
  );
}
