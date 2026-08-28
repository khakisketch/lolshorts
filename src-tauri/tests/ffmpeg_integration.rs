#[cfg(test)]
mod tests {
    use lolshorts::utils::ffmpeg::{get_ffmpeg_path, get_ffprobe_path};
    use lolshorts::video::processor::types::{ClipSpec, ComposeOptions, VerticalFraming};
    use lolshorts::video::VideoProcessor;
    use std::path::Path;
    use std::process::{Command, Output};
    use tempfile::TempDir;

    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 568;
    const DURATION_SECS: f64 = 1.2;

    fn available_media_tools() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        let ffmpeg = get_ffmpeg_path().ok()?;
        let ffprobe = get_ffprobe_path().ok()?;
        for tool in [&ffmpeg, &ffprobe] {
            if !Command::new(tool)
                .arg("-version")
                .output()
                .is_ok_and(|output| output.status.success())
            {
                return None;
            }
        }
        Some((ffmpeg, ffprobe))
    }

    fn assert_success(output: &Output, description: &str) {
        assert!(
            output.status.success(),
            "{description} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn compose_options(framing: VerticalFraming) -> ComposeOptions {
        ComposeOptions {
            width: WIDTH,
            height: HEIGHT,
            transition: None,
            event_times: None,
            fps: Some(30),
            normalize_audio: None,
            captions: None,
            framing,
        }
    }

    fn pixel_rgb_at(ffmpeg: &Path, video: &Path, time: f64, x: u32, y: u32) -> [u8; 3] {
        let seek = format!("{time:.3}");
        let output = Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-ss",
                &seek,
                "-i",
                video.to_str().expect("test path must be UTF-8"),
                "-vf",
                &format!("crop=1:1:{x}:{y},format=rgb24"),
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-",
            ])
            .output()
            .expect("ffmpeg pixel probe should start");
        assert_success(&output, "ffmpeg pixel probe");
        assert_eq!(output.stdout.len(), 3, "pixel probe should return RGB24");
        [output.stdout[0], output.stdout[1], output.stdout[2]]
    }

    fn pixel_rgb(ffmpeg: &Path, video: &Path, x: u32, y: u32) -> [u8; 3] {
        pixel_rgb_at(ffmpeg, video, 0.5, x, y)
    }

    fn assert_dominant(pixel: [u8; 3], channel: usize, label: &str) {
        assert!(
            pixel[channel] > 120
                && pixel[channel] > pixel[(channel + 1) % 3] + 35
                && pixel[channel] > pixel[(channel + 2) % 3] + 35,
            "expected {label}-dominant pixel, got {pixel:?}"
        );
    }

    fn assert_yellow(pixel: [u8; 3], label: &str) {
        assert!(
            pixel[0] > 150 && pixel[1] > 150 && pixel[2] < 110,
            "expected {label} pixel, got {pixel:?}"
        );
    }

    struct MediaContract {
        duration: f64,
        has_audio: bool,
        audio_sample_rate: Option<u32>,
        audio_channels: Option<u32>,
        stream_duration_difference: Option<f64>,
    }

    fn media_contract(ffprobe: &Path, video: &Path) -> MediaContract {
        let output = Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration:stream=codec_type,duration,sample_rate,channels",
                "-of",
                "json",
                video.to_str().expect("test path must be UTF-8"),
            ])
            .output()
            .expect("ffprobe should start");
        assert_success(&output, "ffprobe output contract");
        let probe: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("ffprobe JSON");
        let duration = probe["format"]["duration"]
            .as_str()
            .expect("duration string")
            .parse::<f64>()
            .expect("duration number");
        let streams = probe["streams"].as_array().expect("streams array");
        let video_duration = streams
            .iter()
            .find(|stream| stream["codec_type"] == "video")
            .and_then(|stream| stream["duration"].as_str())
            .and_then(|value| value.parse::<f64>().ok());
        let audio = streams
            .iter()
            .find(|stream| stream["codec_type"] == "audio");
        let audio_duration = audio
            .and_then(|stream| stream["duration"].as_str())
            .and_then(|value| value.parse::<f64>().ok());
        MediaContract {
            duration,
            has_audio: audio.is_some(),
            audio_sample_rate: audio
                .and_then(|stream| stream["sample_rate"].as_str())
                .and_then(|value| value.parse::<u32>().ok()),
            audio_channels: audio
                .and_then(|stream| stream["channels"].as_u64())
                .map(|value| value as u32),
            stream_duration_difference: video_duration
                .zip(audio_duration)
                .map(|(video, audio)| (video - audio).abs()),
        }
    }

    fn decoded_audio_metrics(ffmpeg: &Path, video: &Path) -> (f64, f64) {
        let output = Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-i",
                video.to_str().expect("test path must be UTF-8"),
                "-map",
                "0:a:0",
                "-ac",
                "1",
                "-ar",
                "48000",
                "-f",
                "f32le",
                "-",
            ])
            .output()
            .expect("ffmpeg audio decode should start");
        assert_success(&output, "ffmpeg audio decode");
        let samples: Vec<f32> = output
            .stdout
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte sample")))
            .collect();
        assert!(!samples.is_empty(), "decoded audio must contain samples");
        let rms = (samples
            .iter()
            .map(|sample| f64::from(*sample).powi(2))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt();
        let zero_crossings = samples
            .windows(2)
            .filter(|pair| pair[0].is_sign_positive() != pair[1].is_sign_positive())
            .count();
        let frequency = zero_crossings as f64 * 48_000.0 / (2.0 * samples.len() as f64);
        (rms, frequency)
    }

    #[tokio::test]
    async fn compose_with_options_real_media_regression() {
        let Some((ffmpeg, ffprobe)) = available_media_tools() else {
            eprintln!("Skipping real-media regression: usable ffmpeg and ffprobe are unavailable");
            return;
        };
        let temp_dir = TempDir::new().expect("test TempDir");
        let source = temp_dir.path().join("marked_with_audio.mp4");
        let silent_source = temp_dir.path().join("silent.mp4");
        let vfr_source = temp_dir.path().join("vfr.mp4");

        let output = Command::new(&ffmpeg)
                .args([
                    "-f", "lavfi", "-i", "color=c=red:size=320x180:rate=30",
                    "-f", "lavfi", "-i", "sine=frequency=880:sample_rate=48000",
                    "-t", "1.2", "-vf", "drawbox=x=0:y=0:w=320:h=90:color=blue:t=fill,drawbox=x=8:y=8:w=24:h=24:color=yellow:t=fill",
                    "-c:v", "libx264", "-c:a", "aac", "-pix_fmt", "yuv420p", "-y",
                    source.to_str().expect("test path must be UTF-8"),
                ])
                .output()
                .expect("fixture ffmpeg should start");
        assert_success(&output, "audio fixture generation");
        let output = Command::new(&ffmpeg)
            .args([
                "-f",
                "lavfi",
                "-i",
                "color=c=green:size=320x180:rate=30",
                "-t",
                "1.2",
                "-c:v",
                "libx264",
                "-an",
                "-pix_fmt",
                "yuv420p",
                "-y",
                silent_source.to_str().expect("test path must be UTF-8"),
            ])
            .output()
            .expect("silent fixture ffmpeg should start");
        assert_success(&output, "silent fixture generation");
        let output = Command::new(&ffmpeg)
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=30",
                "-t",
                "1.2",
                "-vf",
                "select='not(mod(n\\,3))'",
                "-fps_mode",
                "vfr",
                "-an",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-y",
                vfr_source.to_str().expect("test path must be UTF-8"),
            ])
            .output()
            .expect("VFR fixture ffmpeg should start");
        assert_success(&output, "VFR fixture generation");

        let processor =
            VideoProcessor::new_with_software_h264().expect("ffmpeg was already verified above");
        let specs = [ClipSpec {
            path: source.clone(),
            trim_start: None,
            trim_duration: None,
        }];
        let safe_output = temp_dir.path().join("safe.mp4");
        processor
            .compose_with_options(
                &specs,
                &safe_output,
                &compose_options(VerticalFraming::SafeFullFrame),
            )
            .await
            .expect("SafeFullFrame composition should succeed");
        let contract = media_contract(&ffprobe, &safe_output);
        assert!(
            (contract.duration - DURATION_SECS).abs() < 0.18,
            "unexpected duration: {}",
            contract.duration
        );
        assert!(
            contract.has_audio,
            "audio-bearing source must yield an audio stream"
        );
        assert_eq!(contract.audio_sample_rate, Some(48_000));
        assert_eq!(contract.audio_channels, Some(2));
        assert!(contract
            .stream_duration_difference
            .is_some_and(|difference| difference <= 0.25));
        let (rms, frequency) = decoded_audio_metrics(&ffmpeg, &safe_output);
        assert!(rms > 0.02, "tone RMS is unexpectedly low: {rms}");
        assert!(
            (frequency - 880.0).abs() < 35.0,
            "unexpected decoded tone frequency: {frequency}Hz"
        );
        assert_yellow(
            pixel_rgb(&ffmpeg, &safe_output, 10, 204),
            "blue full-frame marker",
        );
        assert_dominant(
            pixel_rgb(&ffmpeg, &safe_output, 10, 10),
            2,
            "blue blurred letterbox fill",
        );

        let focus_output = temp_dir.path().join("focus.mp4");
        processor
            .compose_with_options(
                &specs,
                &focus_output,
                &compose_options(VerticalFraming::LolFocusStack),
            )
            .await
            .expect("LolFocusStack composition should succeed");
        assert_dominant(
            pixel_rgb(&ffmpeg, &focus_output, 160, 104),
            2,
            "full HUD region",
        );
        assert_dominant(
            pixel_rgb(&ffmpeg, &focus_output, 160, 300),
            2,
            "upper focus region",
        );
        assert_dominant(
            pixel_rgb(&ffmpeg, &focus_output, 160, 500),
            0,
            "lower focus region",
        );

        let silent_output = temp_dir.path().join("silent_composed.mp4");
        processor
            .compose_with_options(
                &[ClipSpec {
                    path: silent_source,
                    trim_start: None,
                    trim_duration: None,
                }],
                &silent_output,
                &compose_options(VerticalFraming::SafeFullFrame),
            )
            .await
            .expect("silent source should receive bounded synthesized audio");
        let silent_contract = media_contract(&ffprobe, &silent_output);
        assert!((silent_contract.duration - DURATION_SECS).abs() < 0.18);
        assert!(
            silent_contract.has_audio,
            "silent source must receive synthesized audio"
        );
        assert_eq!(silent_contract.audio_sample_rate, Some(48_000));
        assert_eq!(silent_contract.audio_channels, Some(2));
        let (silent_rms, _) = decoded_audio_metrics(&ffmpeg, &silent_output);
        assert!(
            silent_rms < 0.001,
            "synthesized silence is too loud: {silent_rms}"
        );

        let vfr_output = temp_dir.path().join("vfr_composed.mp4");
        processor
            .compose_with_options(
                &[ClipSpec {
                    path: vfr_source,
                    trim_start: None,
                    trim_duration: None,
                }],
                &vfr_output,
                &compose_options(VerticalFraming::SafeFullFrame),
            )
            .await
            .expect("VFR source should compose successfully");
        assert!(media_contract(&ffprobe, &vfr_output).duration > 0.8);

        let mut storyboard_specs = Vec::new();
        for (index, (color, frequency)) in [("red", 440), ("green", 660), ("blue", 880)]
            .into_iter()
            .enumerate()
        {
            let scene = temp_dir.path().join(format!("scene-{index}.mp4"));
            let tone = format!("sine=frequency={frequency}:sample_rate=48000");
            let color_source = format!("color=c={color}:size=320x180:rate=30");
            let output = Command::new(&ffmpeg)
                .args([
                    "-f",
                    "lavfi",
                    "-i",
                    &color_source,
                    "-f",
                    "lavfi",
                    "-i",
                    &tone,
                    "-t",
                    "1.0",
                    "-c:v",
                    "libx264",
                    "-c:a",
                    "aac",
                    "-pix_fmt",
                    "yuv420p",
                    "-y",
                    scene.to_str().expect("test path must be UTF-8"),
                ])
                .output()
                .expect("storyboard fixture ffmpeg should start");
            assert_success(&output, "storyboard fixture generation");
            storyboard_specs.push(ClipSpec {
                path: scene,
                trim_start: Some(0.1),
                trim_duration: Some(0.5),
            });
        }
        let storyboard_output = temp_dir.path().join("storyboard.mp4");
        processor
            .compose_with_options(
                &storyboard_specs,
                &storyboard_output,
                &compose_options(VerticalFraming::SafeFullFrame),
            )
            .await
            .expect("three-scene storyboard should compose");
        let storyboard_contract = media_contract(&ffprobe, &storyboard_output);
        assert!(
            (storyboard_contract.duration - 1.5).abs() <= 0.25,
            "storyboard trim/boundary duration drifted: {}",
            storyboard_contract.duration
        );
        assert_dominant(
            pixel_rgb_at(&ffmpeg, &storyboard_output, 0.25, 160, 284),
            0,
            "first storyboard scene",
        );
        assert_dominant(
            pixel_rgb_at(&ffmpeg, &storyboard_output, 0.75, 160, 284),
            1,
            "second storyboard scene",
        );
        assert_dominant(
            pixel_rgb_at(&ffmpeg, &storyboard_output, 1.25, 160, 284),
            2,
            "third storyboard scene",
        );

        let truncated = temp_dir.path().join("truncated.mp4");
        std::fs::write(&truncated, b"not an MP4").expect("truncated fixture");
        assert!(
            processor
                .compose_with_options(
                    &[ClipSpec {
                        path: truncated,
                        trim_start: None,
                        trim_duration: None
                    }],
                    &temp_dir.path().join("should_not_exist.mp4"),
                    &compose_options(VerticalFraming::SafeFullFrame),
                )
                .await
                .is_err(),
            "truncated media must be rejected"
        );
    }
}
