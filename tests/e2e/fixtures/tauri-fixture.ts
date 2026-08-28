import { test as base, expect, Page } from "@playwright/test";

/**
 * Shared Playwright fixture for LoLShorts E2E tests.
 *
 * Provides:
 * - Tauri API mocking via addInitScript (runs before app JS loads)
 * - Onboarding skip
 * - Dynamic auth state switching via window.__TEST_AUTH_STATE__
 * - Auth helper functions (loginAsFreeUser, loginAsProUser)
 * - Navigation helpers
 */

// Declare global test auth state type
declare global {
  interface Window {
    __TEST_AUTH_STATE__: { authenticated: boolean; tier: string | null };
    __TEST_UPDATE_STATE__: {
      status: string;
      current_version: string;
      available_version: string | null;
      notes: string | null;
      published_at: string | null;
      progress_percentage: number;
      error_code: string | null;
    };
    __TAURI__: unknown;
    __TAURI_INTERNALS__: unknown;
    __TAURI_EVENT_PLUGIN_INTERNALS__: {
      unregisterListener: (event: string, eventId: number) => void;
    };
  }
}

export const test = base.extend({
  page: async ({ page }, run) => {
    // Set up Tauri mocks BEFORE any navigation (runs before app JS loads)
    await page.addInitScript(() => {
      // Skip onboarding modal
      localStorage.setItem(
        "lolshorts_onboarding_completed",
        JSON.stringify({
          version: 2,
          completedAt: "2026-08-20T00:00:00.000Z",
          completion: "passed",
        }),
      );

      // Set default language to English for consistent test results
      // (prevents auto-detection of system language, e.g., Korean on Korean Windows)
      // Only set if not already set, so language persistence tests work across reloads
      if (!localStorage.getItem("i18nextLng")) {
        localStorage.setItem("i18nextLng", "en");
      }

      // Default auth state (unauthenticated)
      (window as Window).__TEST_AUTH_STATE__ = {
        authenticated: false,
        tier: null,
      };
      (window as Window).__TEST_UPDATE_STATE__ = {
        status: "disabled",
        current_version: "1.2.0",
        available_version: null,
        notes: null,
        published_at: null,
        progress_percentage: 0,
        error_code: "updater_disabled",
      };

      // Create a simple mock function that tracks calls
      const createMockFn = () => {
        const calls: unknown[][] = [];
        const fn = (...args: unknown[]) => {
          calls.push(args);
          return Promise.resolve();
        };
        fn.calls = calls;
        return fn;
      };

      // Comprehensive Tauri invoke handler
      const mockInvoke = async (
        cmd: string,
        args?: Record<string, unknown>,
      ): Promise<unknown> => {
        switch (cmd) {
          case "get_auth_status":
            // Dynamic auth state - reads at call time
            return (window as Window).__TEST_AUTH_STATE__;

          case "get_unified_game_status":
            return {
              lcu_connected: false,
              in_game: false,
              game_mode: "Live",
              summoner_name: null,
              champion_name: null,
              game_time: null,
              is_monitoring: false,
              is_recording: false,
            };

          case "get_dashboard_stats":
            return {
              total_games: 0,
              total_clips: 0,
              total_size_bytes: 0,
            };

          case "get_full_settings":
          case "get_settings":
          case "get_recording_settings":
            return {
              schema_version: 4,
              // 백엔드 기본값(1080p60 / medium / h264)과 같은 조합 = 기본 화면의 "보통".
              video: {
                resolution: "r1920x1080",
                frame_rate: "fps60",
                bitrate_preset: "medium",
                codec: "h264",
                encoder: "auto",
              },
              audio: {
                record_microphone: false,
                record_system_audio: true,
                sample_rate: "hz48000",
                bitrate: "kbps192",
                system_audio_device: null,
                microphone_device: null,
                microphone_volume: 100,
                system_audio_volume: 100,
              },
              // 백엔드가 실제로 돌려주는 전체 필드 = HighlightPreset::Balanced 조합.
              // 일부만 담으면 설정 화면이 "직접 설정"으로 보여 갓 설치한 앱 상태를
              // 재현하지 못한다(src-tauri/src/settings/models.rs 참조).
              event_filter: {
                record_kills: true,
                record_multikills: true,
                record_first_blood: true,
                // 셧다운은 킬 계열이라 기본 on. 이 줄이 `false` 이던 동안 mock 은
                // 어떤 프리셋과도 맞지 않아, e2e 가 보는 설정 화면은 첫 화면부터
                // "직접 설정" 배지를 달고 카드가 하나도 안 골라진 상태였다.
                record_shutdown: true,
                record_deaths: false,
                record_first_blood_victim: false,
                record_assists: true,
                record_dragon: true,
                record_baron: true,
                record_elder: true,
                record_herald: true,
                record_turret: false,
                record_inhibitor: false,
                record_nexus: true,
                record_ace: true,
                record_game_end: true,
                record_steal: true,
                record_voidgrubs: true,
                record_atakhan: true,
                record_outplay: true,
                record_trade_kill: true,
                record_low_hp: true,
                min_priority: 1,
                min_game_duration_secs: 300,
                contest_window_secs: 10,
              },
              game_mode: {
                record_ranked_solo: true,
                record_ranked_flex: true,
                record_normal: true,
                record_quick_play: true,
                record_aram: true,
                record_arena: true,
                record_special: true,
                record_custom: false,
                record_practice: false,
              },
              clip_timing: {
                default_pre_duration: 15,
                default_post_duration: 5,
                event_timings: {},
                merge_consecutive_events: true,
                merge_time_threshold: 15,
              },
              hotkeys: {
                toggle_recording: "F8",
                manual_save_clip: "F9",
                delete_last_clip: "F10",
              },
              storage: {
                auto_delete_enabled: false,
                auto_delete_days: 30,
                max_storage_gb: 50,
                delete_exported_clips: false,
              },
              launch_on_windows_startup: false,
              minimize_to_tray: true,
              show_notifications: true,
              show_replay_popup: true,
              crash_reporting_enabled: false,
              overlay_enabled: true,
            };

          case "list_audio_devices":
            return [
              {
                id: "default",
                name: "Default Audio Device",
                device_type: "SystemAudio",
              },
              {
                id: "microphone",
                name: "Microphone",
                device_type: "Microphone",
              },
            ];

          // cpal 기반 장치 열거 (AudioSettings 드롭다운)
          case "list_system_audio_devices":
            return ["Default Audio Device", "Speakers (Realtek)"];

          case "list_microphone_devices":
            return ["Default Microphone", "Headset Mic (USB)"];

          case "get_audio_devices_with_cache_info":
            return {
              devices: [
                {
                  id: "default",
                  name: "Default Audio Device",
                  device_type: "SystemAudio",
                },
                {
                  id: "microphone",
                  name: "Microphone",
                  device_type: "Microphone",
                },
              ],
              cache_age_seconds: 0,
              cache_ttl_seconds: 60,
              cache_valid: true,
              total_devices: 2,
            };

          case "get_recording_status":
            return "idle";

          case "get_detailed_recording_status":
            return {
              status: "idle",
              is_monitoring: false,
              buffer_duration_secs: 120,
            };

          case "get_recording_readiness":
            return {
              ready: false,
              blockers: [
                {
                  code: "ffmpeg_missing",
                  component: "FFmpeg",
                  message: "FFmpeg unavailable in test fixture",
                  action: "Install bundled FFmpeg",
                },
              ],
              warnings: [
                {
                  code: "storage_low",
                  component: "storage",
                  message: "Storage space low in test fixture",
                  action: "Free disk space",
                },
              ],
              components: [
                {
                  component: "FFmpeg",
                  status: "error",
                  message: "Missing binary",
                },
                {
                  component: "system-audio",
                  status: "unknown",
                  message: "Virtual audio device",
                },
                {
                  component: "storage",
                  status: "warning",
                  message: "Low space",
                },
                {
                  component: "League Client Update",
                  status: "error",
                  message: "LCU unavailable",
                },
                {
                  component: "hardware encoder",
                  status: "ok",
                  message: "GPU encoder ready",
                },
              ],
            };

          case "get_replay_target_candidates":
            return {
              status: "ready",
              candidates: [
                { summoner_name: "Faker", champion_id: 103, team_id: 100 },
                { summoner_name: "Chovy", champion_id: 7, team_id: 200 },
              ],
              selected_target: "Faker",
              error: null,
              retryable: false,
            };

          case "get_saved_clips":
            return [];

          case "get_system_info":
            return {
              platform: "win32",
              arch: "x64",
              version: "1.2.0",
            };

          case "connect_lcu":
          case "start_recording":
          case "stop_recording":
          case "start_auto_capture":
          case "stop_auto_capture":
          case "refresh_audio_devices":
          case "save_recording_settings":
          case "reset_settings_to_default":
          case "save_replay":
          case "notify_replay_launched":
          case "login":
          case "signup":
            return {
              id: "test-user-id-12345",
              email: "test@lolshorts.com",
              tier: "Free",
              expires_at: 9999999999,
            };
          case "set_session": {
            const authState = (window as Window).__TEST_AUTH_STATE__;
            const tier = authState.tier === "PRO" ? "PRO" : "FREE";
            return {
              user: {
                id: "test-user-id-12345",
                email: "test@lolshorts.com",
                tier: tier === "PRO" ? "Pro" : "Free",
                expires_at: 9999999999,
              },
              entitlement: {
                tier,
                status: "active",
                expires_at: null,
                source: "supabase",
                checked_at: new Date().toISOString(),
                payment_available: false,
              },
            };
          }
          case "get_current_entitlement": {
            const authState = (window as Window).__TEST_AUTH_STATE__;
            const tier = authState.tier === "PRO" ? "PRO" : "FREE";
            return {
              tier,
              status: authState.authenticated ? "active" : "none",
              expires_at: null,
              source: "supabase",
              checked_at: new Date().toISOString(),
              payment_available: false,
            };
          }
          case "logout":
            return undefined;

          case "check_lcu_status":
            return false;

          case "list_match_history":
            return [];

          case "get_recording_metrics":
          case "get_system_metrics":
            return null;

          case "get_health_status":
            return "Healthy";

          case "get_autostart_status":
            return { configured: true, enabled: false, error_code: null };

          case "set_launch_on_windows_startup":
            return {
              configured: true,
              enabled: Boolean(args?.enabled),
              error_code: null,
            };

          case "select_and_stage_external_media":
            return {
              path: `C:\\Users\\Tester\\AppData\\Roaming\\lolshorts\\staging\\imports\\selected.${args?.kind === "image" ? "png" : args?.kind === "audio" ? "mp3" : "mp4"}`,
              size_bytes: 1024,
              reused_app_owned_file: false,
              original_file_name: `selected.${args?.kind === "image" ? "png" : args?.kind === "audio" ? "mp3" : "mp4"}`,
            };

          case "get_app_update_status":
            return (window as Window).__TEST_UPDATE_STATE__;

          case "check_app_update": {
            const current = (window as Window).__TEST_UPDATE_STATE__;
            const next = current.available_version
              ? { ...current, status: "available", error_code: null }
              : {
                  status: "up_to_date",
                  current_version: "1.2.0",
                  available_version: null,
                  notes: null,
                  published_at: null,
                  progress_percentage: 100,
                  error_code: null,
                };
            (window as Window).__TEST_UPDATE_STATE__ = next;
            return next;
          }

          case "install_app_update": {
            const next = {
              ...(window as Window).__TEST_UPDATE_STATE__,
              status: "installing",
              progress_percentage: 100,
              error_code: null,
            };
            (window as Window).__TEST_UPDATE_STATE__ = next;
            return next;
          }

          case "get_diagnostics_status":
            return {
              overall_status: "warning",
              checks: [
                {
                  key: "required_env",
                  label: "Required environment",
                  status: "ok",
                  message: "Required runtime configuration is present.",
                  action: "No action required.",
                },
                {
                  key: "updater_pubkey",
                  label: "Updater public key",
                  status: "warning",
                  message:
                    "TAURI_UPDATER_PUBKEY is not configured in the test fixture.",
                  action:
                    "Provide TAURI_UPDATER_PUBKEY in CI/release build configuration.",
                },
              ],
            };

          case "export_diagnostics_bundle":
            return {
              output_path: "C:\\fixture\\diagnostics\\diagnostics_fixture.json",
              redacted: true,
              generated_at: new Date().toISOString(),
              included_logs: 1,
            };

          case "get_all_games":
          case "list_games":
            return [];

          case "get_game_metadata":
          case "get_game_events":
            return null;

          case "get_storage_stats":
            return {
              total_games: 0,
              total_clips: 0,
              total_size_bytes: 0,
            };

          case "list_clips":
            return [];

          case "list_clip_vault_page":
            return {
              groups: Array.from({ length: 6 }, (_, gameIndex) => ({
                game_id: `fixture-game-${gameIndex + 1}`,
                game: {
                  game_id: `fixture-game-${gameIndex + 1}`,
                  champion: [
                    "Ahri",
                    "Braum",
                    "Jinx",
                    "Lee Sin",
                    "Lux",
                    "Yasuo",
                  ][gameIndex],
                  game_mode: "CLASSIC",
                  start_time: new Date(
                    Date.UTC(2026, 7, 9 - gameIndex, 12, 0, 0),
                  ).toISOString(),
                  end_time: null,
                  result: gameIndex % 2 === 0 ? "Win" : "Loss",
                  kda: null,
                },
                clips: Array.from({ length: 3 }, (_, clipIndex) => ({
                  file_path: `C:\\fixture\\clips\\game-${gameIndex + 1}-${clipIndex + 1}.mp4`,
                  thumbnail_path: null,
                  event_type:
                    clipIndex === 0 ? { multikill: 3 } : "champion_kill",
                  event_time: 100 + clipIndex * 20,
                  priority: 3 - clipIndex,
                  duration: 25,
                  event_offset_secs: 10,
                  highlight_score: 80 - gameIndex * 3 - clipIndex,
                  score_reasons: [],
                  created_at: new Date(
                    Date.UTC(2026, 7, 9 - gameIndex, 12, clipIndex, 0),
                  ).toISOString(),
                  usage_count: 0,
                })),
                clip_count: 3,
              })),
              next_cursor: null,
              skipped_item_count: 0,
            };

          case "ensure_clip_thumbnail":
            return "C:\\fixture\\clips\\thumbnail.jpg";

          case "save_canvas_template":
          case "delete_canvas_template":
            return undefined;

          case "list_canvas_templates":
            return [];

          case "get_auto_edit_quota":
            return {
              tier: "FREE",
              is_pro: false,
              usage: 0,
              limit: 5,
              remaining: 5,
              month: "2026-04",
            };

          case "youtube_get_auth_status":
            return {
              authenticated: false,
              expires_at: null,
              has_refresh_token: false,
            };

          case "youtube_get_upload_history":
          case "youtube_get_upload_queue":
            return [];

          case "youtube_get_quota_info":
            return {
              daily_limit: 10000,
              used: 0,
              remaining: 10000,
              reset_at: 1767225600,
            };

          case "youtube_get_upload_progress":
            return null;

          case "youtube_start_auth":
          case "youtube_start_auth_with_server":
            return "https://youtube.example.test/oauth";

          case "youtube_upload_video":
            return {
              id: "fixture-video-id",
              title: "Fixture Upload",
              description: "Fixture upload response",
              thumbnail_url: null,
              published_at: "2026-04-25T00:00:00Z",
              privacy_status: "private",
              view_count: null,
            };

          case "youtube_schedule_upload":
            return {
              id: "fixture-scheduled-upload",
              video_path: "C:\\fixture\\clip.mp4",
              title: "Fixture Scheduled Upload",
              description: "Fixture scheduled upload response",
              tags: ["fixture"],
              privacy_status: "private",
              thumbnail_path: null,
              schedule: { scheduled_at: null, queue_position: null },
              created_at: 1777075200,
              status: null,
              error_message: null,
            };

          case "youtube_complete_auth":
          case "youtube_logout":
          case "youtube_add_to_history":
          case "youtube_cancel_scheduled_upload":
            return undefined;

          default:
            console.warn(`[Tauri Mock] Unmocked command: ${cmd}`);
            return null;
        }
      };

      // Set up __TAURI__ (backward compatibility) and __TAURI_INTERNALS__
      const tauriMock = {
        invoke: mockInvoke,
        event: {
          listen: createMockFn(),
          emit: createMockFn(),
        },
      };

      (window as Window).__TAURI__ = tauriMock;
      (window as Window).__TAURI_INTERNALS__ = {
        invoke: mockInvoke,
        transformCallback: () => 0,
        convertFileSrc: (path: string) => path,
        metadata: {},
      };
      (window as Window).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: () => {},
      };

      // Mock Tauri plugin modules by intercepting module resolution
      // This prevents crashes from module-level imports like:
      //   import { getCurrentWindow } from '@tauri-apps/api/window'
      //   import { open } from '@tauri-apps/plugin-dialog'
    });

    // Intercept Supabase auth API calls to prevent them from overriding
    // the mocked auth state. The app's checkAuth() calls supabase.auth.getSession()
    // which would fail (no real Supabase server) and reset the persisted state.
    await page.route("**/auth/v1/**", async (route) => {
      const url = route.request().url();

      if (url.includes("/token") || url.includes("/session")) {
        // Return empty session - the Zustand persisted state will be used instead
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            access_token: "mock-access-token",
            token_type: "bearer",
            expires_in: 3600,
            refresh_token: "mock-refresh-token",
            user: {
              id: "test-user-id-12345",
              email: "test@lolshorts.com",
              aud: "authenticated",
              role: "authenticated",
              app_metadata: {},
              user_metadata: {},
              created_at: "2024-01-01T00:00:00Z",
            },
          }),
        });
      } else {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({}),
        });
      }
    });

    // Intercept Supabase REST API calls (for profile queries etc)
    await page.route("**/rest/v1/**", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify([]),
      });
    });

    await run(page);
  },
});

export { expect };

/**
 * Create a mock user object matching the app's User type.
 */
function createMockUser(tier: "FREE" | "PRO") {
  return {
    id: "test-user-id-12345",
    email: "test@lolshorts.com",
    tier,
    profile: {
      id: "test-user-id-12345",
      email: "test@lolshorts.com",
      display_name: "Test User",
      avatar_url: null,
    },
    supabaseUser: {
      id: "test-user-id-12345",
      email: "test@lolshorts.com",
      aud: "authenticated",
      role: "authenticated",
      app_metadata: {},
      user_metadata: {},
      created_at: "2024-01-01T00:00:00Z",
    },
  };
}

async function setAuthState(page: Page, tier: "FREE" | "PRO"): Promise<void> {
  const user = createMockUser(tier);
  const authState = { authenticated: true, tier };

  if (page.url() === "about:blank") {
    await page.addInitScript(
      ({ state, authTier, authUser }) => {
        (window as Window).__TEST_AUTH_STATE__ = state;

        const zustandState = {
          state: {
            user: authUser,
            entitlement: {
              tier: authTier,
              status: "active",
              expires_at: null,
              source: "supabase",
              checked_at: new Date().toISOString(),
              payment_available: false,
            },
            isAuthenticated: true,
          },
          version: 0,
        };
        localStorage.setItem("lolshorts-auth", JSON.stringify(zustandState));

        const supabaseSession = {
          access_token: "mock-access-token-" + Date.now(),
          token_type: "bearer",
          expires_in: 3600,
          expires_at: Math.floor(Date.now() / 1000) + 3600,
          refresh_token: "mock-refresh-token",
          user: {
            id: authUser.id,
            aud: "authenticated",
            role: "authenticated",
            email: authUser.email,
            email_confirmed_at: "2024-01-01T00:00:00Z",
            app_metadata: { provider: "email", providers: ["email"] },
            user_metadata: { tier: authTier },
            created_at: "2024-01-01T00:00:00Z",
            updated_at: "2024-01-01T00:00:00Z",
          },
        };
        localStorage.setItem(
          "sb-localhost-auth-token",
          JSON.stringify(supabaseSession),
        );
      },
      { state: authState, authTier: tier, authUser: user },
    );
    return;
  }

  await page.evaluate(
    ({ state, authTier, authUser }) => {
      (window as Window).__TEST_AUTH_STATE__ = state;

      const zustandState = {
        state: {
          user: authUser,
          entitlement: {
            tier: authTier,
            status: "active",
            expires_at: null,
            source: "supabase",
            checked_at: new Date().toISOString(),
            payment_available: false,
          },
          isAuthenticated: true,
        },
        version: 0,
      };
      localStorage.setItem("lolshorts-auth", JSON.stringify(zustandState));

      const supabaseSession = {
        access_token: "mock-access-token-" + Date.now(),
        token_type: "bearer",
        expires_in: 3600,
        expires_at: Math.floor(Date.now() / 1000) + 3600,
        refresh_token: "mock-refresh-token",
        user: {
          id: authUser.id,
          aud: "authenticated",
          role: "authenticated",
          email: authUser.email,
          email_confirmed_at: "2024-01-01T00:00:00Z",
          app_metadata: { provider: "email", providers: ["email"] },
          user_metadata: { tier: authTier },
          created_at: "2024-01-01T00:00:00Z",
          updated_at: "2024-01-01T00:00:00Z",
        },
      };
      localStorage.setItem(
        "sb-localhost-auth-token",
        JSON.stringify(supabaseSession),
      );
    },
    { state: authState, authTier: tier, authUser: user },
  );
}

/**
 * Set auth state to authenticated FREE user.
 * Sets both the window test state and Zustand persist localStorage.
 * Seeds auth before the next navigation when page is on about:blank.
 */
export async function loginAsFreeUser(page: Page): Promise<void> {
  await setAuthState(page, "FREE");
}

/**
 * Set auth state to authenticated PRO user.
 * Sets both the window test state and Zustand persist localStorage.
 * Seeds auth before the next navigation when page is on about:blank.
 */
export async function loginAsProUser(page: Page): Promise<void> {
  await setAuthState(page, "PRO");
}

/**
 * Set auth state to unauthenticated.
 */
export async function logout(page: Page): Promise<void> {
  await page.evaluate(() => {
    (window as Window).__TEST_AUTH_STATE__ = {
      authenticated: false,
      tier: null,
    };
    localStorage.removeItem("lolshorts-auth");
  });
}

/**
 * Navigate to a route using nav links (preferred over direct goto).
 * Falls back to page.goto if no nav link found.
 */
export async function navigateTo(page: Page, testId: string): Promise<void> {
  const navLink = page.locator(`[data-testid="${testId}"]`);
  if (await navLink.isVisible({ timeout: 2000 }).catch(() => false)) {
    await navLink.click();
  }
  await page.waitForLoadState("networkidle");
}

/** Base URL for tests */
export const BASE_URL = "http://127.0.0.1:5181";
