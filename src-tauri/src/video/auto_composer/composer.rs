#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::super::thumbnail::auto_generate_thumbnail;
use super::super::{ClipInfo, Result, VideoError, VideoProcessor};
use super::caption::clip_caption;
use super::types::{
    AutoEditConfig, AutoEditOutput, AutoEditOutputIntent, AutoEditPlan, AutoEditPlanClip,
    AutoEditProgress, AutoEditResult, AutoEditStatus, StoryboardClip,
};
use crate::storage::Storage;
use crate::video::processor::types::{CaptionSpec, ClipSpec};

/// YouTube Shorts 최대 길이(초). YouTube Shorts는 최대 3분(180초)까지 지원한다.
pub(super) const MAX_TARGET_DURATION_SECS: u32 = 180;

/// YouTube Shorts 생성을 위한 자동 편집기 (Auto-Composer)
pub struct AutoComposer {
    pub(super) video_processor: Arc<VideoProcessor>,
    pub(super) storage: Arc<Storage>,
    pub(super) progress: Arc<RwLock<Option<AutoEditProgress>>>,
    active_job: Arc<Mutex<Option<ActiveAutoEditJob>>>,
    /// 최종 산출물(+썸네일)을 보존할 앱 관리 루트. None이면 %TEMP% 하위에 남긴다(하위호환).
    pub(super) output_root: Option<PathBuf>,
    /// Some(target_lufs)면 최종 단계에서 loudnorm 2-pass 정규화를 수행한다.
    pub(super) normalize_lufs: Option<f64>,
}

#[derive(Clone)]
struct ActiveAutoEditJob {
    job_id: String,
    cancellation: CancellationToken,
}

impl AutoComposer {
    /// 새로운 AutoComposer 인스턴스 생성
    pub fn new(video_processor: Arc<VideoProcessor>, storage: Arc<Storage>) -> Self {
        Self {
            video_processor,
            storage,
            progress: Arc::new(RwLock::new(None)),
            active_job: Arc::new(Mutex::new(None)),
            output_root: None,
            normalize_lufs: None,
        }
    }

    /// 산출물 루트 주입 (builder-style setter).
    ///
    /// 설정 시 모든 단계 산출물은 이 루트 하위에서 생성되고, 완료 후 중간 산출물은
    /// 삭제되며 최종본 + 썸네일만 루트에 보존된다. 미설정이면 %TEMP% 동작을 유지한다.
    pub fn set_output_root(&mut self, root: PathBuf) {
        self.output_root = Some(root);
    }

    /// 최종 오디오 라우드니스 정규화 설정 (builder-style setter). None이면 끈다.
    pub fn set_normalize_audio(&mut self, target_lufs: Option<f64>) {
        self.normalize_lufs = target_lufs;
    }

    /// 단계별(중간) 산출물을 놓을 디렉토리.
    pub(super) fn stage_dir(&self) -> PathBuf {
        let base = match &self.output_root {
            Some(root) => root.join("intermediate"),
            None => std::env::temp_dir().join("lolshorts_auto_edit"),
        };
        match crate::video::current_auto_edit_job_id() {
            Some(job_id) => base.join(job_id),
            None => base,
        }
    }

    pub fn job_stage_dir(&self, job_id: &str) -> PathBuf {
        match &self.output_root {
            Some(root) => root.join("intermediate").join(job_id),
            None => std::env::temp_dir()
                .join("lolshorts_auto_edit")
                .join(job_id),
        }
    }

    pub async fn begin_job(&self, job_id: &str) -> Result<CancellationToken> {
        let mut guard = self.active_job.lock().await;
        if let Some(active) = guard.as_ref() {
            return Err(VideoError::ProcessingError {
                message: format!("Auto-edit busy; active job is {}", active.job_id),
            });
        }
        let cancellation = CancellationToken::new();
        *guard = Some(ActiveAutoEditJob {
            job_id: job_id.to_string(),
            cancellation: cancellation.clone(),
        });
        drop(guard);
        self.update_progress(job_id, AutoEditStatus::Queued, 0.0, "Queued".to_string())
            .await;
        Ok(cancellation)
    }

    pub async fn finish_job(&self, job_id: &str) {
        let mut guard = self.active_job.lock().await;
        if guard.as_ref().map(|job| job.job_id.as_str()) == Some(job_id) {
            *guard = None;
        }
    }

    /// Returns whether an AutoEdit or platform-export job currently owns the
    /// single media execution slot. The updater uses this backend truth to
    /// prevent an installer from terminating FFmpeg midway through a job.
    pub async fn is_busy(&self) -> bool {
        self.active_job.lock().await.is_some()
    }

    pub async fn cancel_job(&self, job_id: &str) -> Result<AutoEditProgress> {
        let guard = self.active_job.lock().await;
        let active = guard.as_ref().ok_or_else(|| VideoError::ProcessingError {
            message: format!("Auto-edit job {} is not active", job_id),
        })?;
        if active.job_id != job_id {
            return Err(VideoError::ProcessingError {
                message: format!("Auto-edit job {} is not active", job_id),
            });
        }
        active.cancellation.cancel();
        drop(guard);
        let mut progress = self.progress.write().await;
        let snapshot = progress.get_or_insert_with(|| AutoEditProgress {
            job_id: job_id.to_string(),
            status: AutoEditStatus::Cancelled,
            progress: 0.0,
            current_step: "Cancelled".to_string(),
            elapsed_seconds: 0.0,
            estimated_seconds: 0.0,
            output_path: None,
            error: None,
            outputs: Vec::new(),
        });
        snapshot.status = AutoEditStatus::Cancelled;
        snapshot.current_step = "Cancelled".to_string();
        snapshot.error = None;
        Ok(snapshot.clone())
    }

    pub async fn cancel_active_job(&self) {
        let active = self.active_job.lock().await.clone();
        if let Some(active) = active {
            active.cancellation.cancel();
        }
    }

    pub async fn cleanup_job_artifacts(&self, job_id: &str) {
        let dir = self.job_stage_dir(job_id);
        if dir.exists() {
            if let Err(error) = tokio::fs::remove_dir_all(&dir).await {
                warn!(
                    "Failed to clean auto-edit job directory {:?}: {}",
                    dir, error
                );
            }
        }
    }

    /// 최종본 + 썸네일을 놓을 디렉토리.
    pub(crate) fn final_dir(&self) -> PathBuf {
        match &self.output_root {
            Some(root) => root.clone(),
            None => std::env::temp_dir().join("lolshorts_auto_edit"),
        }
    }

    /// 메인 합성 워크플로우
    pub async fn compose(
        &self,
        config: AutoEditConfig,
        result_id: String,
        is_pro: bool,
    ) -> Result<AutoEditResult> {
        let durable_job_id = crate::video::current_auto_edit_job_id();
        let deferred_publication = durable_job_id.is_some();
        let progress_job_id = durable_job_id.unwrap_or_else(|| result_id.clone());
        info!("자동 편집 작업 시작: {} (Pro: {})", result_id, is_pro);

        self.update_progress(
            &progress_job_id,
            AutoEditStatus::SelectingClips,
            0.0,
            "자동 편집 초기화 중...".to_string(),
        )
        .await;

        // YouTube Shorts는 최대 3분(180초)까지 지원한다. 요청값(60/120/180 등)을
        // 존중하고 상한 초과분만 잘라낸다.
        let config = if config.target_duration > MAX_TARGET_DURATION_SECS {
            warn!(
                "target_duration {}초가 YouTube Shorts 최대 {}초를 초과합니다. {}초로 제한합니다.",
                config.target_duration, MAX_TARGET_DURATION_SECS, MAX_TARGET_DURATION_SECS
            );
            let mut clamped = config;
            clamped.target_duration = MAX_TARGET_DURATION_SECS;
            clamped
        } else {
            config
        };

        let start_time = std::time::Instant::now();

        self.update_progress(
            &progress_job_id,
            AutoEditStatus::SelectingClips,
            10.0,
            "DB에서 클립 불러오는 중...".to_string(),
        )
        .await;

        let all_clips = self.load_clips_from_games(&config.game_ids).await?;

        if all_clips.is_empty() {
            return Err(VideoError::NoClipsFound);
        }

        self.update_progress(
            &progress_job_id,
            AutoEditStatus::SelectingClips,
            20.0,
            format!("{}개의 클립 중 최적의 클립 선택 중...", all_clips.len()),
        )
        .await;

        let (selected_clips, prepared_clips) = self.resolve_timeline(&all_clips, &config).await?;

        if selected_clips.is_empty() {
            return Err(VideoError::NoClipsFound);
        }

        info!(
            "합성용 클립 {}개 선택됨 (목표: {}초)",
            selected_clips.len(),
            config.target_duration
        );

        self.update_progress(
            &progress_job_id,
            AutoEditStatus::PreparingClips,
            40.0,
            "클립 트리밍 및 전처리 중...".to_string(),
        )
        .await;

        // 최종 타임라인에서 각 하이라이트가 놓이는 시각. 줌·훅 자막이 같은 값을 쓴다.
        //
        // 예전에는 구간 중앙을 썼다("하이라이트는 대체로 클립 중앙에 위치"). 그
        // 가정은 클립마다 pre/post 가 다르므로 틀렸고, 게임 종료 클립에서는 20초
        // 어긋났다 — 줌이 승리 순간이 아니라 그 20초 전 아무 일 없는 지점에서
        // 걸렸다. 이제 저장된 `event_offset_secs` 를 트림 구간 기준으로 옮겨 쓴다.
        let timeline = Self::event_timeline(&prepared_clips, &selected_clips);

        let event_times: Option<Vec<f64>> = if config.enable_event_zoom {
            info!(
                "이벤트 줌 활성화: {}개 줌 시점 {:?}",
                timeline.len(),
                timeline
            );
            Some(timeline.clone())
        } else {
            None
        };

        self.update_progress(
            &progress_job_id,
            AutoEditStatus::Concatenating,
            60.0,
            "클립 연결 중...".to_string(),
        )
        .await;

        // 훅 자막: 각 클립 앞머리에 "무슨 장면이고 왜 볼 만한지" 한 줄.
        //
        // 산출물이 세로로 자른 게임 화면 그 자체였던 동안, 이 앱이 유일하게
        // 확언할 수 있는 것(그 순간의 체력·생존 인원)은 저장만 되고 영상에는
        // 한 번도 나오지 않았다. 끄고 싶은 사람을 위해 설정으로 남긴다.
        let captions: Option<Vec<Option<CaptionSpec>>> = if config.enable_hook_captions {
            let built: Vec<Option<CaptionSpec>> = selected_clips
                .iter()
                .map(|clip| clip_caption(clip, config.caption_locale))
                .collect();
            let shown = built.iter().filter(|c| c.is_some()).count();
            info!("훅 자막 {}개 / 클립 {}개", shown, built.len());
            Some(built)
        } else {
            None
        };

        // 단일 패스 합성(트림 + 9:16 스케일/크롭 + 선택적 이벤트 줌 + 훅 자막).
        let concatenated_path = self
            .concatenate_clips(
                &prepared_clips,
                event_times.as_deref(),
                captions,
                config.framing_mode,
            )
            .await?;

        // 완료 후 정리할 중간 산출물 추적(마지막 rendered 포함).
        let mut produced: Vec<PathBuf> = vec![concatenated_path.clone()];

        self.update_progress(
            &progress_job_id,
            AutoEditStatus::ApplyingCanvas,
            75.0,
            "캔버스 및 워터마크 적용 중...".to_string(),
        )
        .await;

        let with_overlay = if let Some(canvas) = &config.canvas_template {
            self.apply_canvas_overlay(&concatenated_path, canvas, is_pro)
                .await?
        } else if !is_pro {
            self.apply_watermark_only(&concatenated_path).await?
        } else {
            concatenated_path.clone()
        };
        if with_overlay != concatenated_path {
            produced.push(with_overlay.clone());
        }

        self.update_progress(
            &progress_job_id,
            AutoEditStatus::MixingAudio,
            90.0,
            "오디오 믹싱 중...".to_string(),
        )
        .await;

        let mixed_path = if let Some(music) = &config.background_music {
            let out = self
                .mix_audio(&with_overlay, music, &config.audio_levels)
                .await?;
            if out != with_overlay {
                produced.push(out.clone());
            }
            out
        } else {
            with_overlay.clone()
        };

        // 최종 라우드니스 정규화(오디오만 재인코딩, 비디오 -c copy). 설정 시에만 수행.
        let rendered_path = if let Some(target_lufs) = self.normalize_lufs {
            let normalized = self.stage_dir().join(format!(
                "normalized_{}.mp4",
                chrono::Local::now().format("%Y%m%d_%H%M%S")
            ));
            match self
                .video_processor
                .normalize_audio(&mixed_path, &normalized, target_lufs)
                .await
            {
                Ok(path) => {
                    if path != mixed_path {
                        produced.push(path.clone());
                    }
                    path
                }
                Err(e) => {
                    // 정규화 실패는 치명적이지 않다 — 정규화 전 결과를 그대로 사용.
                    warn!("오디오 정규화 실패, 정규화 전 결과 사용: {}", e);
                    mixed_path.clone()
                }
            }
        } else {
            mixed_path.clone()
        };

        // output_root가 설정되면 최종본을 앱 관리 루트로 이동하고 중간 산출물을 정리한다.
        // 미설정 시 %TEMP% 동작을 그대로 유지한다(하위호환).
        let final_path = self
            .finalize_output(
                &rendered_path,
                &result_id,
                &produced,
                deferred_publication,
                &progress_job_id,
            )
            .await?;

        let total_duration = self.video_processor.get_duration(&final_path).await?;

        let elapsed = start_time.elapsed().as_secs_f64();
        let result = AutoEditResult {
            output_path: final_path.to_string_lossy().to_string(),
            selected_clips: selected_clips.clone(),
            total_duration,
            clip_count: prepared_clips.len(),
        };

        let file_size = tokio::fs::metadata(&final_path)
            .await
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        let thumbnail_path = if deferred_publication {
            None
        } else {
            match auto_generate_thumbnail(
                &final_path,
                final_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(".")),
            )
            .await
            {
                Ok(path) => Some(path.to_string_lossy().to_string()),
                Err(e) => {
                    warn!("썸네일 생성 실패: {}", e);
                    None
                }
            }
        };

        let result_metadata = crate::storage::AutoEditResultMetadata {
            result_id: result_id.clone(),
            job_id: progress_job_id.clone(),
            output_path: final_path.to_string_lossy().to_string(),
            thumbnail_path,
            created_at: chrono::Utc::now(),
            duration: total_duration,
            clip_count: prepared_clips.len(),
            game_ids: config.game_ids.clone(),
            target_duration: config.target_duration,
            canvas_template_name: config.canvas_template.as_ref().map(|t| t.name.clone()),
            has_background_music: config.background_music.is_some(),
            youtube_status: Some(crate::storage::YouTubeUploadStatus {
                video_id: None,
                status: crate::storage::UploadStatus::NotUploaded,
                upload_started_at: None,
                upload_completed_at: None,
                progress: 0.0,
                error: None,
            }),
            file_size_bytes: file_size,
            publish_title: config.publish_metadata.title.clone(),
            publish_description: config.publish_metadata.description.clone(),
            publish_tags: config.publish_metadata.tags.clone(),
            publish_privacy_status: config.publish_metadata.privacy_status.clone(),
            output_intent: serde_json::to_value(config.output_intent)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default(),
            framing_mode: serde_json::to_value(config.framing_mode)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default(),
            platform_preset: serde_json::to_value(config.platform_preset)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default(),
            series_id: progress_job_id.clone(),
            part_index: 1,
            part_count: 1,
            output_kind: "short".to_string(),
            validation: None,
            platform_exports: Vec::new(),
        };

        if !deferred_publication {
            if let Err(e) = self.storage.save_auto_edit_result(&result_metadata) {
                warn!("자동 편집 결과 메타데이터 저장 실패: {}", e);
            }

            for clip in &selected_clips {
                if let Ok(mut clips) = self.storage.load_clip_metadata(&clip.game_id) {
                    let file_path = &clip.file_path;
                    if let Some(target_clip) = clips.iter_mut().find(|c| &c.file_path == file_path)
                    {
                        target_clip.usage_count += 1;
                        if let Err(e) = self.storage.save_clip_metadata(&clip.game_id, target_clip)
                        {
                            warn!("클립 사용 횟수 업데이트 실패 ({}): {}", file_path, e);
                        } else {
                            info!(
                                "클립 사용 횟수 증가: {} (Total: {})",
                                file_path, target_clip.usage_count
                            );
                        }
                    }
                }
            }
        }

        info!(
            "자동 편집 완료 ({:.2}초): {:?}",
            elapsed, result.output_path
        );

        Ok(result)
    }

    /// 클립 하나의 **재사용 감쇠를 뺀** 하이라이트 점수.
    ///
    /// 하이라이트 점수가 있으면 그것으로, 없으면(예전 클립) `priority` 를 같은
    /// 눈금으로 올려서 쓴다. 두 세대의 클립이 한 라이브러리에 섞여도 순서가
    /// 뒤집히지 않아야 한다 — priority 5 = 100, 1 = 20 으로 놓으면 점수
    /// 눈금(펜타 100 · 킬 25)과 대체로 겹친다.
    ///
    /// 프론트의 `src/lib/clipRanking.ts` 가 같은 폴백을 쓴다(드리프트 테스트가
    /// 이 파일의 `* 20.0` 을 읽어 대조한다). 홈은 여기에 감쇠를 곱하지 않는다 —
    /// "이 판에서 뭐가 좋았나" 의 답이 내가 자동편집을 몇 번 돌렸는지에 따라
    /// 달라지면 안 되기 때문이다.
    pub(super) fn base_score(c: &ClipInfo) -> f64 {
        c.highlight_score.unwrap_or((c.priority as f64) * 20.0)
    }

    pub async fn select_clips(
        &self,
        all_clips: &[ClipInfo],
        config: &AutoEditConfig,
    ) -> Result<Vec<ClipInfo>> {
        // 화면에서 고른 것이 먼저다 — 사용자가 직접 지목했는데 자동 선택이
        // 덮어쓰면 그 선택 UI 는 장식이 된다.
        if let Some(paths) = &config.selected_clip_paths {
            let requested: std::collections::HashSet<&str> =
                paths.iter().map(String::as_str).collect();
            let mut selected: Vec<ClipInfo> = all_clips
                .iter()
                .filter(|c| requested.contains(c.file_path.as_str()))
                .cloned()
                .collect();

            // 일부만 사라진 경우에도 조용히 축소하지 않는다. 직접 선택 계약은
            // "남아 있는 것만"이 아니라 사용자가 고른 전부를 포함하는 것이다.
            if selected.len() != requested.len() {
                return Err(VideoError::NoClipsFound);
            }

            // 순서는 클릭 순서가 아니라 **점수순**이다. 화면(홈)이 점수순으로
            // 보여주고 1위에 「최고의 순간」을 달아 두었으므로, 결과 영상의 첫
            // 장면도 그 클립이어야 사용자가 본 것과 어긋나지 않는다.
            selected.sort_by(|a, b| {
                Self::base_score(b)
                    .partial_cmp(&Self::base_score(a))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.id.cmp(&b.id))
            });

            return Ok(selected);
        }

        if let Some(selected_ids) = &config.selected_clip_ids {
            let selected: Vec<ClipInfo> = all_clips
                .iter()
                .filter(|c| selected_ids.contains(&c.id))
                .cloned()
                .collect();

            if selected.is_empty() {
                return Err(VideoError::NoClipsFound);
            }

            return Ok(selected);
        }

        let mut sorted_clips = all_clips.to_vec();

        // 재탕 방지는 **1차 정렬키가 아니라 점수 감쇠**로 넣는다.
        //
        // 예전에는 `usage_count` 오름차순이 1차 키였다. 그래서 두 번째 영상부터는
        // **이미 한 번 쓴 펜타킬이 아직 안 쓴 평범한 킬보다 뒤로 밀렸다** — 앞에
        // 와야 할 장면이 뒤로 가면 쇼츠의 훅이 통째로 무너진다.
        //
        // 감쇠 방식이면 "안 쓴 것을 선호하되, 충분히 좋은 장면은 재사용된다"가
        // 된다. 계수 0.6 은 한 등급쯤(펜타킬 100 -> 60, 트리플킬 70 자리) 내려가는
        // 값이라, 한 번 쓴 펜타킬은 안 쓴 트리플킬과 겨루게 된다.
        const REUSE_DECAY: f64 = 0.6;
        let effective_score = |c: &ClipInfo| -> f64 {
            let base = Self::base_score(c);
            if config.allow_duplicates {
                base
            } else {
                base * REUSE_DECAY.powi(c.usage_count.min(4) as i32)
            }
        };

        sorted_clips.sort_by(|a, b| {
            effective_score(b)
                .partial_cmp(&effective_score(a))
                .unwrap_or(std::cmp::Ordering::Equal)
                // 동점이면 안 쓴 것 먼저, 그 다음 이른 시각 — 실행마다 순서가
                // 달라지지 않도록 완전한 순서를 만든다.
                .then_with(|| a.usage_count.cmp(&b.usage_count))
                .then_with(|| a.id.cmp(&b.id))
        });

        let target_duration = config.target_duration as f64;
        let buffer_duration = target_duration * 0.9;

        let mut selected = Vec::new();
        let mut total_duration = 0.0;

        for clip in &sorted_clips {
            let clip_duration = clip.duration.unwrap_or(10.0);

            if total_duration + clip_duration <= buffer_duration {
                total_duration += clip_duration;
                selected.push(clip.clone());
            }

            if total_duration >= buffer_duration {
                break;
            }
        }

        if selected.is_empty() {
            if let Some(best_clip) = sorted_clips.first() {
                selected.push(best_clip.clone());
            } else {
                return Err(VideoError::NoClipsFound);
            }
        }

        Ok(selected)
    }

    /// Resolve either an exact reviewed storyboard or the legacy/automatic
    /// selector into one ordered clip list and exact source ranges.
    pub async fn resolve_timeline(
        &self,
        all_clips: &[ClipInfo],
        config: &AutoEditConfig,
    ) -> Result<(Vec<ClipInfo>, Vec<ClipSpec>)> {
        if config.storyboard.is_some() && config.selected_clip_paths.is_some() {
            return Err(VideoError::ProcessingError {
                message: "storyboard and selected_clip_paths cannot be sent together".to_string(),
            });
        }

        if let Some(storyboard) = &config.storyboard {
            if storyboard.is_empty() {
                return Err(VideoError::NoClipsFound);
            }
            let mut ordered = storyboard.clone();
            ordered.sort_by_key(|clip| clip.order);
            let mut seen_paths = std::collections::HashSet::new();
            let mut selected = Vec::with_capacity(ordered.len());
            let mut specs = Vec::with_capacity(ordered.len());

            for item in ordered {
                if !seen_paths.insert(item.file_path.clone()) {
                    return Err(VideoError::ProcessingError {
                        message: format!("duplicate storyboard path: {}", item.file_path),
                    });
                }
                let clip = all_clips
                    .iter()
                    .find(|clip| clip.game_id == item.game_id && clip.file_path == item.file_path)
                    .cloned()
                    .ok_or_else(|| VideoError::FileNotFound {
                        path: item.file_path.clone(),
                    })?;
                let source_duration = clip.duration.unwrap_or(0.0);
                if !item.trim_start_secs.is_finite()
                    || !item.trim_end_secs.is_finite()
                    || item.trim_start_secs < 0.0
                    || item.trim_end_secs <= item.trim_start_secs
                    || item.trim_end_secs > source_duration + 0.05
                {
                    return Err(VideoError::ProcessingError {
                        message: format!(
                            "invalid storyboard range {:.3}..{:.3} for {} ({:.3}s)",
                            item.trim_start_secs,
                            item.trim_end_secs,
                            item.file_path,
                            source_duration
                        ),
                    });
                }
                specs.push(ClipSpec {
                    path: PathBuf::from(&clip.file_path),
                    trim_start: (item.trim_start_secs > 0.001).then_some(item.trim_start_secs),
                    trim_duration: Some(item.trim_end_secs - item.trim_start_secs),
                });
                selected.push(clip);
            }
            return Ok((selected, specs));
        }

        let selected = self.select_clips(all_clips, config).await?;
        let preserve_full = config.selected_clip_paths.is_some();
        let specs = self
            .prepare_clips(&selected, config.target_duration, preserve_full)
            .await?;
        Ok((selected, specs))
    }

    pub async fn plan(&self, config: &AutoEditConfig) -> Result<AutoEditPlan> {
        let all_clips = self.load_clips_from_games(&config.game_ids).await?;
        if all_clips.is_empty() {
            return Err(VideoError::NoClipsFound);
        }
        let (selected, specs) = self.resolve_timeline(&all_clips, config).await?;
        let mut estimated_duration_secs = 0.0;
        let clips = selected
            .iter()
            .zip(specs.iter())
            .enumerate()
            .map(|(index, (clip, spec))| {
                let source_duration = clip.duration.unwrap_or(0.0);
                let start = spec.trim_start.unwrap_or(0.0);
                let duration = spec.trim_duration.unwrap_or(source_duration);
                estimated_duration_secs += duration;
                AutoEditPlanClip {
                    storyboard: StoryboardClip {
                        game_id: clip.game_id.clone(),
                        file_path: clip.file_path.clone(),
                        order: index as u32,
                        trim_start_secs: start,
                        trim_end_secs: (start + duration).min(source_duration),
                    },
                    source_duration_secs: source_duration,
                    event_offset_secs: clip.event_offset_secs,
                    event_type: clip.event_type.clone(),
                    highlight_score: Self::base_score(clip),
                    recommended_order: index as u32,
                    thumbnail_path: clip.thumbnail_path.clone(),
                }
            })
            .collect();
        let recommended_output_intent = if estimated_duration_secs > 180.0 {
            AutoEditOutputIntent::ShortsSeries
        } else {
            AutoEditOutputIntent::SingleShort
        };
        let estimated_part_count = if estimated_duration_secs <= 0.0 {
            0
        } else if recommended_output_intent == AutoEditOutputIntent::ShortsSeries {
            (estimated_duration_secs / 180.0).ceil() as usize
        } else {
            1
        };
        Ok(AutoEditPlan {
            clips,
            estimated_duration_secs,
            recommended_output_intent,
            estimated_part_count,
        })
    }

    /// 합쳐진 타임라인에서 각 클립의 **하이라이트 순간**이 놓이는 시각(초).
    ///
    /// 줌과 훅 자막이 같은 값을 봐야 한다 — 따로 계산하면 한쪽만 고쳐졌을 때
    /// 자막은 킬 순간에, 줌은 그 5초 전에 걸리는 식으로 조용히 어긋난다.
    ///
    /// `event_offset_secs` 가 없는 예전 클립은 구간 중앙으로 되돌아간다(예전 동작).
    pub(super) fn event_timeline(prepared: &[ClipSpec], selected: &[ClipInfo]) -> Vec<f64> {
        let mut times = Vec::with_capacity(prepared.len());
        let mut offset = 0.0f64;

        for (spec, clip) in prepared.iter().zip(selected.iter()) {
            let effective = spec
                .trim_duration
                .unwrap_or_else(|| clip.duration.unwrap_or(10.0));

            // 트림했으면 이벤트도 그만큼 앞으로 당겨진다. 구간을 벗어나면
            // (예전 클립 + 새 트림 조합) 가장자리로 붙인다.
            let within = match clip.event_offset_secs {
                Some(e) if e.is_finite() && e >= 0.0 => {
                    (e - spec.trim_start.unwrap_or(0.0)).clamp(0.0, effective)
                }
                _ => effective / 2.0,
            };

            times.push(offset + within);
            offset += effective;
        }

        times
    }

    pub(super) async fn load_clips_from_games(&self, game_ids: &[String]) -> Result<Vec<ClipInfo>> {
        let mut all_clips = Vec::new();
        let mut clip_id_counter = 0i64;

        for game_id in game_ids {
            let storage_clips = self.storage.load_clip_metadata(game_id).map_err(|e| {
                VideoError::ProcessingError {
                    message: format!("게임 {}의 클립 로드 실패: {}", game_id, e),
                }
            })?;

            info!(
                "게임 {}에서 {}개의 클립 로드됨",
                game_id,
                storage_clips.len()
            );

            for clip in storage_clips {
                let event_type = match &clip.event_type {
                    crate::storage::models::EventType::ChampionKill => "ChampionKill".to_string(),
                    crate::storage::models::EventType::Multikill(2) => "DoubleKill".to_string(),
                    crate::storage::models::EventType::Multikill(3) => "TripleKill".to_string(),
                    crate::storage::models::EventType::Multikill(4) => "QuadraKill".to_string(),
                    crate::storage::models::EventType::Multikill(5) => "PentaKill".to_string(),
                    crate::storage::models::EventType::Multikill(n) => {
                        format!("Multikill({})", n)
                    }
                    crate::storage::models::EventType::TurretKill => "TurretKill".to_string(),
                    crate::storage::models::EventType::InhibitorKill => "InhibitorKill".to_string(),
                    crate::storage::models::EventType::DragonKill => "DragonKill".to_string(),
                    crate::storage::models::EventType::BaronKill => "BaronKill".to_string(),
                    crate::storage::models::EventType::Ace => "Ace".to_string(),
                    crate::storage::models::EventType::FirstBlood => "FirstBlood".to_string(),
                    crate::storage::models::EventType::Custom(s) => s.clone(),
                };

                // 방어: 저장 시 duration=0.0 으로 기록된 클립은 실제 파일을 ffprobe로
                // 실측해 선별/트리밍/목표길이 로직이 무력화되지 않도록 backfill 한다.
                let duration = if clip.duration > 0.0 {
                    clip.duration
                } else {
                    match self
                        .video_processor
                        .get_duration(Path::new(&clip.file_path))
                        .await
                    {
                        Ok(measured) if measured > 0.0 => {
                            info!("클립 duration 백필: {} -> {:.2}s", clip.file_path, measured);
                            measured
                        }
                        _ => clip.duration, // 실측 실패 시 원값(0.0) 유지
                    }
                };

                all_clips.push(ClipInfo {
                    id: clip_id_counter,
                    game_id: game_id.clone(),
                    event_type,
                    event_time: clip.event_time,
                    priority: clip.priority as i32,
                    file_path: clip.file_path,
                    thumbnail_path: clip.thumbnail_path,
                    duration: Some(duration),
                    usage_count: clip.usage_count,
                    highlight_score: clip.highlight_score,
                    event_offset_secs: clip.event_offset_secs,
                    score_reasons: clip.score_reasons.clone(),
                });

                clip_id_counter += 1;
            }
        }

        info!(
            "총 {}개 게임에서 {}개 클립 로드됨",
            game_ids.len(),
            all_clips.len()
        );

        Ok(all_clips)
    }

    pub(super) async fn update_progress(
        &self,
        job_id: &str,
        status: AutoEditStatus,
        progress: f64,
        current_step: String,
    ) {
        let mut progress_guard = self.progress.write().await;
        *progress_guard = Some(AutoEditProgress {
            job_id: job_id.to_string(),
            status,
            progress,
            current_step,
            elapsed_seconds: 0.0,
            estimated_seconds: 120.0,
            output_path: None,
            error: None,
            outputs: Vec::new(),
        });
    }

    pub(super) async fn update_progress_complete(
        &self,
        job_id: &str,
        output_path: String,
        elapsed: f64,
    ) {
        let mut progress_guard = self.progress.write().await;
        *progress_guard = Some(AutoEditProgress {
            job_id: job_id.to_string(),
            status: AutoEditStatus::Completed,
            progress: 100.0,
            current_step: "자동 편집 완료!".to_string(),
            elapsed_seconds: elapsed,
            estimated_seconds: elapsed,
            output_path: Some(output_path),
            error: None,
            outputs: Vec::new(),
        });
    }

    pub async fn update_progress_failed(&self, job_id: &str, error: String, elapsed: f64) {
        let mut progress_guard = self.progress.write().await;
        *progress_guard = Some(AutoEditProgress {
            job_id: job_id.to_string(),
            status: AutoEditStatus::Failed,
            progress: 0.0,
            current_step: "자동 편집 실패".to_string(),
            elapsed_seconds: elapsed,
            estimated_seconds: elapsed,
            output_path: None,
            error: Some(error),
            outputs: Vec::new(),
        });
    }

    pub async fn update_progress_completed_outputs(
        &self,
        job_id: &str,
        outputs: Vec<AutoEditOutput>,
        elapsed: f64,
    ) {
        let first_path = outputs.first().map(|output| output.output_path.clone());
        let mut progress_guard = self.progress.write().await;
        *progress_guard = Some(AutoEditProgress {
            job_id: job_id.to_string(),
            status: AutoEditStatus::Completed,
            progress: 100.0,
            current_step: "Completed".to_string(),
            elapsed_seconds: elapsed,
            estimated_seconds: elapsed,
            output_path: first_path,
            error: None,
            outputs,
        });
    }

    pub async fn get_progress(&self, job_id: &str) -> Option<AutoEditProgress> {
        self.progress
            .read()
            .await
            .clone()
            .filter(|progress| progress.job_id == job_id)
    }

    /// 최종 렌더 결과를 확정한다.
    ///
    /// - output_root 설정 시: `rendered`를 `<root>/<job_id>.mp4`로 이동하고,
    ///   중간 산출물(`produced`)을 모두 삭제해 최종본만 보존한다.
    /// - 미설정 시: 이동/정리 없이 `rendered` 경로를 그대로 사용한다(하위호환).
    pub(super) async fn finalize_output(
        &self,
        rendered: &Path,
        job_id: &str,
        produced: &[PathBuf],
        deferred_publication: bool,
        durable_job_id: &str,
    ) -> Result<PathBuf> {
        // output_root 미설정 → 기존 %TEMP% 동작 유지(이동/정리 없음).
        if self.output_root.is_none() {
            return Ok(rendered.to_path_buf());
        }

        let final_dir = if deferred_publication {
            self.job_stage_dir(durable_job_id).join("parts")
        } else {
            self.final_dir()
        };
        tokio::fs::create_dir_all(&final_dir)
            .await
            .map_err(|e| VideoError::ProcessingError {
                message: format!("산출물 루트 생성 실패 ({:?}): {}", final_dir, e),
            })?;
        let target = final_dir.join(if deferred_publication {
            format!("{}.partial.mp4", job_id)
        } else {
            format!("{}.mp4", job_id)
        });

        // rendered → target 이동(같은 볼륨이면 rename, 아니면 copy+remove 폴백).
        if let Err(rename_err) = tokio::fs::rename(rendered, &target).await {
            tokio::fs::copy(rendered, &target)
                .await
                .map_err(|e| VideoError::ProcessingError {
                    message: format!("최종본 이동 실패 (rename: {}, copy: {})", rename_err, e),
                })?;
            if let Err(e) = tokio::fs::remove_file(rendered).await {
                warn!("이동 후 원본 정리 실패 ({:?}): {}", rendered, e);
            }
        }

        // 중간 산출물 정리 — 최종본은 제외. rendered는 이미 이동됐으므로 존재하지 않는다.
        for path in produced {
            if path.as_path() == target {
                continue;
            }
            if let Err(e) = tokio::fs::remove_file(path).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!("중간 산출물 삭제 실패 ({:?}): {}", path, e);
                }
            }
        }

        info!("최종본 보존: {:?} (중간 {}개 정리)", target, produced.len());
        Ok(target)
    }
}
