use super::{AuthError, AuthManager, SubscriptionTier, User};
use crate::auth::middleware::{require_auth, require_tier};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAccess {
    Free,
    AuthRequired,
    ProRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPolicy {
    pub name: &'static str,
    pub access: CommandAccess,
    pub reason: &'static str,
}

pub const COMMAND_POLICIES: &[CommandPolicy] = &[
    CommandPolicy {
        name: "get_app_version",
        access: CommandAccess::Free,
        reason: "Safe local status command.",
    },
    CommandPolicy {
        name: "get_health_status",
        access: CommandAccess::Free,
        reason: "Safe local readiness status command.",
    },
    CommandPolicy {
        name: "get_recording_readiness",
        access: CommandAccess::Free,
        reason: "Safe local readiness status command.",
    },
    CommandPolicy {
        name: "list_games",
        access: CommandAccess::Free,
        reason: "Free local library browsing.",
    },
    CommandPolicy {
        name: "get_game_metadata",
        access: CommandAccess::Free,
        reason: "Free local library browsing.",
    },
    CommandPolicy {
        name: "get_game_events",
        access: CommandAccess::Free,
        reason: "Free local library browsing.",
    },
    CommandPolicy {
        name: "list_clips",
        access: CommandAccess::Free,
        reason: "Free local library browsing.",
    },
    CommandPolicy {
        name: "save_game_metadata",
        access: CommandAccess::Free,
        reason: "위와 같음.",
    },
    CommandPolicy {
        name: "save_game_events",
        access: CommandAccess::Free,
        reason: "위와 같음.",
    },
    CommandPolicy {
        name: "save_clip_metadata",
        access: CommandAccess::Free,
        reason: "녹화가 로그인 없이 되는 이상, 그 결과를 남기는 것도 되어야 한다.",
    },
    CommandPolicy {
        name: "delete_game",
        access: CommandAccess::Free,
        reason: "내 디스크 정리.",
    },
    CommandPolicy {
        name: "show_in_folder",
        access: CommandAccess::Free,
        reason: "내 디스크의 내 파일 위치를 여는 것.",
    },
    CommandPolicy {
        name: "open_file_with_default_app",
        access: CommandAccess::Free,
        reason: "내 디스크의 내 파일을 여는 것.",
    },
    CommandPolicy {
        name: "check_file_exists",
        access: CommandAccess::Free,
        reason: "로컬 파일 존재 확인.",
    },
    CommandPolicy {
        name: "save_replay",
        access: CommandAccess::Free,
        reason: "수동 저장(F8/F9/F10)은 녹화와 같은 층이다.",
    },
    CommandPolicy {
        name: "compose_shorts",
        access: CommandAccess::AuthRequired,
        reason: "Free-account editor export feature.",
    },
    CommandPolicy {
        name: "extract_clip",
        access: CommandAccess::AuthRequired,
        reason: "Free-account editor processing feature.",
    },
    CommandPolicy {
        name: "generate_thumbnail",
        access: CommandAccess::Free,
        reason: "로컬 파일에서 프레임 한 장. 새로 만드는 가치가 아니다.",
    },
    CommandPolicy {
        // FREE users get a metered monthly quota (with watermark); PRO is
        // unlimited. Gating is auth-level here; the quota/tier split is enforced
        // inside the handler.
        name: "start_auto_edit",
        access: CommandAccess::AuthRequired,
        reason: "Authenticated automated editing (metered for FREE, unlimited for PRO).",
    },
    CommandPolicy {
        name: "get_app_update_status",
        access: CommandAccess::Free,
        reason: "Safe local updater status; updates are independent of login.",
    },
    CommandPolicy {
        name: "check_app_update",
        access: CommandAccess::Free,
        reason: "Checks the signed application update channel without login.",
    },
    CommandPolicy {
        name: "install_app_update",
        access: CommandAccess::Free,
        reason: "Installs only artifacts verified by the configured updater key.",
    },
    CommandPolicy {
        name: "plan_auto_edit",
        access: CommandAccess::AuthRequired,
        reason: "Previews auto-edit selection without consuming quota.",
    },
    CommandPolicy {
        name: "cancel_auto_edit",
        access: CommandAccess::AuthRequired,
        reason: "Cancels the authenticated user's active auto-edit.",
    },
    CommandPolicy {
        name: "export_auto_edit_for_platform",
        access: CommandAccess::AuthRequired,
        reason: "Creates and validates a platform delivery artifact.",
    },
    CommandPolicy {
        name: "start_platform_export",
        access: CommandAccess::AuthRequired,
        reason: "Starts a durable platform conversion job.",
    },
    CommandPolicy {
        name: "get_media_job",
        access: CommandAccess::AuthRequired,
        reason: "Reads the authenticated user's durable media job.",
    },
    CommandPolicy {
        name: "list_recoverable_media_jobs",
        access: CommandAccess::AuthRequired,
        reason: "Lists the authenticated user's interrupted media jobs.",
    },
    CommandPolicy {
        name: "pause_media_job",
        access: CommandAccess::AuthRequired,
        reason: "Pauses the authenticated user's active media job.",
    },
    CommandPolicy {
        name: "resume_media_job",
        access: CommandAccess::AuthRequired,
        reason: "Resumes the authenticated user's durable media job.",
    },
    CommandPolicy {
        name: "discard_media_job",
        access: CommandAccess::AuthRequired,
        reason: "Discards the authenticated user's internal job artifacts.",
    },
    CommandPolicy {
        name: "revalidate_auto_edit_result",
        access: CommandAccess::AuthRequired,
        reason: "Revalidates an owned auto-edit output before sharing.",
    },
    CommandPolicy {
        name: "compose_shorts_v2",
        access: CommandAccess::AuthRequired,
        reason: "Free-account editor export feature.",
    },
    CommandPolicy {
        name: "create_longform_video",
        access: CommandAccess::AuthRequired,
        reason: "Free-account multi-clip montage export feature.",
    },
    // ---- Authenticated video/editor utilities ----
    CommandPolicy {
        name: "get_clips",
        access: CommandAccess::Free,
        reason: "내 라이브러리 열람.",
    },
    CommandPolicy {
        name: "generate_clip_thumbnail",
        access: CommandAccess::Free,
        reason: "내 클립의 미리보기. 이게 막히면 목록이 회색 사각형만 남는다.",
    },
    CommandPolicy {
        name: "get_video_duration",
        access: CommandAccess::Free,
        reason: "로컬 파일 정보 읽기.",
    },
    CommandPolicy {
        name: "delete_clip",
        access: CommandAccess::Free,
        reason: "내 디스크 정리. 막으면 용량만 쌓인다.",
    },
    CommandPolicy {
        name: "get_auto_edit_progress",
        access: CommandAccess::AuthRequired,
        reason: "자동편집을 시작한 사람만 볼 진행률. 시작 자체가 인증을 요구하므로 여기도 맞춘다.",
    },
    CommandPolicy {
        name: "save_canvas_template",
        access: CommandAccess::AuthRequired,
        reason: "Free-account editor template feature.",
    },
    CommandPolicy {
        name: "load_canvas_template",
        access: CommandAccess::AuthRequired,
        reason: "Free-account editor template feature.",
    },
    CommandPolicy {
        name: "list_canvas_templates",
        access: CommandAccess::AuthRequired,
        reason: "Free-account editor template feature.",
    },
    CommandPolicy {
        name: "delete_canvas_template",
        access: CommandAccess::AuthRequired,
        reason: "Free-account editor template feature.",
    },
    CommandPolicy {
        name: "get_clip_statistics",
        access: CommandAccess::Free,
        reason: "내 라이브러리 통계 열람.",
    },
    CommandPolicy {
        name: "reset_clip_statistics",
        access: CommandAccess::Free,
        reason: "내 로컬 통계 초기화.",
    },
    CommandPolicy {
        name: "export_video",
        access: CommandAccess::AuthRequired,
        reason: "Exports a local video in a chosen format.",
    },
    CommandPolicy {
        name: "apply_slow_motion_cmd",
        access: CommandAccess::AuthRequired,
        reason: "Applies a local slow-motion effect.",
    },
    CommandPolicy {
        name: "apply_color_grading_cmd",
        access: CommandAccess::AuthRequired,
        reason: "Applies a local color-grading effect.",
    },
    CommandPolicy {
        name: "apply_text_overlay_cmd",
        access: CommandAccess::AuthRequired,
        reason: "Applies a local text overlay.",
    },
    CommandPolicy {
        name: "apply_chained_effects_cmd",
        access: CommandAccess::AuthRequired,
        reason: "Applies chained local effects in one pass.",
    },
    CommandPolicy {
        name: "export_as_gif",
        access: CommandAccess::AuthRequired,
        reason: "Exports a short local GIF.",
    },
    // ---- YouTube (ordinary OAuth/upload is available to a free account) ----
    CommandPolicy {
        name: "youtube_start_auth",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_start_auth_with_server",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_complete_auth",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_get_auth_status",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_upload_video",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_get_upload_progress",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_get_video_details",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_get_upload_history",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_add_to_history",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_get_quota_info",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_logout",
        access: CommandAccess::AuthRequired,
        reason: "Free-account YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_schedule_upload",
        access: CommandAccess::ProRequired,
        reason: "Paid YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_get_upload_queue",
        access: CommandAccess::ProRequired,
        reason: "Paid YouTube upload feature.",
    },
    CommandPolicy {
        name: "youtube_cancel_scheduled_upload",
        access: CommandAccess::ProRequired,
        reason: "Paid YouTube upload feature.",
    },
];

pub fn command_access(command_name: &str) -> Option<CommandAccess> {
    COMMAND_POLICIES
        .iter()
        .find(|policy| policy.name == command_name)
        .map(|policy| policy.access)
}

pub fn require_command_access(
    auth: &Arc<AuthManager>,
    command_name: &str,
) -> Result<Option<User>, AuthError> {
    match command_access(command_name).unwrap_or(CommandAccess::AuthRequired) {
        CommandAccess::Free => Ok(None),
        CommandAccess::AuthRequired => require_auth(auth).map(Some),
        CommandAccess::ProRequired => require_tier(auth, SubscriptionTier::Pro).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user(tier: SubscriptionTier) -> User {
        User {
            id: "test-user".to_string(),
            email: "test@example.com".to_string(),
            tier,
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: 9_999_999_999,
        }
    }

    #[test]
    fn safe_status_commands_remain_free() {
        assert_eq!(
            command_access("get_recording_readiness"),
            Some(CommandAccess::Free)
        );
        let auth = Arc::new(AuthManager::new());
        assert!(require_command_access(&auth, "get_recording_readiness")
            .unwrap()
            .is_none());
    }

    /// 예전에는 `delete_game` 같은 로컬 저장소 변경도 인증을 요구했다.
    ///
    /// 뒤집은 이유: 그 데이터는 **사용자 PC 에 이미 있는 사용자 것**이고, 녹화
    /// 자체가 로그인 없이 되는데 그 결과를 지우지도 못하면 용량만 쌓인다. 인증은
    /// "새 가치를 만드는 순간"에만 요구한다. 로그인 벽을 문 앞에 두면 사람들은
    /// 제품을 보기도 전에 지운다.
    #[test]
    fn housekeeping_my_own_library_no_longer_requires_auth() {
        for command in ["delete_game", "delete_clip", "save_game_metadata"] {
            assert_eq!(
                command_access(command),
                Some(CommandAccess::Free),
                "{command} 은 사용자 자신의 로컬 데이터를 다루는 일이다"
            );
        }
        let auth = Arc::new(AuthManager::new());
        assert!(
            require_command_access(&auth, "delete_game")
                .expect("무료 커맨드는 인증 검사를 통과한다")
                .is_none(),
            "로그아웃 상태에서도 내 게임 기록을 지울 수 있어야 한다"
        );
    }

    #[test]
    fn scheduled_upload_commands_remain_outside_the_free_release_scope() {
        assert_eq!(
            command_access("youtube_schedule_upload"),
            Some(CommandAccess::ProRequired)
        );
        let auth = Arc::new(AuthManager::new());
        auth.login(make_user(SubscriptionTier::Free)).unwrap();

        assert!(matches!(
            require_command_access(&auth, "youtube_schedule_upload"),
            Err(AuthError::Failed(message)) if message.contains("PRO subscription required")
        ));
    }

    #[test]
    fn legacy_pro_users_can_use_scheduled_uploads() {
        let auth = Arc::new(AuthManager::new());
        auth.login(make_user(SubscriptionTier::Pro)).unwrap();

        assert_eq!(
            require_command_access(&auth, "youtube_schedule_upload")
                .unwrap()
                .expect("user returned")
                .tier,
            SubscriptionTier::Pro
        );
    }

    /// Getting your own clip out of the app is free — deliberately, and this test
    /// exists to keep it that way.
    ///
    /// Every comparable tool gives local export away (Medal exports without even a
    /// watermark; Outplayed is ad-supported; the League client itself records
    /// highlights for free), so gating MP4 export put this app below the free
    /// baseline that ships inside the game. The free public release does not expose
    /// a paid upgrade path. If a future change re-gates these, it should have to
    /// delete this test and argue with the comment.
    #[test]
    fn local_export_stays_free_for_signed_in_users() {
        for command in [
            "compose_shorts",
            "compose_shorts_v2",
            "create_longform_video",
            "extract_clip",
            "export_as_gif",
            "save_canvas_template",
            "load_canvas_template",
            "list_canvas_templates",
            "delete_canvas_template",
        ] {
            assert_eq!(
                command_access(command),
                Some(CommandAccess::AuthRequired),
                "{command} must not require PRO — local export is the free baseline"
            );
        }

        let auth = Arc::new(AuthManager::new());
        auth.login(make_user(SubscriptionTier::Free)).unwrap();
        assert!(
            require_command_access(&auth, "compose_shorts_v2").is_ok(),
            "a signed-in FREE user must be able to export their own clip"
        );
    }

    /// 로그인 벽의 위치를 못박는다.
    ///
    /// 정한 것: **인증은 엔진이 실제로 도는 순간에만** 요구한다. 녹화된 것을 보고,
    /// 미리보기를 만들고, 파일을 열고, 지우는 것은 "이미 내 디스크에 있는 내
    /// 것"이므로 로그인 없이 된다.
    ///
    /// 이 줄이 흔들렸던 실제 사례: `generate_clip_thumbnail` 이 `AuthRequired`
    /// 였던 동안, 로그아웃 사용자의 클립 목록은 열리기는 하되 **썸네일이 하나도
    /// 뜨지 않아** 회색 사각형만 남았다. 목록을 무료로 열어 준 의미가 없었다.
    #[test]
    fn browsing_and_managing_my_own_files_needs_no_login() {
        for command in [
            // 보기
            "list_games",
            "list_clips",
            "get_clips",
            "get_game_metadata",
            "get_game_events",
            "get_clip_statistics",
            // 미리보기 — 이게 막히면 목록이 비어 보인다
            "generate_clip_thumbnail",
            "generate_thumbnail",
            "get_video_duration",
            "check_file_exists",
            // 내 디스크 다루기
            "open_file_with_default_app",
            "show_in_folder",
            "delete_clip",
            "delete_game",
            // 녹화가 로그인 없이 되는 이상, 그 결과를 남기는 것도 되어야 한다
            "save_clip_metadata",
            "save_game_metadata",
            "save_game_events",
            "save_replay",
        ] {
            assert_eq!(
                command_access(command),
                Some(CommandAccess::Free),
                "{command} 은 로그인 없이 되어야 한다 — 이미 사용자 디스크에 있는 것을                  보거나 정리하는 일이다"
            );
        }
    }

    /// 반대 방향: 가치를 만들어 내는 순간에는 반드시 인증을 요구한다.
    ///
    /// 무료로 열어 준 목록·미리보기가 여기까지 넘어오면 수익 모델이 사라진다.
    #[test]
    fn the_engine_never_runs_for_a_logged_out_user() {
        for command in [
            "start_auto_edit",
            "compose_shorts",
            "compose_shorts_v2",
            "create_longform_video",
            "export_video",
            "export_as_gif",
            "extract_clip",
            "apply_text_overlay_cmd",
            "apply_color_grading_cmd",
            "apply_slow_motion_cmd",
            "apply_chained_effects_cmd",
            "youtube_upload_video",
        ] {
            assert_ne!(
                command_access(command),
                Some(CommandAccess::Free),
                "{command} 은 실제로 영상을 만들거나 내보내는 일이다 — 로그인 없이 열면 안 된다"
            );
        }
    }

    /// 무료 표면이 조용히 넓어지는 것을 막는다.
    ///
    /// 새 커맨드를 `Free` 로 넣으면 이 테스트가 먼저 깨진다. 그때 "이게 정말
    /// 사용자 디스크에 이미 있는 것을 다루는 일인가"를 다시 묻게 된다.
    #[test]
    fn the_free_surface_is_browsing_previewing_and_housekeeping() {
        let free: Vec<&str> = COMMAND_POLICIES
            .iter()
            .filter(|p| p.access == CommandAccess::Free)
            .map(|p| p.name)
            .collect();

        // 만들거나 내보내는 동사는 무료 표면에 있으면 안 된다.
        for name in &free {
            for verb in ["compose", "export", "auto_edit", "upload", "apply_"] {
                assert!(
                    !name.contains(verb),
                    "무료 커맨드 `{name}` 에 `{verb}` 가 들어 있다 — 가치를 만드는 일이                      무료 표면으로 새어 나왔는지 확인할 것"
                );
            }
        }

        assert!(
            free.len() >= 18,
            "무료 표면이 예상보다 좁다({}개). 로그인 벽이 앞으로 당겨졌는지 확인할 것",
            free.len()
        );
    }

    /// The paid surface, pinned so it cannot drift back onto the export path.
    #[test]
    fn paid_surface_is_templates_and_scheduling_only() {
        let pro: Vec<&str> = COMMAND_POLICIES
            .iter()
            .filter(|p| p.access == CommandAccess::ProRequired)
            .map(|p| p.name)
            .collect();

        for name in &pro {
            assert!(
                name.contains("canvas_template") || name.contains("schedule") || name.contains("queue"),
                "unexpected PRO-gated command `{name}` — the paid surface is canvas                  templates and scheduled/batch upload, not export"
            );
        }
    }
}
