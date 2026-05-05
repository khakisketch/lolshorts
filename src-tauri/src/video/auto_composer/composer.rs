#![allow(dead_code)]
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::super::thumbnail::auto_generate_thumbnail;
use super::super::{ClipInfo, Result, VideoError, VideoProcessor};
use super::types::{AutoEditConfig, AutoEditProgress, AutoEditResult, AutoEditStatus};
use crate::storage::Storage;

/// YouTube Shorts 생성을 위한 자동 편집기 (Auto-Composer)
pub struct AutoComposer {
    pub(super) video_processor: Arc<VideoProcessor>,
    pub(super) storage: Arc<Storage>,
    pub(super) progress: Arc<RwLock<Option<AutoEditProgress>>>,
}

impl AutoComposer {
    /// 새로운 AutoComposer 인스턴스 생성
    pub fn new(video_processor: Arc<VideoProcessor>, storage: Arc<Storage>) -> Self {
        Self {
            video_processor,
            storage,
            progress: Arc::new(RwLock::new(None)),
        }
    }

    /// 메인 합성 워크플로우
    pub async fn compose(
        &self,
        config: AutoEditConfig,
        job_id: String,
        is_pro: bool,
    ) -> Result<AutoEditResult> {
        info!("자동 편집 작업 시작: {} (Pro: {})", job_id, is_pro);

        self.update_progress(
            &job_id,
            AutoEditStatus::Processing,
            0.0,
            "자동 편집 초기화 중...".to_string(),
        )
        .await;

        // YouTube Shorts 최대 60초 제한 검증
        if config.target_duration > 60 {
            warn!(
                "target_duration {}초가 YouTube Shorts 최대 60초를 초과합니다. 60초로 제한합니다.",
                config.target_duration
            );
        }
        let config = if config.target_duration > 60 {
            let mut clamped = config;
            clamped.target_duration = 60;
            clamped
        } else {
            config
        };

        let start_time = std::time::Instant::now();

        self.update_progress(
            &job_id,
            AutoEditStatus::Processing,
            10.0,
            "DB에서 클립 불러오는 중...".to_string(),
        )
        .await;

        let all_clips = self.load_clips_from_games(&config.game_ids).await?;

        if all_clips.is_empty() {
            return Err(VideoError::NoClipsFound);
        }

        self.update_progress(
            &job_id,
            AutoEditStatus::Processing,
            20.0,
            format!("{}개의 클립 중 최적의 클립 선택 중...", all_clips.len()),
        )
        .await;

        let selected_clips = self.select_clips(&all_clips, &config).await?;

        if selected_clips.is_empty() {
            return Err(VideoError::NoClipsFound);
        }

        info!(
            "합성용 클립 {}개 선택됨 (목표: {}초)",
            selected_clips.len(),
            config.target_duration
        );

        self.update_progress(
            &job_id,
            AutoEditStatus::Processing,
            40.0,
            "클립 트리밍 및 전처리 중...".to_string(),
        )
        .await;

        let prepared_clips = self
            .prepare_clips(&selected_clips, config.target_duration)
            .await?;

        self.update_progress(
            &job_id,
            AutoEditStatus::Processing,
            60.0,
            "클립 연결 중...".to_string(),
        )
        .await;

        let concatenated_path = self.concatenate_clips(&prepared_clips).await?;

        self.update_progress(
            &job_id,
            AutoEditStatus::Processing,
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
            concatenated_path
        };

        self.update_progress(
            &job_id,
            AutoEditStatus::Processing,
            90.0,
            "오디오 믹싱 중...".to_string(),
        )
        .await;

        let final_path = if let Some(music) = &config.background_music {
            self.mix_audio(&with_overlay, music, &config.audio_levels)
                .await?
        } else {
            with_overlay
        };

        let total_duration = self.video_processor.get_duration(&final_path).await?;

        let elapsed = start_time.elapsed().as_secs_f64();
        self.update_progress_complete(&job_id, final_path.to_string_lossy().to_string(), elapsed)
            .await;

        let result = AutoEditResult {
            output_path: final_path.to_string_lossy().to_string(),
            selected_clips: selected_clips.clone(),
            total_duration,
            clip_count: prepared_clips.len(),
        };

        let file_size = std::fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);

        let thumbnail_path = match auto_generate_thumbnail(
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
        };

        let result_metadata = crate::storage::AutoEditResultMetadata {
            result_id: job_id.clone(),
            job_id: job_id.clone(),
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
        };

        if let Err(e) = self.storage.save_auto_edit_result(&result_metadata) {
            warn!("자동 편집 결과 메타데이터 저장 실패: {}", e);
        }

        for clip in &selected_clips {
            if let Ok(mut clips) = self.storage.load_clip_metadata(&clip.game_id) {
                let file_path = &clip.file_path;
                if let Some(target_clip) = clips.iter_mut().find(|c| &c.file_path == file_path) {
                    target_clip.usage_count += 1;
                    if let Err(e) = self.storage.save_clip_metadata(&clip.game_id, target_clip) {
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

        info!(
            "자동 편집 완료 ({:.2}초): {:?}",
            elapsed, result.output_path
        );

        Ok(result)
    }

    pub async fn select_clips(
        &self,
        all_clips: &[ClipInfo],
        config: &AutoEditConfig,
    ) -> Result<Vec<ClipInfo>> {
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

        sorted_clips.sort_by(|a, b| {
            if !config.allow_duplicates {
                let usage_cmp = a.usage_count.cmp(&b.usage_count);
                if usage_cmp != std::cmp::Ordering::Equal {
                    return usage_cmp;
                }
            }
            b.priority.cmp(&a.priority)
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

                all_clips.push(ClipInfo {
                    id: clip_id_counter,
                    game_id: game_id.clone(),
                    event_type,
                    event_time: clip.event_time,
                    priority: clip.priority as i32,
                    file_path: clip.file_path,
                    thumbnail_path: clip.thumbnail_path,
                    duration: Some(clip.duration),
                    usage_count: clip.usage_count,
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
        });
    }

    pub(super) async fn update_progress_failed(&self, job_id: &str, error: String, elapsed: f64) {
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
        });
    }

    pub async fn get_progress(&self) -> Option<AutoEditProgress> {
        self.progress.read().await.clone()
    }
}
