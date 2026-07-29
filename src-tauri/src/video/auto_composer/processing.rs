#![allow(dead_code)]
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use super::super::processor::effects::{escape_ffmpeg_text, validate_ffmpeg_color};
use super::super::{execute_ffmpeg_command, ClipInfo, Result, VideoError};
use super::composer::AutoComposer;
use super::types::{AudioLevels, BackgroundLayer, BackgroundMusic, CanvasElement, CanvasTemplate};
use crate::utils::ffmpeg::get_ffmpeg_path;
use crate::video::processor::types::{ClipSpec, ComposeOptions};

impl AutoComposer {
    pub(super) async fn prepare_clips(
        &self,
        clips: &[ClipInfo],
        target_duration: u32,
    ) -> Result<Vec<ClipSpec>> {
        let total_duration: f64 = clips.iter().map(|c| c.duration.unwrap_or(10.0)).sum();
        let target = target_duration as f64;
        let buffer_target = target * 0.9;

        info!(
            "클립 {}개 준비 중: 총 {:.1}초, 목표 {:.1}초",
            clips.len(),
            total_duration,
            target
        );

        // 모든 소스 파일 존재 확인.
        for clip in clips {
            let path = PathBuf::from(&clip.file_path);
            if !path.exists() {
                return Err(VideoError::FileNotFound {
                    path: path.display().to_string(),
                });
            }
        }

        if total_duration <= buffer_target {
            info!("총 길이가 목표 범위 내이므로 원본 클립 전체 사용(트림 없음)");
            return Ok(clips
                .iter()
                .map(|c| ClipSpec {
                    path: PathBuf::from(&c.file_path),
                    trim_start: None,
                    trim_duration: None,
                })
                .collect());
        }

        info!(
            "총 길이 {:.1}초가 목표 {:.1}초를 초과하여 트림 구간 계산 적용",
            total_duration, buffer_target
        );

        // 세대 손실 방지: 여기서는 재인코딩하지 않고 트림 구간(start/duration)만 계산한다.
        // 실제 트림 + 스케일/크롭은 compose_with_options 단일 패스가 -ss/-t 로 수행.
        let trim_factor = buffer_target / total_duration;
        let mut specs = Vec::with_capacity(clips.len());

        for (idx, clip) in clips.iter().enumerate() {
            let path = PathBuf::from(&clip.file_path);
            let clip_duration = clip.duration.unwrap_or(10.0);
            let trimmed_duration = (clip_duration * trim_factor).max(3.0);

            if (clip_duration - trimmed_duration).abs() < 0.5 {
                info!(
                    "클립 {} ({:.1}초): 원본 사용 (트리밍 차이 <0.5초)",
                    idx, clip_duration
                );
                specs.push(ClipSpec {
                    path,
                    trim_start: None,
                    trim_duration: None,
                });
                continue;
            }

            let start_time = (clip_duration - trimmed_duration) / 2.0;
            info!(
                "클립 {} 트림 구간: {:.1}초 -> {:.1}초 (시작점={:.1}초)",
                idx, clip_duration, trimmed_duration, start_time
            );
            specs.push(ClipSpec {
                path,
                trim_start: Some(start_time),
                trim_duration: Some(trimmed_duration),
            });
        }

        info!(
            "{}개 클립 준비 완료(구간 계산 방식, 재인코딩 없음)",
            specs.len()
        );
        Ok(specs)
    }

    pub(super) async fn concatenate_clips(
        &self,
        clip_specs: &[ClipSpec],
        event_times: Option<&[f64]>,
    ) -> Result<PathBuf> {
        let output_dir = self.stage_dir();
        tokio::fs::create_dir_all(&output_dir)
            .await
            .map_err(|e| VideoError::ProcessingError {
                message: format!("임시 디렉토리 생성 실패: {}", e),
            })?;

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let output_path = output_dir.join(format!("concatenated_{}.mp4", timestamp));

        // 단일 패스: 클립별 트림(-ss/-t) + 9:16 스케일/크롭 + 선택적 이벤트 줌.
        let opts = ComposeOptions {
            width: 1080,
            height: 1920,
            transition: None,
            event_times: event_times.map(|t| t.to_vec()),
            // 소스(리플레이 버퍼)는 60fps → 줌 활성화 시 zoompan fps 로 전달.
            fps: event_times.map(|_| 60),
            // 라우드니스 정규화는 오디오 믹싱까지 끝난 최종 단계에서 별도 수행.
            normalize_audio: None,
        };

        self.video_processor
            .compose_with_options(clip_specs, &output_path, &opts)
            .await?;

        Ok(output_path)
    }

    pub(super) async fn apply_canvas_overlay(
        &self,
        video_path: &Path,
        canvas: &CanvasTemplate,
        is_pro: bool,
    ) -> Result<PathBuf> {
        let output_dir = self.stage_dir();
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

        // 배경을 비디오 길이만큼 유지하기 위해 소스 길이를 실측(실패 시 fallback).
        let video_duration = self
            .video_processor
            .get_duration(video_path)
            .await
            .unwrap_or(0.0);

        // 올바른 z-순서: 배경(맨 아래) → 게임 비디오 → 텍스트/이미지 → 워터마크(최상단).
        // filter_complex 를 명시적 in/out 라벨로 체이닝해 스트림 순서를 보장한다.
        let mut parts: Vec<String> = Vec::new();
        let mut current: String = "0:v".to_string();
        let mut label_seq = 0u32;

        // 1) 배경 레이어([bg])를 비디오 길이만큼 생성.
        let bg_available = build_background_layer(
            &mut parts,
            &canvas.background,
            WIDTH,
            HEIGHT,
            video_duration,
        );

        if bg_available {
            // 게임 비디오를 배경 위에 올린다(템플릿 지오메트리 없음 → 풀프레임 중앙).
            // shortest=1 로 배경/게임 중 짧은 쪽(=비디오 길이)에 맞춰 컷 → 결과 길이=비디오 길이.
            parts.push("[bg][0:v]overlay=(W-w)/2:(H-h)/2:shortest=1[base]".to_string());
            current = "base".to_string();
        }

        // 2) 텍스트 오버레이(게임 위).
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
                // 필터 인젝션 방지: 텍스트 이스케이프 + 색상 화이트리스트 검증.
                let safe_content = escape_ffmpeg_text(content);
                let safe_color =
                    validate_ffmpeg_color(color).unwrap_or_else(|_| "white".to_string());
                // 프론트는 폰트 "이름"(예: "Arial")을 보내는데 drawtext 의 fontfile= 은
                // 파일 경로 전용이라 이름을 그대로 넣으면 fontconfig 폴백으로 exit 0 은 나오지만
                // 한글 글리프가 없어 빈 네모(tofu)로 렌더링된다 — 실제 폰트 파일 절로 변환한다.
                let font_clause = resolve_font_clause(font);
                let mut drawtext = format!(
                    "drawtext=text='{}':{}:fontsize={}:fontcolor={}:x={}:y={}",
                    safe_content, font_clause, size, safe_color, x, y
                );
                if let Some(outline_color) = outline {
                    let safe_outline = validate_ffmpeg_color(outline_color)
                        .unwrap_or_else(|_| "black".to_string());
                    drawtext.push_str(&format!(":borderw=2:bordercolor={}", safe_outline));
                }
                let out = format!("v{}", label_seq);
                label_seq += 1;
                parts.push(format!("[{}]{}[{}]", current, drawtext, out));
                current = out;
            }
        }

        // 3) 이미지 오버레이(텍스트 위).
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
                parts.push(format!(
                    "movie={}[imgsrc{}];[imgsrc{}]scale={}:{}[img{}]",
                    safe_path, idx, idx, width, height, idx
                ));
                let out = format!("v{}", label_seq);
                label_seq += 1;
                parts.push(format!(
                    "[{}][img{}]overlay={}:{}[{}]",
                    current, idx, x, y, out
                ));
                current = out;
            }
        }

        // 4) FREE 티어 워터마크는 항상 최상단 레이어.
        if !is_pro {
            info!("Free Tier 감지: 워터마크 추가(최상단)");
            let out = format!("v{}", label_seq);
            parts.push(format!(
                "[{}]drawtext=text='LoLShorts Free Tier':fontsize=36:fontcolor=white@0.5:x=w-tw-20:y=h-th-20:shadowx=2:shadowy=2[{}]",
                current, out
            ));
            current = out;
        }

        // 적용할 필터가 전혀 없으면(예: is_pro + 존재하지 않는 이미지 배경 + 요소 없음)
        // 원본을 그대로 반환.
        if current == "0:v" {
            info!("적용할 캔버스 필터가 없음 — 원본 사용");
            return Ok(video_path.to_path_buf());
        }

        let filter_complex = parts.join(";");
        let map_label = format!("[{}]", current);
        let ffmpeg_path = get_ffmpeg_path().map_err(|e| VideoError::ProcessingError {
            message: format!("FFmpeg를 찾을 수 없음: {}", e),
        })?;

        let mut command = tokio::process::Command::new(ffmpeg_path);
        command.arg("-i");
        command.arg(
            video_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: video_path.display().to_string(),
                })?,
        );
        command.arg("-filter_complex");
        command.arg(&filter_complex);
        // 합성된 비디오 + 원본 오디오(있으면)를 명시적으로 매핑.
        command.arg("-map");
        command.arg(&map_label);
        command.arg("-map");
        command.arg("0:a?");
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
        ]);
        command.arg(
            output_path
                .to_str()
                .ok_or_else(|| VideoError::FileAccessError {
                    path: output_path.display().to_string(),
                })?,
        );

        execute_ffmpeg_command(&mut command).await.map_err(|e| {
            VideoError::CanvasApplicationError {
                reason: e.to_string(),
            }
        })?;

        info!("캔버스 오버레이 적용 완료");
        Ok(output_path)
    }

    pub(super) async fn apply_watermark_only(&self, video_path: &Path) -> Result<PathBuf> {
        let output_dir = self.stage_dir();
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
        let output_dir = self.stage_dir();
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

/// 캔버스 배경 레이어를 filter_complex 에 `[bg]` 라벨로 추가한다.
///
/// 핵심: 배경을 **비디오 길이만큼** 생성해(color/gradient=d=비디오길이, image=무한 루프)
/// 그 위에 게임 비디오를 overlay 할 때 shortest=1 로 비디오 길이에 맞춰 컷 되도록 한다.
/// (과거 버그: `d=1` + `overlay=shortest=1` 로 결과가 ~1초로 잘리고, 배경이 게임 위에
///  올라가 게임 화면이 가려졌음.)
///
/// 배경을 만들 수 없으면(존재하지 않는 이미지 등) false 를 반환한다.
fn build_background_layer(
    parts: &mut Vec<String>,
    background: &BackgroundLayer,
    width: u32,
    height: u32,
    video_duration: f64,
) -> bool {
    // color/gradient 소스 지속시간: 비디오보다 약간 길게(overlay shortest 가 컷).
    // 실측 실패(0.0) 시 넉넉한 상한으로 폴백.
    let dur = if video_duration > 0.0 {
        video_duration + 1.0
    } else {
        3600.0
    };

    match background {
        BackgroundLayer::Color { value } => {
            // r=60: lavfi `color` 소스는 미지정 시 기본 25fps라, 이 위에
            // overlay 되는 60fps 게임 영상이 배경 프레임레이트로 저하된다.
            parts.push(format!(
                "color=c={}:s={}x{}:d={:.3}:r=60[bg]",
                value, width, height, dur
            ));
            true
        }
        BackgroundLayer::Gradient { value } => {
            // 게임 비디오가 풀프레임으로 덮으므로 배경은 사실상 베이스 레이어 역할.
            // gradients 필터 미가용 환경까지 안전하도록 첫 색상의 단색으로 근사한다.
            let base = value
                .split(':')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("black");
            // r=60: 위 color 케이스와 동일한 이유로 명시.
            parts.push(format!(
                "color=c={}:s={}x{}:d={:.3}:r=60[bg]",
                base, width, height, dur
            ));
            true
        }
        BackgroundLayer::Image { path } => {
            let bg_path = PathBuf::from(path);
            if !bg_path.exists() {
                warn!("배경 이미지를 찾을 수 없음: {}", path);
                return false;
            }
            let safe_path = path.replace('\\', "\\\\").replace(':', "\\:");
            // 단일 프레임 이미지를 무한 반복 후 30fps 스트림으로 만들어 비디오 길이만큼 유지.
            // overlay=shortest=1 이 비디오 길이에 맞춰 컷 한다.
            parts.push(format!(
                "movie={}[bgsrc];[bgsrc]scale={}:{}:force_original_aspect_ratio=increase,crop={}:{},boxblur=20,loop=loop=-1:size=1,fps=30[bg]",
                safe_path, width, height, width, height
            ));
            true
        }
    }
}

/// Windows 시스템 폰트 디렉터리.
const WINDOWS_FONTS_DIR: &str = r"C:\Windows\Fonts";

/// 캔버스 텍스트 drawtext 의 폰트 절(font clause)을 만든다.
///
/// 프론트는 폰트 "이름"(예: `CanvasControlsPanel.tsx` 의 `font: 'Arial'`)을 보내는데
/// ffmpeg drawtext 의 `fontfile=` 옵션은 파일 경로 전용이라, 이름을 그대로 넣으면
/// fontconfig 폴백으로 조용히 성공(exit 0)하지만 한글 글리프가 없는 폰트로 떨어져
/// 텍스트가 빈 네모(tofu)로 렌더링된다.
/// (픽셀로 검증된 정상 동작: `fontfile='C\:/Windows/Fonts/malgun.ttf'` — 작은따옴표로
/// 감싸고 콜론을 `\:` 로 이스케이프해야 한다.)
///
/// 우선순위:
/// 1. `font` 자체가 실존하는 폰트 파일 경로(.ttf/.otf/.ttc)면 그 경로를 사용.
/// 2. 알려진 폰트 이름이면 Windows 폰트 디렉터리의 실제 파일로 매핑.
/// 3. 매핑 실패 또는 파일 미존재 → 맑은 고딕(malgun.ttf)으로 폴백(한글 글리프
///    커버리지가 기본 요구사항).
/// 4. malgun.ttf 조차 없으면(비 Windows 환경 등) `font=<이름>` (fontconfig) 최후 폴백.
pub(super) fn resolve_font_clause(font: &str) -> String {
    let trimmed = font.trim();

    // 1) 폰트 "경로"가 직접 온 경우: 확장자 확인 + 실존 확인.
    let looks_like_font_path = trimmed
        .rsplit('.')
        .next()
        .map(|ext| matches!(ext.to_lowercase().as_str(), "ttf" | "otf" | "ttc"))
        .unwrap_or(false);

    if looks_like_font_path {
        if PathBuf::from(trimmed).exists() {
            return fontfile_clause(trimmed);
        }
        warn!("지정된 폰트 파일을 찾을 수 없어 폴백 진행: {}", trimmed);
    }

    // 2) 알려진 폰트 이름 → Windows 폰트 디렉터리의 실제 파일명 매핑.
    match map_known_font_name(trimmed) {
        Some(file_name) => {
            let full_path = format!("{}\\{}", WINDOWS_FONTS_DIR, file_name);
            if PathBuf::from(&full_path).exists() {
                return fontfile_clause(&full_path);
            }
            warn!(
                "매핑된 폰트 파일이 없어 malgun.ttf 로 폴백: {} -> {}",
                trimmed, full_path
            );
        }
        None => {
            warn!("알 수 없는 폰트 이름, malgun.ttf 로 폴백: {}", trimmed);
        }
    }

    // 3) 맑은 고딕 폴백(한글 글리프 커버리지 기본 요구사항).
    let malgun_path = format!("{}\\malgun.ttf", WINDOWS_FONTS_DIR);
    if PathBuf::from(&malgun_path).exists() {
        return fontfile_clause(&malgun_path);
    }

    // 4) 최후 폴백: fontconfig 이름 매칭(비 Windows 환경 등, malgun.ttf 조차 없을 때).
    warn!(
        "malgun.ttf 도 찾을 수 없어 fontconfig 이름 폴백 사용: {}",
        trimmed
    );
    format!("font={}", escape_ffmpeg_text(trimmed))
}

/// 알려진 폰트 "이름"을 Windows 폰트 디렉터리의 실제 파일명으로 매핑한다.
/// 대소문자 무관, 영문/한글 별칭 모두 인식.
fn map_known_font_name(name: &str) -> Option<&'static str> {
    match name.trim().to_lowercase().as_str() {
        "arial" => Some("arial.ttf"),
        "malgun gothic" | "malgungothic" | "맑은 고딕" | "맑은고딕" => Some("malgun.ttf"),
        "gulim" | "굴림" => Some("gulim.ttc"),
        "batang" | "바탕" => Some("batang.ttc"),
        "dotum" | "돋움" => Some("dotum.ttc"),
        "gungsuh" | "궁서" => Some("gungsuh.ttc"),
        _ => None,
    }
}

/// 폰트 파일 경로를 drawtext `fontfile=` 절로 변환한다:
/// 역슬래시 → 슬래시 정규화 후 콜론을 이스케이프하고 작은따옴표로 감싼다.
fn fontfile_clause(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let escaped = normalized.replace(':', "\\:");
    format!("fontfile='{}'", escaped)
}

#[cfg(test)]
mod resolve_font_clause_tests {
    use super::*;

    #[test]
    fn fontfile_clause_escapes_colon_and_normalizes_backslashes() {
        let clause = fontfile_clause(r"C:\Windows\Fonts\malgun.ttf");
        assert_eq!(clause, r"fontfile='C\:/Windows/Fonts/malgun.ttf'");
    }

    #[test]
    fn fontfile_clause_escapes_colon_when_already_forward_slash() {
        let clause = fontfile_clause("C:/Windows/Fonts/malgun.ttf");
        assert_eq!(clause, r"fontfile='C\:/Windows/Fonts/malgun.ttf'");
    }

    #[test]
    fn maps_known_font_names_case_insensitively() {
        assert_eq!(map_known_font_name("Arial"), Some("arial.ttf"));
        assert_eq!(map_known_font_name("arial"), Some("arial.ttf"));
        assert_eq!(map_known_font_name("Malgun Gothic"), Some("malgun.ttf"));
        assert_eq!(map_known_font_name("맑은 고딕"), Some("malgun.ttf"));
        assert_eq!(map_known_font_name("Gulim"), Some("gulim.ttc"));
        assert_eq!(map_known_font_name("굴림"), Some("gulim.ttc"));
        assert_eq!(map_known_font_name("바탕"), Some("batang.ttc"));
    }

    #[test]
    fn unknown_font_name_maps_to_none() {
        assert_eq!(map_known_font_name("SomeRandomFontThatDoesNotExist"), None);
    }

    #[test]
    fn resolve_font_clause_always_returns_fontfile_or_fontconfig_form() {
        // 환경에 따라 arial.ttf 유무가 갈릴 수 있으므로 어떤 경로든 유효한
        // drawtext 절(fontfile= 또는 font=)을 반환하는지만 검증한다.
        let clause = resolve_font_clause("Arial");
        assert!(
            clause.starts_with("fontfile=") || clause.starts_with("font="),
            "unexpected clause: {}",
            clause
        );
    }

    #[test]
    fn resolve_font_clause_unknown_name_falls_back_to_malgun_or_fontconfig() {
        let clause = resolve_font_clause("존재하지않는폰트이름12345");
        assert!(
            clause.contains("malgun.ttf") || clause.starts_with("font="),
            "expected malgun fallback or fontconfig fallback, got: {}",
            clause
        );
    }

    #[test]
    fn resolve_font_clause_direct_path_used_when_file_exists() {
        // 실제 Windows 환경(테스트 실행 머신)에 malgun.ttf 가 있으면 경로 그대로
        // fontfile= 절로 변환되는지 확인, 없는 환경(CI 등)에서는 스킵.
        let path = format!("{}\\malgun.ttf", WINDOWS_FONTS_DIR);
        if PathBuf::from(&path).exists() {
            let clause = resolve_font_clause(&path);
            assert_eq!(clause, r"fontfile='C\:/Windows/Fonts/malgun.ttf'");
        }
    }
}
