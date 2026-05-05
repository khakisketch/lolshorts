#![allow(dead_code)]
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use super::super::processor::effects::{escape_ffmpeg_text, validate_ffmpeg_color};
use super::super::{execute_ffmpeg_command, ClipInfo, Result, VideoError};
use super::composer::AutoComposer;
use super::types::{AudioLevels, BackgroundLayer, BackgroundMusic, CanvasElement, CanvasTemplate};
use crate::utils::ffmpeg::get_ffmpeg_path;

impl AutoComposer {
    pub(super) async fn prepare_clips(
        &self,
        clips: &[ClipInfo],
        target_duration: u32,
    ) -> Result<Vec<PathBuf>> {
        let output_dir = std::env::temp_dir().join("lolshorts_auto_edit");
        tokio::fs::create_dir_all(&output_dir)
            .await
            .map_err(|e| VideoError::ProcessingError {
                message: format!("임시 디렉토리 생성 실패: {}", e),
            })?;

        let total_duration: f64 = clips.iter().map(|c| c.duration.unwrap_or(10.0)).sum();
        let target = target_duration as f64;
        let buffer_target = target * 0.9;

        info!(
            "클립 {}개 준비 중: 총 {:.1}초, 목표 {:.1}초",
            clips.len(),
            total_duration,
            target
        );

        if total_duration <= buffer_target {
            info!("총 길이가 목표 범위 내이므로 원본 클립 사용");
            let paths: Vec<PathBuf> = clips.iter().map(|c| PathBuf::from(&c.file_path)).collect();

            for path in &paths {
                if !path.exists() {
                    return Err(VideoError::FileNotFound {
                        path: path.display().to_string(),
                    });
                }
            }

            return Ok(paths);
        }

        info!(
            "총 길이 {:.1}초가 목표 {:.1}초를 초과하여 지능형 트리밍 적용",
            total_duration, buffer_target
        );

        let trim_factor = buffer_target / total_duration;
        let mut prepared_paths = Vec::new();

        for (idx, clip) in clips.iter().enumerate() {
            let input_path = PathBuf::from(&clip.file_path);

            if !input_path.exists() {
                return Err(VideoError::FileNotFound {
                    path: input_path.display().to_string(),
                });
            }

            let clip_duration = clip.duration.unwrap_or(10.0);
            let trimmed_duration = (clip_duration * trim_factor).max(3.0);

            if (clip_duration - trimmed_duration).abs() < 0.5 {
                info!(
                    "클립 {} ({:.1}초): 원본 사용 (트리밍 차이 <0.5초)",
                    idx, clip_duration
                );
                prepared_paths.push(input_path);
                continue;
            }

            let start_time = (clip_duration - trimmed_duration) / 2.0;
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let output_path = output_dir.join(format!("trimmed_{}_{}.mp4", idx, timestamp));

            info!(
                "클립 {} 트리밍: {:.1}초 -> {:.1}초 (시작점={:.1}초)",
                idx, clip_duration, trimmed_duration, start_time
            );

            self.video_processor
                .extract_clip(&input_path, &output_path, start_time, trimmed_duration)
                .await
                .map_err(|e| VideoError::ProcessingError {
                    message: format!("클립 {} 트리밍 실패: {}", idx, e),
                })?;

            prepared_paths.push(output_path);
        }

        info!(
            "{}개 클립 준비 완료 ({}개 트리밍됨)",
            clips.len(),
            clips.len() - prepared_paths.len()
        );

        Ok(prepared_paths)
    }

    pub(super) async fn concatenate_clips(&self, clip_paths: &[PathBuf]) -> Result<PathBuf> {
        let output_dir = std::env::temp_dir().join("lolshorts_auto_edit");
        tokio::fs::create_dir_all(&output_dir)
            .await
            .map_err(|e| VideoError::ProcessingError {
                message: format!("임시 디렉토리 생성 실패: {}", e),
            })?;

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let output_path = output_dir.join(format!("concatenated_{}.mp4", timestamp));

        self.video_processor
            .compose_shorts(clip_paths, &output_path, 1080, 1920)
            .await
    }

    pub(super) async fn apply_canvas_overlay(
        &self,
        video_path: &Path,
        canvas: &CanvasTemplate,
        is_pro: bool,
    ) -> Result<PathBuf> {
        let output_dir = std::env::temp_dir().join("lolshorts_auto_edit");
        tokio::fs::create_dir_all(&output_dir).await.map_err(|e| {
            VideoError::CanvasApplicationError {
                reason: format!("임시 디렉토리 생성 실패: {}", e),
            }
        })?;

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let output_path = output_dir.join(format!("with_canvas_{}.mp4", timestamp));

        info!("캔버스 템플릿 적용: {}", canvas.name);

        const WIDTH: u32 = 1080;
        const HEIGHT: u32 = 1920;

        let mut filter_parts: Vec<String> = Vec::new();

        match &canvas.background {
            BackgroundLayer::Color { value } => {
                filter_parts.push(format!("color=c={}:s={}x{}:d=1[bg]", value, WIDTH, HEIGHT));
                filter_parts.push("[0:v][bg]overlay=shortest=1".to_string());
            }
            BackgroundLayer::Gradient { value } => {
                let colors: Vec<&str> = value.split(':').collect();
                if colors.len() == 2 {
                    filter_parts.push(format!(
                        "color=c={}:s={}x{}:d=1,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)',fade=type=in:duration=0:color={}[bg]",
                        colors[0], WIDTH, HEIGHT, colors[1]
                    ));
                    filter_parts.push("[0:v][bg]overlay=shortest=1".to_string());
                } else {
                    filter_parts.push(format!("color=c=black:s={}x{}:d=1[bg]", WIDTH, HEIGHT));
                    filter_parts.push("[0:v][bg]overlay=shortest=1".to_string());
                }
            }
            BackgroundLayer::Image { path } => {
                let bg_path = PathBuf::from(path);
                if bg_path.exists() {
                    let safe_path = path.replace('\\', "\\\\").replace(':', "\\:");
                    filter_parts.push(format!(
                        "movie={}[bg_img];[bg_img]scale={}:{}:force_original_aspect_ratio=increase,crop={}:{},boxblur=20[bg]",
                        safe_path, WIDTH, HEIGHT, WIDTH, HEIGHT
                    ));
                    filter_parts.push("[0:v][bg]overlay=shortest=1".to_string());
                } else {
                    warn!("배경 이미지를 찾을 수 없음: {}", path);
                }
            }
        }

        for element in canvas.elements.iter() {
            if let CanvasElement::Text {
                content,
                font,
                size,
                color,
                outline,
                position,
                ..
            } = element
            {
                let x = (position.x * WIDTH as f32 / 100.0) as u32;
                let y = (position.y * HEIGHT as f32 / 100.0) as u32;
                // FIX #2: Use proper FFmpeg text escaping to prevent filter injection
                let safe_content = escape_ffmpeg_text(content);
                // FIX #3: Validate colors to prevent FFmpeg injection; default to white on invalid
                let safe_color =
                    validate_ffmpeg_color(color).unwrap_or_else(|_| "white".to_string());
                let mut drawtext = format!(
                    "drawtext=text='{}':fontfile={}:fontsize={}:fontcolor={}:x={}:y={}",
                    safe_content, font, size, safe_color, x, y
                );
                if let Some(outline_color) = outline {
                    let safe_outline = validate_ffmpeg_color(outline_color)
                        .unwrap_or_else(|_| "black".to_string());
                    drawtext.push_str(&format!(":borderw=2:bordercolor={}", safe_outline));
                }
                filter_parts.push(drawtext);
            }
        }

        for (idx, element) in canvas.elements.iter().enumerate() {
            if let CanvasElement::Image {
                path,
                width,
                height,
                position,
                ..
            } = element
            {
                let img_path = PathBuf::from(path);
                if !img_path.exists() {
                    warn!("오버레이 이미지를 찾을 수 없음: {}", path);
                    continue;
                }
                let x = (position.x * WIDTH as f32 / 100.0) as u32;
                let y = (position.y * HEIGHT as f32 / 100.0) as u32;
                let safe_path = path.replace('\\', "\\\\").replace(':', "\\:");
                filter_parts.push(format!(
                    "movie={}[img{}];[img{}]scale={}:{}[scaled_img{}]",
                    safe_path, idx, idx, width, height, idx
                ));
                filter_parts.push(format!("overlay={}:{}[out{}]", x, y, idx));
            }
        }

        if !is_pro {
            info!("Free Tier 감지: 워터마크 추가");
            let watermark_text = "LoLShorts Free Tier";
            if !filter_parts.is_empty() {
                let last_idx = filter_parts.len() - 1;
                filter_parts[last_idx].push_str(&format!(
                    ",drawtext=text='{}':fontsize=36:fontcolor=white@0.5:x=w-tw-20:y=h-th-20:shadowx=2:shadowy=2",
                    watermark_text
                ));
            } else {
                filter_parts.push(format!(
                    "drawtext=text='{}':fontsize=36:fontcolor=white@0.5:x=w-tw-20:y=h-th-20:shadowx=2:shadowy=2",
                    watermark_text
                ));
            }
        }

        if filter_parts.is_empty() {
            info!("적용할 필터가 없음");
            return Ok(video_path.to_path_buf());
        }

        let filter_complex = filter_parts.join(";");
        let ffmpeg_path = get_ffmpeg_path().map_err(|e| VideoError::ProcessingError {
            message: format!("FFmpeg를 찾을 수 없음: {}", e),
        })?;

        let mut command = tokio::process::Command::new(ffmpeg_path);
        command.args([
            "-i",
            video_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: video_path.display().to_string(),
                })?,
            "-filter_complex",
            &filter_complex,
        ]);
        for arg in self.video_processor.get_optimal_encoder().get_ffmpeg_args() {
            command.arg(arg);
        }
        command.args([
            "-c:a",
            "copy",
            // faststart moves moov atom to front for faster YouTube streaming start
            "-movflags",
            "+faststart",
            // prevents muxer overflow when complex filter graphs produce uneven streams
            "-max_muxing_queue_size",
            "1024",
            "-y",
            output_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: output_path.display().to_string(),
                })?,
        ]);

        execute_ffmpeg_command(&mut command).await.map_err(|e| {
            VideoError::CanvasApplicationError {
                reason: e.to_string(),
            }
        })?;

        info!("캔버스 오버레이 적용 완료");
        Ok(output_path)
    }

    pub(super) async fn apply_watermark_only(&self, video_path: &Path) -> Result<PathBuf> {
        let output_dir = std::env::temp_dir().join("lolshorts_auto_edit");
        tokio::fs::create_dir_all(&output_dir)
            .await
            .map_err(|e| VideoError::ProcessingError {
                message: format!("임시 디렉토리 생성 실패: {}", e),
            })?;

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let output_path = output_dir.join(format!("watermarked_{}.mp4", timestamp));

        info!("워터마크 적용 중 (Free Tier)...");

        let watermark_text = "LoLShorts Free Tier";
        let filter = format!(
            "drawtext=text='{}':fontsize=36:fontcolor=white@0.5:x=w-tw-20:y=h-th-20:shadowx=2:shadowy=2",
            watermark_text
        );

        let ffmpeg_path = get_ffmpeg_path().map_err(|e| VideoError::ProcessingError {
            message: format!("FFmpeg를 찾을 수 없음: {}", e),
        })?;

        let mut command = tokio::process::Command::new(ffmpeg_path);
        command.args([
            "-i",
            video_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: video_path.display().to_string(),
                })?,
            "-vf",
            &filter,
        ]);
        for arg in self.video_processor.get_optimal_encoder().get_ffmpeg_args() {
            command.arg(arg);
        }
        command.args([
            "-c:a",
            "copy",
            // faststart moves moov atom to front for faster YouTube streaming start
            "-movflags",
            "+faststart",
            // prevents muxer overflow when audio/video packet queues diverge
            "-max_muxing_queue_size",
            "1024",
            "-y",
            output_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: output_path.display().to_string(),
                })?,
        ]);

        execute_ffmpeg_command(&mut command).await?;
        Ok(output_path)
    }

    pub(super) async fn mix_audio(
        &self,
        video_path: &Path,
        music: &BackgroundMusic,
        levels: &AudioLevels,
    ) -> Result<PathBuf> {
        let output_dir = std::env::temp_dir().join("lolshorts_auto_edit");
        tokio::fs::create_dir_all(&output_dir)
            .await
            .map_err(|e| VideoError::AudioMixingError {
                reason: format!("임시 디렉토리 생성 실패: {}", e),
            })?;

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let output_path = output_dir.join(format!("with_audio_{}.mp4", timestamp));

        let music_path = PathBuf::from(&music.file_path);
        if !music_path.exists() {
            return Err(VideoError::BackgroundMusicNotFound {
                path: music.file_path.clone(),
            });
        }

        info!(
            "오디오 믹싱: 게임={}%, 음악={}%",
            levels.game_audio, levels.background_music
        );

        let game_volume = levels.game_audio as f64 / 100.0;
        let music_volume = levels.background_music as f64 / 100.0;

        let video_duration = self
            .video_processor
            .get_duration(video_path)
            .await
            .map_err(|e| VideoError::AudioMixingError {
                reason: format!("영상 길이 확인 실패: {}", e),
            })?;

        info!("영상 길이: {:.1}초", video_duration);

        let mut audio_filter = String::new();
        audio_filter.push_str(&format!("[0:a]volume={}[game_audio];", game_volume));

        let fade_duration = 3.0;
        let fade_out_start = (video_duration - fade_duration).max(0.0);

        if music.loop_music {
            audio_filter.push_str(&format!(
                "[1:a]aloop=loop=-1:size=2e+09,atrim=0:{},volume={},afade=t=in:st=0:d={},afade=t=out:st={}:d={}[bg_music]",
                video_duration, music_volume, fade_duration, fade_out_start, fade_duration
            ));
        } else {
            audio_filter.push_str(&format!(
                "[1:a]volume={},afade=t=in:st=0:d={},afade=t=out:st={}:d={}[bg_music]",
                music_volume, fade_duration, fade_out_start, fade_duration
            ));
        }

        audio_filter.push_str("[game_audio][bg_music]amix=inputs=2:duration=first[audio_out]");

        info!("오디오 필터 체인: {}", audio_filter);

        let ffmpeg_path = get_ffmpeg_path().map_err(|e| VideoError::ProcessingError {
            message: format!("FFmpeg를 찾을 수 없음: {}", e),
        })?;

        let mut command = tokio::process::Command::new(ffmpeg_path);
        command.args([
            "-i",
            video_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: video_path.display().to_string(),
                })?,
            "-i",
            music_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: music_path.display().to_string(),
                })?,
            "-filter_complex",
            &audio_filter,
            "-map",
            "0:v",
            "-map",
            "[audio_out]",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-shortest",
            "-y",
            output_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: output_path.display().to_string(),
                })?,
        ]);

        execute_ffmpeg_command(&mut command)
            .await
            .map_err(|e| VideoError::AudioMixingError {
                reason: e.to_string(),
            })?;

        info!("오디오 믹싱 완료");
        Ok(output_path)
    }
}
