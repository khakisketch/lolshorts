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
import { useConfirmDialog } from "@/components/ui/confirm-dialog";
import { Settings as SettingsIcon, Save } from "lucide-react";
import { pageStyles } from "@/lib/utils";
import { useToast } from "@/components/ui/use-toast";
import { logger } from "@/lib/logger";

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
      <h2 className={pageStyles.title} data-autofocus tabIndex={-1}>{t('settings.title')}</h2>

      <div className="space-y-6">
        {/* Language Selector */}
        <LanguageSelector />

        {/* Recording Configuration */}
        <div className="gaming-panel p-6">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h3 className="text-base font-semibold flex items-center gap-2">
                <SettingsIcon className="w-5 h-5 text-gaming-cyan" />
                {t('settings.recordingConfig.title')}
              </h3>
              <p className="text-sm text-muted-foreground mt-1">
                {t('settings.recordingConfig.description')}
              </p>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={resetSettingsToDefault}
              disabled={isSavingSettings || !recordingSettings}
            >
              {t('settings.recordingConfig.resetToDefaults')}
            </Button>
          </div>

          {isLoadingSettings ? (
            <div className="text-center py-8">
              <p className="text-sm text-muted-foreground">{t('settings.recordingConfig.loadingSettings')}</p>
            </div>
          ) : recordingSettings ? (
            <Tabs defaultValue="general" className="w-full">
              <TabsList className="grid w-full grid-cols-2 sm:grid-cols-4 lg:grid-cols-7 h-auto gap-1">
                <TabsTrigger value="general" className="text-xs sm:text-sm">{t('settings.recordingConfig.tabs.general')}</TabsTrigger>
                <TabsTrigger value="events" className="text-xs sm:text-sm">{t('settings.recordingConfig.tabs.events')}</TabsTrigger>
                <TabsTrigger value="modes" className="text-xs sm:text-sm">{t('settings.recordingConfig.tabs.modes')}</TabsTrigger>
                <TabsTrigger value="video" className="text-xs sm:text-sm">{t('settings.recordingConfig.tabs.video')}</TabsTrigger>
                <TabsTrigger value="audio" className="text-xs sm:text-sm">{t('settings.recordingConfig.tabs.audio')}</TabsTrigger>
                <TabsTrigger value="timing" className="text-xs sm:text-sm">{t('settings.recordingConfig.tabs.timing')}</TabsTrigger>
                <TabsTrigger value="hotkeys" className="text-xs sm:text-sm">{t('settings.recordingConfig.tabs.hotkeys')}</TabsTrigger>
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

              {isSavingSettings && (
                <div className="flex items-center justify-center gap-2 mt-4 text-sm text-muted-foreground">
                  <Save className="w-4 h-4 animate-pulse" />
                  {t('settings.recordingConfig.savingSettings')}
                </div>
              )}
            </Tabs>
          ) : (
            <div className="text-center py-8">
              <p className="text-sm text-muted-foreground">{t('settings.recordingConfig.loadError')}</p>
              <Button onClick={loadRecordingSettings} variant="outline" className="mt-4">
                {t('editor.retry')}
              </Button>
            </div>
          )}
        </div>

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
