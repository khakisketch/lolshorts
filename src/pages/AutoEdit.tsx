import { AutoEditPanel } from "@/components/editor/AutoEditPanel";
import { StudioModeNav } from "@/components/editor/StudioModeNav";
import { ProtectedFeature } from "@/components/auth/ProtectedFeature";
import { useTranslation } from "react-i18next";

export function AutoEdit() {
  const { t } = useTranslation();

  return (
    <ProtectedFeature requiresPro={false} featureName={t("autoEdit.title")}>
      <div className="h-full flex flex-col gap-4 overflow-hidden">
        <StudioModeNav active="auto" />
        <AutoEditPanel />
      </div>
    </ProtectedFeature>
  );
}
