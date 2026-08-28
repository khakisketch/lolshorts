import { ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Film, Play, Settings, Layers, Sparkles } from "lucide-react";

interface EditorLayoutProps {
  clipLibrary: ReactNode;
  videoPreview: ReactNode;
  compositionSettings: ReactNode;
  effectsPanel: ReactNode;
  timeline: ReactNode;
}

export function EditorLayout({
  clipLibrary,
  videoPreview,
  compositionSettings,
  effectsPanel,
  timeline,
}: EditorLayoutProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState("preview");
  const [activeSidebarTab, setActiveSidebarTab] = useState("composition");

  return (
    <>
      {/* Mobile Layout: Tab-based navigation */}
      <div className="flex flex-col h-full overflow-hidden md:hidden">
        <Tabs
          value={activeTab}
          onValueChange={setActiveTab}
          className="flex flex-col h-full"
        >
          {/* Tab Content Area */}
          <div className="flex-1 overflow-hidden">
            <TabsContent
              value="clips"
              className="h-full m-0 data-[state=active]:flex data-[state=active]:flex-col"
            >
              <div className="flex-1 overflow-y-auto bg-card">
                {clipLibrary}
              </div>
            </TabsContent>

            <TabsContent
              value="preview"
              className="h-full m-0 data-[state=active]:flex data-[state=active]:flex-col"
            >
              <div className="flex-1 overflow-hidden">{videoPreview}</div>
            </TabsContent>

            <TabsContent
              value="settings"
              className="h-full m-0 data-[state=active]:flex data-[state=active]:flex-col"
            >
              <div className="flex-1 overflow-y-auto bg-card">
                {compositionSettings}
              </div>
            </TabsContent>

            <TabsContent
              value="effects"
              className="h-full m-0 data-[state=active]:flex data-[state=active]:flex-col"
            >
              <div className="flex-1 overflow-y-auto bg-card">
                {effectsPanel}
              </div>
            </TabsContent>

            <TabsContent
              value="timeline"
              className="h-full m-0 data-[state=active]:flex data-[state=active]:flex-col"
            >
              <div className="flex-1 overflow-hidden bg-card">{timeline}</div>
            </TabsContent>
          </div>

          {/* Tab Navigation - Fixed at bottom */}
          <TabsList className="grid grid-cols-5 w-full h-14 rounded-none border-t bg-background">
            <TabsTrigger
              value="clips"
              className="flex flex-col items-center gap-1 h-full rounded-none data-[state=active]:bg-muted"
            >
              <Film className="h-4 w-4" />
              <span className="text-xs">{t("editor.mobileTabs.clips")}</span>
            </TabsTrigger>
            <TabsTrigger
              value="preview"
              className="flex flex-col items-center gap-1 h-full rounded-none data-[state=active]:bg-muted"
            >
              <Play className="h-4 w-4" />
              <span className="text-xs">{t("editor.mobileTabs.preview")}</span>
            </TabsTrigger>
            <TabsTrigger
              value="settings"
              className="flex flex-col items-center gap-1 h-full rounded-none data-[state=active]:bg-muted"
            >
              <Settings className="h-4 w-4" />
              <span className="text-xs">{t("editor.mobileTabs.settings")}</span>
            </TabsTrigger>
            <TabsTrigger
              value="effects"
              className="flex flex-col items-center gap-1 h-full rounded-none data-[state=active]:bg-muted"
            >
              <Sparkles className="h-4 w-4" />
              <span className="text-xs">{t("editor.mobileTabs.effects")}</span>
            </TabsTrigger>
            <TabsTrigger
              value="timeline"
              className="flex flex-col items-center gap-1 h-full rounded-none data-[state=active]:bg-muted"
            >
              <Layers className="h-4 w-4" />
              <span className="text-xs">{t("editor.mobileTabs.timeline")}</span>
            </TabsTrigger>
          </TabsList>
        </Tabs>
      </div>

      {/* Desktop Layout: Traditional 3-column + bottom timeline */}
      <div className="hidden md:flex flex-col h-full overflow-hidden">
        {/* Top Section: 3 columns */}
        <div className="flex flex-1 overflow-hidden">
          {/* Left: Clip Library */}
          <aside className="w-72 lg:w-80 border-r bg-card overflow-y-auto flex-shrink-0">
            {clipLibrary}
          </aside>

          {/* Center: Video Preview */}
          <main className="flex-1 flex flex-col overflow-hidden min-w-0">
            {videoPreview}
          </main>

          {/* Right: Composition Settings / Effects (tab-switched) */}
          <aside className="w-72 lg:w-80 border-l bg-card overflow-hidden flex-shrink-0 flex flex-col">
            <Tabs
              value={activeSidebarTab}
              onValueChange={setActiveSidebarTab}
              className="flex flex-col h-full"
            >
              <TabsList className="grid grid-cols-2 w-full rounded-none border-b bg-background shrink-0">
                <TabsTrigger value="composition" className="gap-1.5">
                  <Settings className="h-3.5 w-3.5" />
                  {t("editor.sidebarTabs.composition")}
                </TabsTrigger>
                <TabsTrigger value="effects" className="gap-1.5">
                  <Sparkles className="h-3.5 w-3.5" />
                  {t("editor.sidebarTabs.effects")}
                </TabsTrigger>
              </TabsList>
              <TabsContent
                value="composition"
                className="flex-1 overflow-y-auto m-0 data-[state=active]:flex data-[state=active]:flex-col"
              >
                {compositionSettings}
              </TabsContent>
              <TabsContent
                value="effects"
                className="flex-1 overflow-y-auto m-0 data-[state=active]:flex data-[state=active]:flex-col"
              >
                {effectsPanel}
              </TabsContent>
            </Tabs>
          </aside>
        </div>

        {/* Bottom: Timeline */}
        <div className="h-56 lg:h-64 border-t bg-card flex-shrink-0">
          {timeline}
        </div>
      </div>
    </>
  );
}
