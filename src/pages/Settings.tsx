import { useCallback, useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useAuthStore } from "@/lib/auth";
import { settingsApi } from "@/api/settings";
import { authApi, EntitlementInfo } from "@/api/auth";
import { AuthModal } from "@/components/auth";
import { PaymentModal } from "@/components/PaymentModal";
import { SubscriptionManagement } from "@/components/SubscriptionManagement";
import { RecordingSettings } from "@/types";
import { BasicSettings } from "@/components/settings/BasicSettings";
import { AdvancedDisclosure } from "@/components/settings/AdvancedDisclosure";
import { EventFilterSettings } from "@/components/settings/EventFilterSettings";
import { GameModeSettings } from "@/components/settings/GameModeSettings";
import { VideoSettings } from "@/components/settings/VideoSettings";
import { AudioSettings } from "@/components/settings/AudioSettings";
import { ClipTimingSettings } from "@/components/settings/ClipTimingSettings";
import { HotkeySettings } from "@/components/settings/HotkeySettings";
import { LanguageSelector } from "@/components/settings/LanguageSelector";
import { GeneralSettings } from "@/components/settings/GeneralSettings";
import { AppUpdateSettings } from "@/components/settings/AppUpdateSettings";
import { LicensePanel } from "@/components/settings/LicensePanel";
import { AccountInfoPanel } from "@/components/settings/AccountInfoPanel";
import { DiagnosticsSection } from "@/components/settings/DiagnosticsSection";
import { useConfirmDialog } from "@/components/ui/confirm-dialog";
import { RotateCcw, Save } from "lucide-react";
import { pageStyles } from "@/lib/utils";
import { useToast } from "@/components/ui/use-toast";
import { logger } from "@/lib/logger";
import { configureErrorTelemetry } from "@/lib/telemetry";

/** 왼쪽 카테고리. 그룹 이름과 항목 이름은 로케일이 SSOT. */
const SETTINGS_GROUPS = [
  { key: "recording", items: ["video", "highlights", "sound", "hotkeys"] },
  { key: "manage", items: ["storage", "app"] },
  { key: "account", items: ["license", "diagnostics"] },
] as const;

type SettingsSection = (typeof SETTINGS_GROUPS)[number]["items"][number];

/**
 * 설정 — 기본 5가지 + 접힌 고급.
 *
 * 예전에는 73개 항목이 7개 탭에 위계 없이 평평하게 늘어서 있었다. 전부 똑같이
 * 중요해 보이니 사용자는 무엇을 만져야 할지 알 수 없고, 하나가 잘못돼도 앱은
 * 조용히 아무것도 만들지 않는다. 이제 기본 화면은 다섯 질문만 묻고, 개별 항목은
 * 고급 설정 안에 그대로 남는다(삭제가 아니라 강등).
 *
 * 두 화면은 같은 `recordingSettings` 상태를 공유하므로 동기화는 파생으로 성립한다:
 * 기본에서 프리셋을 고르면 고급의 개별 토글이 그 조합으로 바뀌고, 고급에서
 * 토글 하나를 건드리면 기본 화면의 프리셋 표시가 "직접 설정"이 된다.
 */
export function Settings() {
  const { t } = useTranslation();
  const { toast } = useToast();
  const { user, isAuthenticated } = useAuthStore();
  const { confirm, ConfirmDialog } = useConfirmDialog();
  const [showAuthModal, setShowAuthModal] = useState(false);
  const [showPaymentModal, setShowPaymentModal] = useState(false);
  const [showSubscriptionManagement, setShowSubscriptionManagement] =
    useState(false);
  const [license, setLicense] = useState<EntitlementInfo | null>(null);
  const [isLoadingLicense, setIsLoadingLicense] = useState(false);
  // 어느 칸을 보고 있나. 기본은 「영상·화질」 — 새 사용자가 가장 먼저 궁금해하는 것.
  const [section, setSection] = useState<SettingsSection>("video");

  const [recordingSettings, setRecordingSettings] =
    useState<RecordingSettings | null>(null);
  const [isLoadingSettings, setIsLoadingSettings] = useState(false);
  const [isSavingSettings, setIsSavingSettings] = useState(false);

  const loadLicenseInfo = useCallback(async () => {
    setIsLoadingLicense(true);
    try {
      const licenseData = await authApi.getCurrentEntitlement(true);
      setLicense(licenseData);
    } catch (error) {
      toast({
        title: t("settings.error.licenseFailed"),
        variant: "destructive",
      });
      logger.error("Failed to load license info:", error);
    } finally {
      setIsLoadingLicense(false);
    }
  }, [t, toast]);

  const loadRecordingSettings = useCallback(async () => {
    setIsLoadingSettings(true);
    try {
      const settings = await settingsApi.getRecordingSettings();
      try {
        const autostart = await settingsApi.getAutostartStatus();
        setRecordingSettings({
          ...settings,
          launch_on_windows_startup: autostart.configured
            ? autostart.enabled
            : settings.launch_on_windows_startup,
        });
      } catch (error) {
        logger.warn("Failed to synchronize Windows autostart state:", error);
        setRecordingSettings(settings);
      }
    } catch (error) {
      toast({ title: t("settings.error.loadFailed"), variant: "destructive" });
      logger.error("Failed to load settings:", error);
    } finally {
      setIsLoadingSettings(false);
    }
  }, [t, toast]);

  useEffect(() => {
    if (isAuthenticated && user) {
      loadLicenseInfo();
    }
  }, [isAuthenticated, user, loadLicenseInfo]);

  useEffect(() => {
    loadRecordingSettings();
  }, [loadRecordingSettings]);

  const saveRecordingSettings = async (settings: RecordingSettings) => {
    setIsSavingSettings(true);
    try {
      await settingsApi.saveRecordingSettings(settings);
      setRecordingSettings(settings);
      configureErrorTelemetry(settings.crash_reporting_enabled);
      toast({ title: t("settings.saved") });
    } catch (error) {
      toast({ title: t("settings.error.saveFailed"), variant: "destructive" });
      logger.error("Failed to save settings:", error);
    } finally {
      setIsSavingSettings(false);
    }
  };

  const resetSettingsToDefault = async () => {
    const confirmed = await confirm({
      title: t("confirmations.resetSettingsTitle"),
      description: t("confirmations.resetSettingsDescription"),
      confirmText: t("settings.recordingConfig.resetToDefaults"),
      variant: "warning",
    });
    if (!confirmed) return;

    setIsSavingSettings(true);
    try {
      await settingsApi.resetToDefault();
      const defaultSettings = await settingsApi.getRecordingSettings();
      setRecordingSettings(defaultSettings);
      configureErrorTelemetry(defaultSettings.crash_reporting_enabled);
    } catch (error) {
      toast({ title: t("settings.error.resetFailed"), variant: "destructive" });
      logger.error("Failed to reset settings:", error);
    } finally {
      setIsSavingSettings(false);
    }
  };

  const handleUpgradeToPro = () => {
    if (!isAuthenticated) {
      setShowAuthModal(true);
      return;
    }
    setShowPaymentModal(true);
  };

  const handleManageSubscription = () => {
    if (!isAuthenticated) {
      setShowAuthModal(true);
      return;
    }
    setShowSubscriptionManagement(true);
  };

  const handlePaymentClose = () => {
    setShowPaymentModal(false);
    if (isAuthenticated) loadLicenseInfo();
  };

  const handleSubscriptionClose = () => {
    setShowSubscriptionManagement(false);
    if (isAuthenticated) loadLicenseInfo();
  };

  return (
    <div data-testid="settings" className={pageStyles.container}>
      <div>
        <h2
          className="text-2xl md:text-3xl font-bold"
          data-autofocus
          tabIndex={-1}
        >
          {t("settings.title")}
        </h2>
        <p
          className="text-sm text-muted-foreground mt-1"
          style={{ wordBreak: "keep-all" }}
        >
          {t("settings.basic.pageDescription")}
        </p>
      </div>

      <div className="space-y-6">
        {isLoadingSettings ? (
          <div className="gaming-panel p-6 text-center">
            <p className="text-sm text-muted-foreground">
              {t("settings.recordingConfig.loadingSettings")}
            </p>
          </div>
        ) : recordingSettings ? (
          <>
            <div className="flex flex-col gap-4 lg:flex-row lg:items-start">
              {/* 왼쪽: 카테고리. 설정이 한 화면에 다 쌓여 있으면 1280x800 에서
                  3화면분이 되고, 뭘 찾으려면 무조건 스크롤해야 한다. */}
              <nav
                data-testid="settings-nav"
                className="flex shrink-0 gap-1 overflow-x-auto lg:w-52 lg:flex-col lg:overflow-visible"
                aria-label={t("settings.title")}
              >
                {SETTINGS_GROUPS.map((group) => (
                  <div key={group.key} className="contents lg:block">
                    <p className="hidden px-3 pb-1 pt-4 text-xs uppercase tracking-wider text-muted-foreground first:pt-0 lg:block">
                      {t(`settings.nav.groups.${group.key}`)}
                    </p>
                    {group.items.map((item) => (
                      <button
                        key={item}
                        type="button"
                        onClick={() => setSection(item)}
                        aria-current={section === item ? "page" : undefined}
                        data-testid={`settings-nav-${item}`}
                        className={[
                          "min-h-[44px] whitespace-nowrap rounded-md px-3 py-2 text-left text-sm transition-colors lg:block lg:w-full",
                          section === item
                            ? "border-l-2 border-gaming-cyan bg-gaming-cyan/10 text-foreground"
                            : "text-muted-foreground hover:text-foreground",
                        ].join(" ")}
                      >
                        {t(`settings.nav.items.${item}`)}
                      </button>
                    ))}
                  </div>
                ))}
              </nav>

              {/* 오른쪽: 고른 칸만. 스크롤은 여기 안에서만 일어난다. */}
              <div
                className="min-w-0 flex-1 space-y-4"
                data-testid={`settings-section-${section}`}
              >
                {section === "video" && (
                  <>
                    <BasicSettings
                      settings={recordingSettings}
                      onChange={saveRecordingSettings}
                      disabled={isSavingSettings}
                      sections={["quality"]}
                    />
                    <AdvancedDisclosure
                      testId="advanced-video"
                      summary={t("settings.advanced.summary.video")}
                    >
                      <VideoSettings
                        settings={recordingSettings.video}
                        onChange={(video) =>
                          saveRecordingSettings({ ...recordingSettings, video })
                        }
                      />
                    </AdvancedDisclosure>
                  </>
                )}

                {section === "highlights" && (
                  <>
                    <BasicSettings
                      settings={recordingSettings}
                      onChange={saveRecordingSettings}
                      disabled={isSavingSettings}
                      sections={["highlights"]}
                    />
                    <AdvancedDisclosure
                      testId="advanced-highlights"
                      summary={t("settings.advanced.summary.highlights")}
                    >
                      <EventFilterSettings
                        settings={recordingSettings.event_filter}
                        onChange={(eventFilter) =>
                          saveRecordingSettings({
                            ...recordingSettings,
                            event_filter: eventFilter,
                          })
                        }
                      />
                      <ClipTimingSettings
                        settings={recordingSettings.clip_timing}
                        onChange={(clip_timing) =>
                          saveRecordingSettings({
                            ...recordingSettings,
                            clip_timing,
                          })
                        }
                      />
                      <GameModeSettings
                        settings={recordingSettings.game_mode}
                        onChange={(gameMode) =>
                          saveRecordingSettings({
                            ...recordingSettings,
                            game_mode: gameMode,
                          })
                        }
                      />
                    </AdvancedDisclosure>
                  </>
                )}

                {section === "sound" && (
                  <>
                    <BasicSettings
                      settings={recordingSettings}
                      onChange={saveRecordingSettings}
                      disabled={isSavingSettings}
                      sections={["sound"]}
                    />
                    <AdvancedDisclosure
                      testId="advanced-sound"
                      summary={t("settings.advanced.summary.sound")}
                    >
                      <AudioSettings
                        settings={recordingSettings.audio}
                        onChange={(audio) =>
                          saveRecordingSettings({ ...recordingSettings, audio })
                        }
                      />
                    </AdvancedDisclosure>
                  </>
                )}

                {section === "hotkeys" && (
                  <HotkeySettings
                    settings={recordingSettings.hotkeys}
                    onChange={(hotkeys) =>
                      saveRecordingSettings({ ...recordingSettings, hotkeys })
                    }
                  />
                )}

                {section === "storage" && (
                  <BasicSettings
                    settings={recordingSettings}
                    onChange={saveRecordingSettings}
                    disabled={isSavingSettings}
                    sections={["storage"]}
                  />
                )}

                {section === "app" && (
                  <>
                    <BasicSettings
                      settings={recordingSettings}
                      onChange={saveRecordingSettings}
                      disabled={isSavingSettings}
                      sections={["autoStart"]}
                    />
                    <GeneralSettings
                      settings={{
                        minimize_to_tray: recordingSettings.minimize_to_tray,
                        show_notifications:
                          recordingSettings.show_notifications,
                        show_replay_popup: recordingSettings.show_replay_popup,
                        crash_reporting_enabled:
                          recordingSettings.crash_reporting_enabled,
                        overlay_enabled: recordingSettings.overlay_enabled,
                      }}
                      onChange={(updatedGeneral) =>
                        saveRecordingSettings({
                          ...recordingSettings,
                          ...updatedGeneral,
                        })
                      }
                    />
                    <LanguageSelector />
                    <AppUpdateSettings />
                  </>
                )}

                {section === "license" && (
                  <>
                    <LicensePanel
                      isAuthenticated={isAuthenticated}
                      isLoadingLicense={isLoadingLicense}
                      license={license}
                      userEmail={user?.email}
                      onLogin={() => setShowAuthModal(true)}
                      onUpgradeToPro={handleUpgradeToPro}
                      onManageSubscription={handleManageSubscription}
                      onRetry={loadLicenseInfo}
                    />
                    {isAuthenticated && user && (
                      <AccountInfoPanel user={user} />
                    )}
                  </>
                )}

                {section === "diagnostics" && (
                  <>
                    <DiagnosticsSection />
                    {/* 초기화는 고급 설정이 사라지면서 갈 곳이 없어졌다. 되돌리기
                        어려운 동작이라 평소 눈에 띄지 않는 진단 칸이 맞다. */}
                    <section className="gaming-panel p-6">
                      <h3 className="text-base font-semibold">
                        {t("settings.recordingConfig.resetTitle")}
                      </h3>
                      <p
                        className="mt-1 text-sm text-muted-foreground"
                        style={{ wordBreak: "keep-all" }}
                      >
                        {t("settings.recordingConfig.resetDescription")}
                      </p>
                      <Button
                        variant="outline"
                        className="mt-4"
                        onClick={resetSettingsToDefault}
                        disabled={isSavingSettings}
                        data-testid="settings-reset"
                      >
                        <RotateCcw
                          className="mr-2 h-4 w-4"
                          aria-hidden="true"
                        />
                        {t("settings.recordingConfig.resetAction")}
                      </Button>
                    </section>
                  </>
                )}

                {isSavingSettings && (
                  <div className="flex items-center justify-center gap-2 text-sm text-muted-foreground">
                    <Save className="w-4 h-4 animate-pulse" />
                    {t("settings.recordingConfig.savingSettings")}
                  </div>
                )}
              </div>
            </div>
          </>
        ) : (
          <div className="gaming-panel p-6 text-center">
            <p className="text-sm text-muted-foreground">
              {t("settings.recordingConfig.loadError")}
            </p>
            <Button
              onClick={loadRecordingSettings}
              variant="outline"
              className="mt-4"
            >
              {t("editor.retry")}
            </Button>
          </div>
        )}
      </div>

      <ConfirmDialog />

      {showAuthModal && (
        <AuthModal
          open={showAuthModal}
          onClose={() => setShowAuthModal(false)}
        />
      )}

      <PaymentModal isOpen={showPaymentModal} onClose={handlePaymentClose} />

      <SubscriptionManagement
        isOpen={showSubscriptionManagement}
        onClose={handleSubscriptionClose}
        currentTier={(license?.tier || "FREE") as "FREE" | "PRO"}
        expiresAt={license?.expires_at || null}
      />
    </div>
  );
}
