//! WASAPI loopback audio capture via cpal
//!
//! Captures system audio output (desktop audio) using Windows WASAPI loopback mode.
//! This replaces the unreliable DirectShow "Stereo Mix" approach that fails on most
//! modern PCs where Stereo Mix is disabled by default.
//!
//! The captured audio is written to a WAV file that can be muxed with video by FFmpeg.
//!
//! Design: cpal::Stream is !Send+!Sync, so the stream lives on a dedicated thread.
//! WasapiCapture itself is Send+Sync and communicates via atomics.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info, warn};

/// WASAPI loopback audio capture (Send + Sync safe)
///
/// The cpal::Stream is owned by a dedicated capture thread.
/// This struct only holds Send+Sync types for Tauri State compatibility.
pub struct WasapiCapture {
    output_path: PathBuf,
    is_capturing: Arc<AtomicBool>,
    /// Handle to the dedicated capture thread
    capture_thread: Option<std::thread::JoinHandle<()>>,
}

// Safety: WasapiCapture only contains Send+Sync fields (PathBuf, Arc<AtomicBool>, JoinHandle)
// The !Send cpal::Stream lives entirely within the capture thread.
unsafe impl Send for WasapiCapture {}
unsafe impl Sync for WasapiCapture {}

impl WasapiCapture {
    /// Create a new WASAPI capture instance
    ///
    /// # Arguments
    /// * `output_dir` - Directory where the WAV file will be written
    pub fn new(output_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(output_dir)
            .context("Failed to create output directory for WASAPI audio")?;

        let output_path = output_dir.join("wasapi_loopback.wav");

        Ok(Self {
            output_path,
            is_capturing: Arc::new(AtomicBool::new(false)),
            capture_thread: None,
        })
    }

    /// Start capturing system audio via WASAPI loopback
    pub fn start(&mut self) -> Result<()> {
        if self.is_capturing.load(Ordering::SeqCst) {
            anyhow::bail!("WASAPI capture is already running");
        }

        let is_capturing = Arc::clone(&self.is_capturing);
        let output_path = self.output_path.clone();

        // Use a channel to report initialization result from the capture thread
        let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<()>>();

        let thread_handle = std::thread::Builder::new()
            .name("wasapi-capture".into())
            .spawn(move || {
                if let Err(e) = run_capture_thread(&output_path, &is_capturing, init_tx) {
                    error!("WASAPI capture thread error: {}", e);
                    is_capturing.store(false, Ordering::SeqCst);
                }
            })
            .context("Failed to spawn WASAPI capture thread")?;

        // Wait for initialization result from the capture thread
        let init_result = init_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| anyhow::anyhow!("WASAPI capture thread initialization timed out"))?;

        init_result?;

        self.is_capturing.store(true, Ordering::SeqCst);
        self.capture_thread = Some(thread_handle);

        info!(
            "WASAPI loopback capture started: {}",
            self.output_path.display()
        );
        Ok(())
    }

    /// Stop capturing and finalize the WAV file
    ///
    /// Returns the path to the WAV file if capture was active, None otherwise.
    pub fn stop(&mut self) -> Option<PathBuf> {
        if !self.is_capturing.load(Ordering::SeqCst) {
            return None;
        }

        // Signal the capture thread to stop
        self.is_capturing.store(false, Ordering::SeqCst);

        // Wait for the capture thread to finish
        if let Some(handle) = self.capture_thread.take() {
            let _ = handle.join();
        }

        if self.output_path.exists() {
            info!(
                "WASAPI loopback capture stopped: {}",
                self.output_path.display()
            );
            Some(self.output_path.clone())
        } else {
            warn!("WASAPI WAV file not found after capture");
            None
        }
    }

    /// Get the output WAV file path
    pub fn output_path(&self) -> &Path {
        &self.output_path
    }
}

impl Drop for WasapiCapture {
    fn drop(&mut self) {
        if self.is_capturing.load(Ordering::SeqCst) {
            self.is_capturing.store(false, Ordering::SeqCst);
        }
        if let Some(handle) = self.capture_thread.take() {
            let _ = handle.join();
        }
    }
}

/// Run the audio capture on a dedicated thread.
/// cpal::Stream is !Send, so it must be created and used entirely within this thread.
fn run_capture_thread(
    output_path: &Path,
    is_capturing: &Arc<AtomicBool>,
    init_tx: std::sync::mpsc::Sender<Result<()>>,
) -> Result<()> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    // Get WASAPI host
    let host = get_wasapi_host()?;

    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            let err = anyhow::anyhow!("No default audio output device found");
            let _ = init_tx.send(Err(anyhow::anyhow!("No default audio output device found")));
            return Err(err);
        }
    };

    let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
    info!("WASAPI loopback device: {}", device_name);

    let supported_config = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            let err = anyhow::anyhow!("Failed to get default output config: {}", e);
            let _ = init_tx.send(Err(anyhow::anyhow!(
                "Failed to get default output config: {}",
                e
            )));
            return Err(err);
        }
    };

    info!(
        "WASAPI config: channels={}, sample_rate={}, sample_format={:?}",
        supported_config.channels(),
        supported_config.sample_rate().0,
        supported_config.sample_format()
    );

    let spec = hound::WavSpec {
        channels: supported_config.channels(),
        sample_rate: supported_config.sample_rate().0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let writer = match hound::WavWriter::create(output_path, spec) {
        Ok(w) => w,
        Err(e) => {
            let err = anyhow::anyhow!("Failed to create WAV file: {}", e);
            let _ = init_tx.send(Err(anyhow::anyhow!("Failed to create WAV file: {}", e)));
            return Err(err);
        }
    };
    let writer = Arc::new(parking_lot::Mutex::new(Some(writer)));

    let is_cap = Arc::clone(is_capturing);
    let writer_clone = Arc::clone(&writer);
    let sample_format = supported_config.sample_format();

    let err_flag = Arc::clone(is_capturing);
    let err_callback = move |err: cpal::StreamError| {
        error!("WASAPI stream error: {}", err);
        err_flag.store(false, Ordering::SeqCst);
    };

    let config: cpal::StreamConfig = supported_config.into();

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !is_cap.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(ref mut w) = *writer_clone.lock() {
                    for &sample in data {
                        if w.write_sample(f32_to_i16(sample)).is_err() {
                            break;
                        }
                    }
                }
            },
            err_callback,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                if !is_cap.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(ref mut w) = *writer_clone.lock() {
                    for &sample in data {
                        if w.write_sample(sample).is_err() {
                            break;
                        }
                    }
                }
            },
            err_callback,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                if !is_cap.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(ref mut w) = *writer_clone.lock() {
                    for &sample in data {
                        let i16_sample = (sample as i32 - 32768) as i16;
                        if w.write_sample(i16_sample).is_err() {
                            break;
                        }
                    }
                }
            },
            err_callback,
            None,
        ),
        format => {
            let err = anyhow::anyhow!("Unsupported sample format: {:?}", format);
            let _ = init_tx.send(Err(anyhow::anyhow!(
                "Unsupported sample format: {:?}",
                format
            )));
            return Err(err);
        }
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            let err = anyhow::anyhow!("Failed to build WASAPI input stream: {}", e);
            let _ = init_tx.send(Err(anyhow::anyhow!(
                "Failed to build WASAPI input stream: {}",
                e
            )));
            return Err(err);
        }
    };

    if let Err(e) = stream.play() {
        let err = anyhow::anyhow!("Failed to start WASAPI stream: {}", e);
        let _ = init_tx.send(Err(anyhow::anyhow!("Failed to start WASAPI stream: {}", e)));
        return Err(err);
    }

    // Signal successful initialization
    let _ = init_tx.send(Ok(()));

    // Keep thread alive while capturing -- poll the flag
    while is_capturing.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // Drop stream to stop capture callbacks
    drop(stream);

    // Finalize WAV: take the writer out and let it drop (writes header)
    if let Some(w) = writer.lock().take() {
        if let Err(e) = w.finalize() {
            warn!("Failed to finalize WAV file: {}", e);
        }
    }

    info!("WASAPI capture thread exiting");
    Ok(())
}

/// Enumerate available audio output devices suitable for WASAPI loopback capture.
///
/// Returns a list of `(device_id, device_name)` pairs.  The `device_id` is the
/// device name as reported by cpal (which is also how cpal selects devices), so
/// it can be stored in `AudioSettings::audio_device_id` and used to request a
/// specific device at capture time.
///
/// Currently uses the system default device only; full enumeration via the
/// Windows MMDevice API can be added here in a future iteration.
#[cfg(windows)]
pub fn enumerate_audio_devices() -> Vec<(String, String)> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = match get_wasapi_host() {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("enumerate_audio_devices: failed to get WASAPI host: {}", e);
            return vec![];
        }
    };

    // Collect all output devices that support loopback (output devices on WASAPI)
    match host.output_devices() {
        Ok(devices) => devices
            .filter_map(|d| {
                let name = d.name().unwrap_or_default();
                if name.is_empty() {
                    None
                } else {
                    Some((name.clone(), name))
                }
            })
            .collect(),
        Err(e) => {
            tracing::warn!(
                "enumerate_audio_devices: failed to list output devices: {}",
                e
            );
            vec![]
        }
    }
}

/// Non-Windows stub — returns an empty list on unsupported platforms.
#[cfg(not(windows))]
pub fn enumerate_audio_devices() -> Vec<(String, String)> {
    vec![]
}

/// Get WASAPI host, falling back to default host if WASAPI ID is not found
fn get_wasapi_host() -> Result<cpal::Host> {
    // Try to find WASAPI host explicitly
    let wasapi_host = cpal::available_hosts()
        .into_iter()
        .find(|id| id.name().to_lowercase().contains("wasapi"));

    match wasapi_host {
        Some(host_id) => {
            let host = cpal::host_from_id(host_id).context("Failed to initialize WASAPI host")?;
            info!("Using explicit WASAPI host");
            Ok(host)
        }
        None => {
            // On Windows, default_host() is WASAPI anyway
            warn!("WASAPI host ID not found in available hosts, using default host (likely WASAPI on Windows)");
            Ok(cpal::default_host())
        }
    }
}

/// Convert f32 audio sample to i16 for WAV writing
#[inline]
fn f32_to_i16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f32_to_i16_conversion() {
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(1.0), i16::MAX);
        assert_eq!(f32_to_i16(-1.0), -i16::MAX);
        // Clamp beyond range
        assert_eq!(f32_to_i16(2.0), i16::MAX);
        assert_eq!(f32_to_i16(-2.0), -i16::MAX);
    }

    #[test]
    fn test_wasapi_capture_creation() {
        let temp_dir = std::env::temp_dir().join("lolshorts_wasapi_test");
        let result = WasapiCapture::new(&temp_dir);
        assert!(result.is_ok());

        let capture = result.unwrap();
        assert!(!capture.is_capturing.load(Ordering::SeqCst));
        assert!(capture.output_path().ends_with("wasapi_loopback.wav"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_wasapi_stop_when_not_capturing() {
        let temp_dir = std::env::temp_dir().join("lolshorts_wasapi_stop_test");
        let mut capture = WasapiCapture::new(&temp_dir).unwrap();

        // Stop without starting should return None
        assert!(capture.stop().is_none());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_wasapi_capture_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WasapiCapture>();
    }
}
