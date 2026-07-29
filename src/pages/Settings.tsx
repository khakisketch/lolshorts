import { useCallback, useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useAuthStore } from "@/lib/auth";
import { settingsApi } from "@/api/settings";
import { authApi, EntitlementInfo } from "@/api/auth";
import { AuthModal } from "@/components/auth";
import { PaymentModal } from "@/components/PaymentModal";
import { SubscriptionManagement } from "@/components/SubscriptionManagement";
import { RecordingSettings } from "@/types";
import { BasicSettings } from "@/components/settings/BasicSettings";
import { EventFilterSettings } from "@/components/settings/EventFilterSettings";
import { GameModeSettings } from "@/components/settings/GameModeSettings";
import { VideoSettings } from "@/components/settings/VideoSettings";
import { AudioSettings } from "@/components/settings/AudioSettings";
import { ClipTimingSettings } from "@/components/settings/ClipTimingSettings";
import { HotkeySettings } from "@/components/settings/HotkeySettings";
import { LanguageSelector } from "@/components/settings/LanguageSelector";
import { GeneralSettings } from "@/components/settings/GeneralSettings";
import { LicensePanel } from "@/components/settings/LicensePanel";
import { AccountInfoPanel } from "@/components/settings/AccountInfoPanel";
import { DiagnosticsSection } from "@/components/settings/DiagnosticsSection";
import { useConfirmDialog } from "@/components/ui/confirm-dialog";
import { ChevronDown, SlidersHorizontal, Save } from "lucide-react";
import { pageStyles } from "@/lib/utils";
import { useToast } from "@/components/ui/use-toast";
import { logger } from "@/lib/logger";

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
  const [showSubscriptionManagement, setShowSubscriptionManagement] = useState(false);
  const [license, setLicense] = useState<EntitlementInfo | null>(null);
  const [isLoadingLicense, setIsLoadingLicense] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);

  const [recordingSettings, setRecordingSettings] = useState<RecordingSettings | null>(null);
  const [isLoadingSettings, setIsLoadingSettings] = useState(false);
  const [isSavingSettings, setIsSavingSettings] = useState(false);

  const loadLicenseInfo = useCallback(async () => {
    setIsLoadingLicense(true);
    try {
      const licenseData = await authApi.getCurrentEntitlement(true);
      setLicense(licenseData);
    } catch (error) {
      toast({ title: t('settings.error.licenseFailed'), variant: 'destructive' });
      logger.error('Failed to load license info:', error);
    } finally {
      setIsLoadingLicense(false);
    }
  }, [t, toast]);

  const loadRecordingSettings = useCallback(async () => {
    setIsLoadingSettings(true);
    try {
      const settings = await settingsApi.getRecordingSettings();
      setRecordingSettings(settings);
    } catch (error) {
      toast({ title: t('settings.error.loadFailed'), variant: 'destructive' });
      logger.error('Failed to load settings:', error);
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
      toast({ title: t('settings.saved') });
    } catch (error) {
      toast({ title: t('settings.error.saveFailed'), variant: 'destructive' });
      logger.error('Failed to save settings:', error);
    } finally {
      setIsSavingSettings(false);
    }
  };

  const resetSettingsToDefault = async () => {
    const confirmed = await confirm({
      title: t('confirmations.resetSettingsTitle'),
      description: t('confirmations.resetSettingsDescription'),
      confirmText: t('settings.recordingConfig.resetToDefaults'),
      variant: 'warning',
    });
    if (!confirmed) return;

    setIsSavingSettings(true);
    try {
      await settingsApi.resetToDefault();
      const defaultSettings = await settingsApi.getRecordingSettings();
      setRecordingSettings(defaultSettings);
    } catch (error) {
      toast({ title: t('settings.error.resetFailed'), variant: 'destructive' });
      logger.error('Failed to reset settings:', error);
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
        <h2 className="text-2xl md:text-3xl font-bold" data-autofocus tabIndex={-1}>
          {t('settings.title')}
        </h2>
        <p className="text-sm text-muted-foreground mt-1" style={{ wordBreak: 'keep-all' }}>
          {t('settings.basic.pageDescription')}
        </p>
      </div>

      <div className="space-y-6">
        {isLoadingSettings ? (
          <div className="gaming-panel p-6 text-center">
            <p className="text-sm text-muted-foreground">
              {t('settings.recordingConfig.loadingSettings')}
            </p>
          </div>
        ) : recordingSettings ? (
          <>
            <BasicSettings
              settings={recordingSettings}
              onChange={saveRecordingSettings}
              disabled={isSavingSettings}
            />

            {isSavingSettings && (
              <div className="flex items-center justify-center gap-2 text-sm text-muted-foreground">
                <Save className="w-4 h-4 animate-pulse" />
                {t('settings.recordingConfig.savingSettings')}
              </div>
            )}

            {/* 고급 설정 — 개별 항목은 사라지지 않고 여기로 내려왔다. */}
            <section data-testid="advanced-settings" className="gaming-panel">
              <button
                type="button"
                onClick={() => setShowAdvanced((open) => !open)}
                aria-expanded={showAdvanced}
                data-testid="advanced-settings-toggle"
                className="flex w-full items-center gap-3 px-6 py-4 text-left min-h-[44px]"
              >
                <SlidersHorizontal className="h-5 w-5 shrink-0 text-gaming-cyan" />
                <span className="flex-1">
                  <span className="block text-base font-semibold">
                    {t('settings.advanced.title')}
                  </span>
                  <span
                    className="block text-sm text-muted-foreground"
                    style={{ wordBreak: 'keep-all' }}
                  >
                    {t('settings.advanced.description')}
                  </span>
                </span>
                <ChevronDown
                  className={`h-5 w-5 shrink-0 text-muted-foreground transition-transform ${
                    showAdvanced ? 'rotate-180' : ''
                  }`}
                />
              </button>

              {showAdvanced && (
                <div className="space-y-6 border-t border-white/5 p-6">
                  <p
                    className="text-xs text-muted-foreground"
                    style={{ wordBreak: 'keep-all' }}
                  >
                    {t('settings.advanced.syncNotice')}
                  </p>

                  <LanguageSelector />

                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <h3 className="text-base font-semibold">
                        {t('settings.recordingConfig.title')}
                      </h3>
                      <p
                        className="text-sm text-muted-foreground mt-1"
                        style={{ wordBreak: 'keep-all' }}
                      >
                        {t('settings.recordingConfig.description')}
                      </p>
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      className="shrink-0 min-h-[44px]"
                      onClick={resetSettingsToDefault}
                      disabled={isSavingSettings}
                    >
                      {t('settings.recordingConfig.resetToDefaults')}
                    </Button>
                  </div>

                  <Tabs defaultValue="general" className="w-full">
                    <TabsList className="grid w-full grid-cols-2 sm:grid-cols-4 lg:grid-cols-7 h-auto gap-1">
                      <TabsTrigger value="general" className="min-h-[44px] text-xs sm:text-sm">{t('settings.recordingConfig.tabs.general')}</TabsTrigger>
                      <TabsTrigger value="events" className="min-h-[44px] text-xs sm:text-sm">{t('settings.recordingConfig.tabs.events')}</TabsTrigger>
                      <TabsTrigger value="modes" className="min-h-[44px] text-xs sm:text-sm">{t('settings.recordingConfig.tabs.modes')}</TabsTrigger>
                      <TabsTrigger value="video" className="min-h-[44px] text-xs sm:text-sm">{t('settings.recordingConfig.tabs.video')}</TabsTrigger>
                      <TabsTrigger value="audio" className="min-h-[44px] text-xs sm:text-sm">{t('settings.recordingConfig.tabs.audio')}</TabsTrigger>
                      <TabsTrigger value="timing" className="min-h-[44px] text-xs sm:text-sm">{t('settings.recordingConfig.tabs.timing')}</TabsTrigger>
                      <TabsTrigger value="hotkeys" className="min-h-[44px] text-xs sm:text-sm">{t('settings.recordingConfig.tabs.hotkeys')}</TabsTrigger>
                    </TabsList>

                    <div className="mt-6">
                      <TabsContent value="general" className="space-y-4">
                        <GeneralSettings
                          settings={{
                            auto_start_with_league: recordingSettings.auto_start_with_league,
                            minimize_to_tray: recordingSettings.minimize_to_tray,
                            show_notifications: recordingSettings.show_notifications,
                            show_replay_popup: recordingSettings.show_replay_popup,
                            crash_reporting_enabled: recordingSettings.crash_reporting_enabled,
                            overlay_enabled: recordingSettings.overlay_enabled,
                            storage: recordingSettings.storage,
                          }}
                          onChange={(updatedGeneral) =>
                            saveRecordingSettings({ ...recordingSettings, ...updatedGeneral })
                          }
                        />
                      </TabsContent>

                      <TabsContent value="events" className="space-y-4">
                        <EventFilterSettings
                          settings={recordingSettings.event_filter}
                          onChange={(eventFilter) =>
                            saveRecordingSettings({ ...recordingSettings, event_filter: eventFilter })
                          }
                        />
                      </TabsContent>

                      <TabsContent value="modes" className="space-y-4">
                        <GameModeSettings
                          settings={recordingSettings.game_mode}
                          onChange={(gameMode) =>
                            saveRecordingSettings({ ...recordingSettings, game_mode: gameMode })
                          }
                        />
                      </TabsContent>

                      <TabsContent value="video" className="space-y-4">
                        <VideoSettings
                          settings={recordingSettings.video}
                          onChange={(video) =>
                            saveRecordingSettings({ ...recordingSettings, video })
                          }
                        />
                      </TabsContent>

                      <TabsContent value="audio" className="space-y-4">
                        <AudioSettings
                          settings={recordingSettings.audio}
                          onChange={(audio) =>
                            saveRecordingSettings({ ...recordingSettings, audio })
                          }
                        />
                      </TabsContent>

                      <TabsContent value="timing" className="space-y-4">
                        <ClipTimingSettings
                          settings={recordingSettings.clip_timing}
                          onChange={(clip_timing) =>
                            saveRecordingSettings({ ...recordingSettings, clip_timing })
                          }
                        />
                      </TabsContent>

                      <TabsContent value="hotkeys" className="space-y-4">
                        <HotkeySettings
                          settings={recordingSettings.hotkeys}
                          onChange={(hotkeys) =>
                            saveRecordingSettings({ ...recordingSettings, hotkeys })
                          }
                        />
                      </TabsContent>
                    </div>
                  </Tabs>

                  {/* 진단도 고급으로 — 평소에는 볼 일이 없고, 필요할 때만 펼친다. */}
                  <DiagnosticsSection />
                </div>
              )}
            </section>
          </>
        ) : (
          <div className="gaming-panel p-6 text-center">
            <p className="text-sm text-muted-foreground">{t('settings.recordingConfig.loadError')}</p>
            <Button onClick={loadRecordingSettings} variant="outline" className="mt-4">
              {t('editor.retry')}
            </Button>
          </div>
        )}

        {/* License & Subscription */}
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

        {/* Account Information */}
        {isAuthenticated && user && (
          <AccountInfoPanel user={user} />
        )}
      </div>

      <ConfirmDialog />

      {showAuthModal && <AuthModal open={showAuthModal} onClose={() => setShowAuthModal(false)} />}

      <PaymentModal isOpen={showPaymentModal} onClose={handlePaymentClose} />

      <SubscriptionManagement
        isOpen={showSubscriptionManagement}
        onClose={handleSubscriptionClose}
        currentTier={(license?.tier || 'FREE') as 'FREE' | 'PRO'}
        expiresAt={license?.expires_at || null}
      />
    </div>
  );
}
