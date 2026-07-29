#[cfg(test)]
mod tests {
    use lolshorts::utils::ffmpeg::get_ffmpeg_path;
    use lolshorts::video::VideoProcessor;
    use std::process::Command;
    use tokio::fs;

    #[tokio::test]
    async fn test_full_video_pipeline() {
        // 1. Initialize Processor
        // Using new_with_fallback to ensure we get a working processor even if hardware accel fails
        let processor = VideoProcessor::new_with_fallback();

        // Use a temp dir for artifacts
        let temp_dir = std::env::temp_dir().join("lolshorts_test_artifacts");
        if !temp_dir.exists() {
            fs::create_dir_all(&temp_dir).await.unwrap();
        }

        let input_video = temp_dir.join("input_source.mp4");
        let extracted_clip = temp_dir.join("extracted_clip.mp4");
        let composed_short = temp_dir.join("composed_short.mp4");
        let thumbnail = temp_dir.join("thumbnail.jpg");

        // 2. Generate Test Video
        let absolute_ffmpeg_path =
            get_ffmpeg_path().expect("A usable FFmpeg binary is required for ffmpeg_integration");

        println!("Using FFmpeg at: {:?}", absolute_ffmpeg_path);

        // Verify FFmpeg works
        let version_output = Command::new(&absolute_ffmpeg_path)
            .arg("-version")
            .output()
            .expect("Failed to run ffmpeg -version");

        if !version_output.status.success() {
            println!("FFmpeg version check failed!");
            println!(
                "Stdout: {}",
                String::from_utf8_lossy(&version_output.stdout)
            );
            println!(
                "Stderr: {}",
                String::from_utf8_lossy(&version_output.stderr)
            );
            panic!("Cannot run ffmpeg binary");
        } else {
            println!("FFmpeg version check passed.");
        }

        println!("Generating test video...");
        let output = Command::new(&absolute_ffmpeg_path)
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=5:size=1280x720:rate=30",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-y",
                input_video.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to run ffmpeg command");

        if !output.status.success() {
            println!("FFmpeg generation failed!");
            println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
            println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
            panic!("FFmpeg failed to generate test video");
        }

        assert!(input_video.exists(), "Test video file was not created");

        // 3. Test Extract Clip (1.0s to 3.0s)
        println!("Testing extract_clip...");
        let result = processor
            .extract_clip(
                &input_video,
                &extracted_clip,
                1.0,
                2.0, // duration
            )
            .await;

        if let Err(e) = &result {
            println!("Extract Clip Error: {:?}", e);
        }
        assert!(result.is_ok(), "extract_clip failed");
        assert!(extracted_clip.exists(), "Extracted clip file missing");

        // 4. Test Compose Shorts (9:16)
        println!("Testing compose_shorts...");
        let clips = vec![extracted_clip.clone()];
        let result = processor
            .compose_shorts(&clips, &composed_short, 1080, 1920)
            .await;

        if let Err(e) = &result {
            println!("Compose Shorts Error: {:?}", e);
        }
        assert!(result.is_ok(), "compose_shorts failed");
        assert!(composed_short.exists(), "Composed short file missing");

        // 5. Test Thumbnail
        println!("Testing generate_thumbnail...");
        let result = processor
            .generate_thumbnail(&input_video, &thumbnail, 1.5)
            .await;

        if let Err(e) = &result {
            println!("Thumbnail Error: {:?}", e);
        }
        assert!(result.is_ok(), "generate_thumbnail failed");
        assert!(thumbnail.exists(), "Thumbnail file missing");

        // Cleanup
        // let _ = fs::remove_dir_all(temp_dir).await;
    }
}
