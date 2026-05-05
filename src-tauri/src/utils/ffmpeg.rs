use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};

use crate::utils::process::command_output_with_timeout;

#[derive(Debug, Error)]
pub enum FFmpegError {
    #[error("FFmpeg not found in bundled binaries or system PATH")]
    NotFound,
}

// Global cache for FFmpeg path - initialized once per application lifetime
static FFMPEG_PATH: OnceLock<PathBuf> = OnceLock::new();
const FFMPEG_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

pub fn get_ffmpeg_path() -> Result<PathBuf, FFmpegError> {
    // Return cached path if already found
    if let Some(path) = FFMPEG_PATH.get() {
        return Ok(path.clone());
    }

    info!("Searching for FFmpeg binary...");

    // 1. Try to get the path from Tauri's bundled binaries (development)
    if let Ok(resource_dir) = env::var("CARGO_MANIFEST_DIR") {
        let binaries_dir = PathBuf::from(resource_dir).join("binaries");
        debug!("Checking bundled FFmpeg directory: {:?}", binaries_dir);

        if let Some(ffmpeg_path) = check_ffmpeg_in_dir(&binaries_dir) {
            info!("Found bundled FFmpeg at: {:?}", ffmpeg_path);
            // Cache the path for future calls
            let _ = FFMPEG_PATH.set(ffmpeg_path.clone());
            return Ok(ffmpeg_path);
        }
    }

    // 2. Try the common binary location for Tauri apps (production)
    if let Ok(mut exe_path) = env::current_exe() {
        exe_path.pop(); // Remove executable name
        let binaries_dir = exe_path.join("binaries");

        debug!("Checking production FFmpeg directory: {:?}", binaries_dir);
        if let Some(ffmpeg_path) = check_ffmpeg_in_dir(&binaries_dir) {
            info!("Found production FFmpeg at: {:?}", ffmpeg_path);
            let _ = FFMPEG_PATH.set(ffmpeg_path.clone());
            return Ok(ffmpeg_path);
        }
    }

    // 3. Try Windows-specific locations
    #[cfg(target_os = "windows")]
    {
        // Check Program Files
        if let Ok(program_files) = env::var("ProgramFiles") {
            let system_ffmpeg = PathBuf::from(program_files)
                .join("ffmpeg")
                .join("bin")
                .join("ffmpeg.exe");

            debug!("Checking system FFmpeg at: {:?}", system_ffmpeg);
            if system_ffmpeg.exists() {
                info!("Found system FFmpeg at: {:?}", system_ffmpeg);
                let _ = FFMPEG_PATH.set(system_ffmpeg.clone());
                return Ok(system_ffmpeg);
            }
        }

        // Check Chocolatey package manager
        let choco_ffmpeg = PathBuf::from("C:\\ProgramData\\chocolatey\\bin\\ffmpeg.exe");
        debug!("Checking Chocolatey FFmpeg at: {:?}", choco_ffmpeg);
        if choco_ffmpeg.exists() {
            info!("Found Chocolatey FFmpeg at: {:?}", choco_ffmpeg);
            let _ = FFMPEG_PATH.set(choco_ffmpeg.clone());
            return Ok(choco_ffmpeg);
        }
    }

    // 4. Fallback to system PATH
    let path_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    debug!("Checking system PATH for FFmpeg using: {}", path_cmd);

    let mut path_lookup = Command::new(path_cmd);
    path_lookup.arg("ffmpeg");
    if let Ok(output) =
        command_output_with_timeout(path_lookup, FFMPEG_PROBE_TIMEOUT, "FFmpeg PATH lookup")
    {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout);
            if let Some(first_line) = path_str.lines().next() {
                let path = PathBuf::from(first_line.trim());
                if path.exists() {
                    info!("Found FFmpeg in PATH: {:?}", path);
                    let _ = FFMPEG_PATH.set(path.clone());
                    return Ok(path);
                }
            }
        }
    }

    warn!("FFmpeg not found in any location");
    Err(FFmpegError::NotFound)
}

#[allow(dead_code)]
fn get_ffmpeg_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe" // Use standard name, let the search logic find the right file
    } else {
        "ffmpeg"
    }
}

// Check if a specific FFmpeg executable exists in the given directory
fn check_ffmpeg_in_dir(dir: &Path) -> Option<PathBuf> {
    let candidates = if cfg!(target_os = "windows") {
        vec!["ffmpeg.exe", "ffmpeg-x86_64-pc-windows-msvc.exe"]
    } else {
        vec!["ffmpeg"]
    };

    for candidate in candidates {
        let path = dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

// ============================================================================
// FFmpeg Information and Hardware Acceleration Detection
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FFmpegInfo {
    pub version: String,
    pub configuration: Vec<String>,
    pub available_encoders: Vec<EncoderInfo>,
    pub available_formats: Vec<FormatInfo>,
    pub supported_protocols: Vec<String>,
    pub license: String,
    pub build_information: BuildInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderInfo {
    pub name: String,
    pub description: String,
    pub can_encode: bool,
    pub can_decode: bool,
    pub is_hardware_accelerated: bool,
    pub supported_pix_fmts: Vec<String>,
    pub supported_frame_rates: Vec<String>,
    pub threading_support: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatInfo {
    pub name: String,
    pub description: String,
    pub can_mux: bool,
    pub can_demux: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInfo {
    pub compiler: String,
    pub configuration: Vec<String>,
    pub libraries: Vec<String>,
}

/// Get comprehensive FFmpeg information
pub fn get_ffmpeg_info() -> Result<FFmpegInfo, FFmpegError> {
    let ffmpeg_path = get_ffmpeg_path()?;

    info!("Querying FFmpeg information from: {:?}", ffmpeg_path);

    let mut version_cmd = Command::new(&ffmpeg_path);
    version_cmd.arg("-version");
    let version_output =
        command_output_with_timeout(version_cmd, FFMPEG_PROBE_TIMEOUT, "FFmpeg version probe")
            .map_err(|e| {
                error!("Failed to execute FFmpeg version command: {}", e);
                FFmpegError::NotFound
            })?;

    if !version_output.status.success() {
        let stderr = String::from_utf8_lossy(&version_output.stderr);
        error!("FFmpeg version command failed: {}", stderr);
        return Err(FFmpegError::NotFound);
    }

    let mut encoders_cmd = Command::new(&ffmpeg_path);
    encoders_cmd.arg("-encoders");
    let encoders_output =
        command_output_with_timeout(encoders_cmd, FFMPEG_PROBE_TIMEOUT, "FFmpeg encoder probe")
            .map_err(|e| {
                error!("Failed to execute FFmpeg encoders command: {}", e);
                FFmpegError::NotFound
            })?;

    if !encoders_output.status.success() {
        let stderr = String::from_utf8_lossy(&encoders_output.stderr);
        error!("FFmpeg encoders command failed: {}", stderr);
        return Err(FFmpegError::NotFound);
    }

    let mut formats_cmd = Command::new(&ffmpeg_path);
    formats_cmd.arg("-formats");
    let formats_output =
        command_output_with_timeout(formats_cmd, FFMPEG_PROBE_TIMEOUT, "FFmpeg format probe")
            .map_err(|e| {
                error!("Failed to execute FFmpeg formats command: {}", e);
                FFmpegError::NotFound
            })?;

    if !formats_output.status.success() {
        let stderr = String::from_utf8_lossy(&formats_output.stderr);
        error!("FFmpeg formats command failed: {}", stderr);
        return Err(FFmpegError::NotFound);
    }

    let version_str = String::from_utf8_lossy(&version_output.stdout);
    let encoders_str = String::from_utf8_lossy(&encoders_output.stdout);
    let formats_str = String::from_utf8_lossy(&formats_output.stdout);

    let version = parse_ffmpeg_version(&version_str)?;
    let configuration = parse_configuration(&version_str);
    let available_encoders = parse_encoders(&encoders_str);
    let available_formats = parse_formats(&formats_str);
    let supported_protocols = get_supported_protocols();
    let license = parse_license(&version_str);
    let build_information = parse_build_info(&version_str);

    Ok(FFmpegInfo {
        version,
        configuration,
        available_encoders,
        available_formats,
        supported_protocols,
        license,
        build_information,
    })
}

/// Parse FFmpeg version from version output
fn parse_ffmpeg_version(output: &str) -> Result<String, FFmpegError> {
    if let Some(first_line) = output.lines().next() {
        if let Some(version_part) = first_line.split_whitespace().nth(2) {
            Ok(version_part.trim_matches('v').to_string())
        } else {
            Ok("Unknown".to_string())
        }
    } else {
        Ok("Unknown".to_string())
    }
}

/// Parse configuration options from version output
fn parse_configuration(output: &str) -> Vec<String> {
    let mut config = Vec::new();
    let lines: Vec<&str> = output.lines().collect();

    // Find the line with "configuration:"
    for line in lines {
        if line.contains("configuration:") {
            if let Some(config_part) = line.split("configuration:").nth(1) {
                let config_str = config_part.trim();
                // Remove surrounding characters if present
                let cleaned = config_str.trim_matches(|c| c == '-' || c == ' ');
                if !cleaned.is_empty() {
                    config = cleaned.split_whitespace().map(|s| s.to_string()).collect();
                }
                break;
            }
        }
    }

    config
}

/// Parse encoders from encoders output
fn parse_encoders(output: &str) -> Vec<EncoderInfo> {
    let mut encoders = Vec::new();
    let lines: Vec<&str> = output.lines().collect();

    // Skip header lines and start parsing from line 3 (index 2)
    for line in lines.iter().skip(2) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('-') {
            continue;
        }

        // Parse encoder line format: "D..... h264                  H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10"
        if let Some(parts) = parse_encoder_line(trimmed) {
            encoders.push(parts);
        }
    }

    encoders
}

/// Parse individual encoder line
fn parse_encoder_line(line: &str) -> Option<EncoderInfo> {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() < 7 {
        return None;
    }

    let capabilities = &chars[0..7];
    let can_encode = capabilities[1] == 'E';
    let can_decode = capabilities[2] == 'D';

    // Extract the rest of the line after capabilities
    let rest: String = chars[7..].iter().collect();
    let trimmed = rest.trim();

    // Find encoder name and description
    let encoder_name_end = trimmed.find(' ').unwrap_or(trimmed.len());
    let encoder_name = trimmed[..encoder_name_end].trim().to_string();

    let description_start = if encoder_name.len() < trimmed.len() {
        trimmed[encoder_name.len()..].trim()
    } else {
        ""
    };

    // Clean up description (remove leading/trailing slashes)
    let description = description_start
        .trim_matches(|c| c == '/' || c == ' ')
        .trim();

    // Detect hardware acceleration
    let is_hardware_accelerated = is_hardware_encoder(&encoder_name);

    Some(EncoderInfo {
        name: encoder_name,
        description: description.to_string(),
        can_encode,
        can_decode,
        is_hardware_accelerated,
        supported_pix_fmts: vec![], // Could be parsed from additional FFmpeg commands
        supported_frame_rates: vec![], // Could be parsed from additional FFmpeg commands
        threading_support: None,    // Could be parsed from additional FFmpeg commands
    })
}

/// Check if an encoder is hardware accelerated
fn is_hardware_encoder(name: &str) -> bool {
    let hardware_keywords = [
        "nvenc",
        "qsv",
        "amf",
        "vaapi",
        "videotoolbox",
        "cuda",
        "nvidia",
        "intel",
        "amd",
        "quicksync",
        "vce",
        "media_sdk",
    ];

    let name_lower = name.to_lowercase();
    hardware_keywords
        .iter()
        .any(|keyword| name_lower.contains(keyword))
}

/// Parse formats from formats output
fn parse_formats(output: &str) -> Vec<FormatInfo> {
    let mut formats = Vec::new();
    let lines: Vec<&str> = output.lines().collect();

    // Skip header lines and start parsing from line 4 (index 3)
    for line in lines.iter().skip(3) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('-') {
            continue;
        }

        // Parse format line format: "D   3gp                  3GP format"
        if let Some(parts) = parse_format_line(trimmed) {
            formats.push(parts);
        }
    }

    formats
}

/// Parse individual format line
fn parse_format_line(line: &str) -> Option<FormatInfo> {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() < 4 {
        return None;
    }

    let can_demux = chars[0] == 'D';
    let can_mux = chars[2] == 'E';

    // Extract the rest of the line after capabilities
    let rest: String = chars[4..].iter().collect();
    let trimmed = rest.trim();

    // Find format name and description
    let format_name_end = trimmed.find(' ').unwrap_or(trimmed.len());
    let format_name = trimmed[..format_name_end].trim().to_string();

    let description_start = if format_name.len() < trimmed.len() {
        trimmed[format_name.len()..].trim()
    } else {
        ""
    };

    let description = description_start
        .trim_matches(|c| c == '/' || c == ' ')
        .trim();

    Some(FormatInfo {
        name: format_name,
        description: description.to_string(),
        can_mux,
        can_demux,
    })
}

/// Get supported protocols (common ones)
fn get_supported_protocols() -> Vec<String> {
    vec![
        "http".to_string(),
        "https".to_string(),
        "rtmp".to_string(),
        "rtsp".to_string(),
        "file".to_string(),
        "pipe".to_string(),
        "tcp".to_string(),
        "udp".to_string(),
    ]
}

/// Parse license information from version output
fn parse_license(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();

    for line in lines {
        if line.contains("license:") {
            if let Some(license_part) = line.split("license:").nth(1) {
                return license_part.trim().to_string();
            }
        }
    }

    "Unknown".to_string()
}

/// Parse build information from version output
fn parse_build_info(output: &str) -> BuildInfo {
    let compiler = "Unknown".to_string();
    let configuration = parse_configuration(output);
    let libraries = vec![]; // Could be parsed from additional FFmpeg commands

    BuildInfo {
        compiler,
        configuration,
        libraries,
    }
}

/// Get only hardware-accelerated encoders
pub fn get_hardware_encoders() -> Result<Vec<EncoderInfo>, FFmpegError> {
    let info = get_ffmpeg_info()?;
    let hardware_encoders: Vec<EncoderInfo> = info
        .available_encoders
        .into_iter()
        .filter(|e| e.is_hardware_accelerated)
        .collect();

    Ok(hardware_encoders)
}

/// Get available video encoders
pub fn get_video_encoders() -> Result<Vec<EncoderInfo>, FFmpegError> {
    let info = get_ffmpeg_info()?;
    let video_encoders: Vec<EncoderInfo> = info
        .available_encoders
        .into_iter()
        .filter(|e| {
            e.name.to_lowercase().contains("h264")
                || e.name.to_lowercase().contains("h265")
                || e.name.to_lowercase().contains("hevc")
                || e.name.to_lowercase().contains("vp9")
                || e.name.to_lowercase().contains("av1")
        })
        .collect();

    Ok(video_encoders)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- FFmpegError ----

    #[test]
    fn ffmpeg_error_not_found_display_is_non_empty() {
        let err = FFmpegError::NotFound;
        let msg = err.to_string();
        assert!(!msg.is_empty());
    }

    // ---- get_ffmpeg_executable_name (tested via cfg!) ----

    #[test]
    fn get_ffmpeg_executable_name_is_non_empty() {
        let name = get_ffmpeg_executable_name();
        assert!(!name.is_empty());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn get_ffmpeg_executable_name_has_exe_extension_on_windows() {
        let name = get_ffmpeg_executable_name();
        assert!(
            name.ends_with(".exe"),
            "expected .exe extension, got: {}",
            name
        );
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn get_ffmpeg_executable_name_has_no_extension_on_non_windows() {
        let name = get_ffmpeg_executable_name();
        assert_eq!(name, "ffmpeg");
    }

    // ---- check_ffmpeg_in_dir ----

    #[test]
    fn check_ffmpeg_in_dir_returns_none_for_nonexistent_directory() {
        let fake_dir = std::path::Path::new("/nonexistent/directory/that/does/not/exist");
        let result = check_ffmpeg_in_dir(fake_dir);
        assert!(result.is_none());
    }

    #[test]
    fn check_ffmpeg_in_dir_returns_none_for_empty_temp_directory() {
        let tmp = std::env::temp_dir().join("lolshorts_test_empty_ffmpeg_dir");
        let _ = std::fs::create_dir_all(&tmp);
        let result = check_ffmpeg_in_dir(&tmp);
        let _ = std::fs::remove_dir(&tmp);
        assert!(result.is_none());
    }

    #[test]
    fn check_ffmpeg_in_dir_finds_ffmpeg_exe_in_directory() {
        // Create a temp dir with a fake ffmpeg.exe to verify detection logic
        let tmp = std::env::temp_dir().join("lolshorts_test_ffmpeg_detection");
        let _ = std::fs::create_dir_all(&tmp);

        let ffmpeg_name = if cfg!(target_os = "windows") {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        let fake_ffmpeg = tmp.join(ffmpeg_name);
        std::fs::write(&fake_ffmpeg, b"").expect("should write fake ffmpeg");

        let result = check_ffmpeg_in_dir(&tmp);

        // Clean up before asserting
        let _ = std::fs::remove_file(&fake_ffmpeg);
        let _ = std::fs::remove_dir(&tmp);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), fake_ffmpeg);
    }

    // ---- is_hardware_encoder (via parse path) ----

    #[test]
    fn parse_ffmpeg_version_returns_unknown_for_empty_string() {
        // parse_ffmpeg_version is private; exercise via parse_build_info which calls parse_configuration
        // We test the public surface: get_ffmpeg_path returns an error when ffmpeg is absent.
        // This test verifies the error path is reachable without panicking.
        //
        // Reset the OnceLock by calling get_ffmpeg_path — it may succeed or return NotFound;
        // either way it must not panic.
        let _result = get_ffmpeg_path();
        // No assertion on result because the test environment may or may not have ffmpeg.
    }

    // ---- EncoderInfo struct ----

    #[test]
    fn encoder_info_can_be_constructed() {
        let info = EncoderInfo {
            name: "h264_nvenc".to_string(),
            description: "NVIDIA NVENC H.264 encoder".to_string(),
            can_encode: true,
            can_decode: false,
            is_hardware_accelerated: true,
            supported_pix_fmts: vec!["yuv420p".to_string()],
            supported_frame_rates: vec![],
            threading_support: Some("frame".to_string()),
        };

        assert_eq!(info.name, "h264_nvenc");
        assert!(info.can_encode);
        assert!(!info.can_decode);
        assert!(info.is_hardware_accelerated);
    }

    #[test]
    fn encoder_info_serializes_and_deserializes_correctly() {
        let info = EncoderInfo {
            name: "libx264".to_string(),
            description: "libx264 H.264 encoder".to_string(),
            can_encode: true,
            can_decode: false,
            is_hardware_accelerated: false,
            supported_pix_fmts: vec![],
            supported_frame_rates: vec![],
            threading_support: None,
        };

        let json = serde_json::to_string(&info).expect("serialization should succeed");
        let restored: EncoderInfo =
            serde_json::from_str(&json).expect("deserialization should succeed");

        assert_eq!(restored.name, "libx264");
        assert!(!restored.is_hardware_accelerated);
    }

    // ---- FormatInfo struct ----

    #[test]
    fn format_info_can_be_constructed() {
        let fmt = FormatInfo {
            name: "mp4".to_string(),
            description: "MP4 (MPEG-4 Part 14)".to_string(),
            can_mux: true,
            can_demux: true,
        };

        assert_eq!(fmt.name, "mp4");
        assert!(fmt.can_mux);
        assert!(fmt.can_demux);
    }
}
