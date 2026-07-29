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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// Which audio endpoint a capture reads from.
///
/// Both variants build a cpal *input* stream — WASAPI loopback works by opening
/// an OUTPUT device in capture mode, while a microphone is a normal INPUT device.
/// Only the device-selection and default-config calls differ, so the two share the
/// same capture thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureSource {
    /// System audio via WASAPI loopback on an output (render) device.
    Loopback,
    /// Microphone via an input (capture) device.
    Microphone,
}

impl CaptureSource {
    fn label(self) -> &'static str {
        match self {
            CaptureSource::Loopback => "loopback",
            CaptureSource::Microphone => "microphone",
        }
    }
}

/// How often the capture thread wakes up to re-check the run flag.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How often the WAV header is checkpointed while capture is still running.
/// See `flush_writer` for why this is a correctness requirement, not just crash insurance.
const FLUSH_INTERVAL: Duration = Duration::from_millis(500);

/// Sentinel kept in `CaptureFlags::exit_reason` until the capture thread reports one.
const EXIT_REASON_UNSET: u8 = u8::MAX;

/// Type of the WAV writer used by the capture thread (hound wraps the file in a BufWriter).
type WavFileWriter = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

/// Writer shared between the capture thread and the cpal data callback.
/// `None` once the writer has been taken out and finalized.
type SharedWavWriter = Arc<parking_lot::Mutex<Option<WavFileWriter>>>;

/// Why the capture thread stopped.
///
/// The thread used to log a bare `"WASAPI capture thread exiting"` with no cause, so a
/// silent recording looked exactly like a healthy one in the logs. Every exit path now
/// records one of these and logs it, at `error!` level when it means lost audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitReason {
    /// `stop()` / `Drop` asked for shutdown — the normal path.
    Requested,
    /// The cpal error callback reported a stream failure (device unplugged, format lost…).
    StreamError,
    /// Writing samples to the WAV failed (disk full, handle lost…).
    WriteError,
    /// Device selection / config / stream construction failed; capture never started.
    InitFailed,
    /// The run flag was already false on the very first poll: nothing was ever captured.
    /// This is the start/stop race that produced 44-byte header-only WAVs.
    NeverStarted,
    /// The run flag was cleared mid-run without a stop request or a recorded error.
    /// Should not happen — logged loudly so it cannot hide again.
    FlagCleared,
}

impl ExitReason {
    const fn as_u8(self) -> u8 {
        match self {
            ExitReason::Requested => 0,
            ExitReason::StreamError => 1,
            ExitReason::WriteError => 2,
            ExitReason::InitFailed => 3,
            ExitReason::NeverStarted => 4,
            ExitReason::FlagCleared => 5,
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(ExitReason::Requested),
            1 => Some(ExitReason::StreamError),
            2 => Some(ExitReason::WriteError),
            3 => Some(ExitReason::InitFailed),
            4 => Some(ExitReason::NeverStarted),
            5 => Some(ExitReason::FlagCleared),
            _ => None,
        }
    }

    /// Human-readable cause for the log line.
    fn describe(self) -> &'static str {
        match self {
            ExitReason::Requested => "stop requested by owner (normal shutdown)",
            ExitReason::StreamError => "cpal stream error callback fired",
            ExitReason::WriteError => "WAV write/flush failed",
            ExitReason::InitFailed => "initialization failed, capture never started",
            ExitReason::NeverStarted => {
                "run flag was already false on the first poll (start/stop race) — no audio captured"
            }
            ExitReason::FlagCleared => "run flag cleared without a stop request or recorded error",
        }
    }

    /// Whether this exit means audio was lost and must be logged at `error!` level.
    fn is_error(self) -> bool {
        !matches!(self, ExitReason::Requested)
    }
}

/// Classify why the poll loop ended. Pure logic so it can be unit-tested without a
/// real audio device.
///
/// Priority is deliberate: a stream/write error is the most actionable cause even when a
/// stop was also requested afterwards, and `!ran` (loop body never executed) pins the
/// start/stop race apart from a normal stop.
fn classify_exit(
    ran: bool,
    stop_requested: bool,
    stream_error: bool,
    write_error: bool,
) -> ExitReason {
    if stream_error {
        ExitReason::StreamError
    } else if write_error {
        ExitReason::WriteError
    } else if stop_requested {
        ExitReason::Requested
    } else if !ran {
        ExitReason::NeverStarted
    } else {
        ExitReason::FlagCleared
    }
}

/// State shared between the owner (`WasapiCapture`) and its capture thread.
///
/// Previously a single `Arc<AtomicBool>` carried four different meanings at once (owner
/// session state, thread run flag, stop request, stream-error signal). That conflation is
/// what made a stream error flip the owner's state, so `stop()` returned `None`, skipped
/// the thread join and threw away the WAV path.
struct CaptureFlags {
    /// Owner-visible session state: true between a successful `start()` and `stop()`.
    is_capturing: AtomicBool,
    /// Capture-thread run flag. Cleared by `stop()`/`Drop` or by a fatal stream/write error.
    should_run: AtomicBool,
    /// Set when `stop()`/`Drop` requested shutdown (used for exit classification).
    stop_requested: AtomicBool,
    /// Set by the cpal error callback.
    stream_error: AtomicBool,
    /// Set when writing/flushing the WAV failed.
    write_error: AtomicBool,
    /// Samples handed to hound. Zero at stop time means every clip will be silent.
    samples_written: AtomicU64,
    /// `ExitReason::as_u8` recorded by the capture thread, or `EXIT_REASON_UNSET`.
    exit_reason: AtomicU8,
}

impl CaptureFlags {
    fn new() -> Self {
        Self {
            is_capturing: AtomicBool::new(false),
            should_run: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            stream_error: AtomicBool::new(false),
            write_error: AtomicBool::new(false),
            samples_written: AtomicU64::new(0),
            exit_reason: AtomicU8::new(EXIT_REASON_UNSET),
        }
    }

    /// Arm the flags for a new capture run.
    ///
    /// MUST be called BEFORE the capture thread is spawned. The thread reads `should_run`
    /// immediately after signalling successful init, so arming it afterwards (what the old
    /// `start()` did) is a race the thread normally lost: the poll loop exited on its very
    /// first check, the stream was dropped ~19ms later and the WAV stayed 44 bytes (header
    /// only) while the owner happily logged "capture started".
    fn arm(&self) {
        self.stop_requested.store(false, Ordering::SeqCst);
        self.stream_error.store(false, Ordering::SeqCst);
        self.write_error.store(false, Ordering::SeqCst);
        self.samples_written.store(0, Ordering::SeqCst);
        self.exit_reason.store(EXIT_REASON_UNSET, Ordering::SeqCst);
        // Published last: the capture thread must never observe a stale error flag
        // together with a live run flag.
        self.should_run.store(true, Ordering::SeqCst);
    }

    /// Ask the capture thread to stop (normal shutdown).
    fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        self.should_run.store(false, Ordering::SeqCst);
    }

    /// Stop the capture thread because of a failure, without marking it a normal shutdown.
    fn abort_run(&self) {
        self.should_run.store(false, Ordering::SeqCst);
    }

    fn record_exit(&self, reason: ExitReason) {
        self.exit_reason.store(reason.as_u8(), Ordering::SeqCst);
    }

    fn exit_reason(&self) -> Option<ExitReason> {
        ExitReason::from_u8(self.exit_reason.load(Ordering::SeqCst))
    }

    /// Recorded exit cause, or a placeholder when the thread never reported one.
    fn exit_description(&self) -> &'static str {
        self.exit_reason()
            .map(ExitReason::describe)
            .unwrap_or("no exit reason recorded (thread did not report)")
    }
}

/// WASAPI audio capture (Send + Sync safe)
///
/// Captures either system audio (WASAPI loopback on an output device) or the
/// microphone (a normal input device), selected by `source`. The cpal::Stream is
/// owned by a dedicated capture thread; this struct only holds Send+Sync types for
/// Tauri State compatibility.
pub struct WasapiCapture {
    output_path: PathBuf,
    /// Run/stop/error state shared with the capture thread and the cpal callbacks.
    flags: Arc<CaptureFlags>,
    /// Handle to the dedicated capture thread
    capture_thread: Option<std::thread::JoinHandle<()>>,
    /// Preferred device name (None = system default for this source kind).
    /// Matched case-insensitively against the source's device list.
    device_name: Option<String>,
    /// Whether this instance captures system loopback or a microphone.
    source: CaptureSource,
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
    /// * `device_name` - Preferred output device name for loopback capture, or `None`
    ///   to use the system default output. When set but not found at capture time, the
    ///   default device is used and a warning is logged.
    pub fn new(output_dir: &Path, device_name: Option<String>) -> Result<Self> {
        Self::new_with_source(
            output_dir,
            device_name,
            CaptureSource::Loopback,
            "wasapi_loopback.wav",
        )
    }

    /// Create a microphone capture instance.
    ///
    /// Writes to `mic_capture.wav` in `output_dir` (the shared segment-directory
    /// contract) and selects an INPUT device by name with the same
    /// exact→substring→default fallback as loopback selection.
    ///
    /// # Arguments
    /// * `output_dir` - Directory where `mic_capture.wav` will be written
    /// * `device_name` - Preferred microphone (input) device name, or `None` to use
    ///   the system default input. When set but not found at capture time, the
    ///   default input device is used and a warning is logged.
    pub fn new_microphone(output_dir: &Path, device_name: Option<String>) -> Result<Self> {
        Self::new_with_source(
            output_dir,
            device_name,
            CaptureSource::Microphone,
            "mic_capture.wav",
        )
    }

    fn new_with_source(
        output_dir: &Path,
        device_name: Option<String>,
        source: CaptureSource,
        file_name: &str,
    ) -> Result<Self> {
        std::fs::create_dir_all(output_dir)
            .context("Failed to create output directory for WASAPI audio")?;

        let output_path = output_dir.join(file_name);

        Ok(Self {
            output_path,
            flags: Arc::new(CaptureFlags::new()),
            capture_thread: None,
            device_name,
            source,
        })
    }

    /// Start capturing system audio via WASAPI loopback
    pub fn start(&mut self) -> Result<()> {
        if self.flags.is_capturing.load(Ordering::SeqCst) {
            anyhow::bail!("WASAPI capture is already running");
        }

        // Arm the run flag BEFORE the thread exists — see `CaptureFlags::arm` for the
        // race this ordering fixes (thread exited on its first poll => silent 44-byte WAV).
        self.flags.arm();

        let flags = Arc::clone(&self.flags);
        let output_path = self.output_path.clone();
        let device_name = self.device_name.clone();
        let source = self.source;

        // Use a channel to report initialization result from the capture thread
        let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<()>>();

        let spawn_result = std::thread::Builder::new()
            .name(format!("wasapi-{}-capture", source.label()))
            .spawn(move || {
                if let Err(e) =
                    run_capture_thread(&output_path, &flags, device_name, source, init_tx)
                {
                    // Every failing path records its reason first, so this line always
                    // carries a cause instead of a bare "exiting".
                    error!(
                        "WASAPI {} capture thread error [{}]: {}",
                        source.label(),
                        flags.exit_description(),
                        e
                    );
                    flags.abort_run();
                }
            });

        let thread_handle = match spawn_result {
            Ok(handle) => handle,
            Err(e) => {
                // Nothing will ever clear the flag we just armed, so disarm it here.
                self.flags.abort_run();
                self.flags.record_exit(ExitReason::InitFailed);
                return Err(anyhow::Error::new(e).context("Failed to spawn WASAPI capture thread"));
            }
        };

        // Wait for initialization result from the capture thread
        let init_result = match init_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => result,
            Err(e) => {
                // The thread is stuck or died without answering: disarm so it unwinds if
                // it ever gets there, and leave it detached (joining could block forever).
                self.flags.request_stop();
                self.flags.record_exit(ExitReason::InitFailed);
                return Err(anyhow::anyhow!(
                    "WASAPI {} capture thread initialization timed out: {}",
                    source.label(),
                    e
                ));
            }
        };

        if let Err(e) = init_result {
            // The thread returns immediately after sending an init error, so this join
            // is bounded and keeps the failure path free of detached threads.
            self.flags.request_stop();
            let _ = thread_handle.join();
            return Err(e);
        }

        self.flags.is_capturing.store(true, Ordering::SeqCst);
        self.capture_thread = Some(thread_handle);

        info!(
            "WASAPI {} capture started: {}",
            self.source.label(),
            self.output_path.display()
        );
        Ok(())
    }

    /// Stop capturing and finalize the WAV file
    ///
    /// Returns the path to the WAV file if capture was active, None otherwise.
    pub fn stop(&mut self) -> Option<PathBuf> {
        if !self.flags.is_capturing.load(Ordering::SeqCst) {
            return None;
        }

        // Signal the capture thread to stop
        self.flags.request_stop();
        self.flags.is_capturing.store(false, Ordering::SeqCst);

        // Wait for the capture thread to finish
        if let Some(handle) = self.capture_thread.take() {
            let _ = handle.join();
        }

        // Always report how much audio actually landed and why the thread ended: a
        // silent capture used to be indistinguishable from a healthy one in the logs.
        let samples = self.flags.samples_written.load(Ordering::SeqCst);
        if samples == 0 {
            error!(
                "WASAPI {} capture wrote 0 samples — clips will be SILENT [{}]",
                self.source.label(),
                self.flags.exit_description()
            );
        } else {
            info!(
                "WASAPI {} capture stopped after {} samples [{}]",
                self.source.label(),
                samples,
                self.flags.exit_description()
            );
        }

        if self.output_path.exists() {
            info!(
                "WASAPI {} WAV finalized: {}",
                self.source.label(),
                self.output_path.display()
            );
            Some(self.output_path.clone())
        } else {
            warn!(
                "WASAPI {} WAV file not found after capture",
                self.source.label()
            );
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
        // Unconditional: a capture whose `start()` timed out has `is_capturing == false`
        // but may still own a live run flag, and the thread must be told to stop either way.
        self.flags.request_stop();
        self.flags.is_capturing.store(false, Ordering::SeqCst);
        if let Some(handle) = self.capture_thread.take() {
            let _ = handle.join();
        }
    }
}

/// Record an initialization failure, answer `start()` and build the error to return.
///
/// Every early return in `run_capture_thread` goes through this: the previous code let
/// `get_wasapi_host()?` propagate WITHOUT answering `init_tx`, so `start()` blocked for the
/// full 5s timeout and reported "initialization timed out" instead of the real cause.
fn fail_init(
    flags: &CaptureFlags,
    init_tx: &std::sync::mpsc::Sender<Result<()>>,
    msg: String,
) -> anyhow::Error {
    flags.record_exit(ExitReason::InitFailed);
    flags.abort_run();
    let _ = init_tx.send(Err(anyhow::anyhow!("{}", msg)));
    anyhow::anyhow!(msg)
}

/// Write one cpal callback buffer into the WAV, converting samples to i16.
///
/// Shared by all sample-format branches so the error handling exists exactly once:
/// a failed write is logged (only the first one — the callback fires ~100x/s) and tears
/// the capture down, because WAV write failures (disk full, handle lost) never recover on
/// their own and a silent spin would hide the cause all over again.
fn write_block<T, F>(
    writer: &SharedWavWriter,
    flags: &CaptureFlags,
    source: CaptureSource,
    data: &[T],
    convert: F,
) where
    T: Copy,
    F: Fn(T) -> i16,
{
    if !flags.should_run.load(Ordering::Relaxed) {
        return;
    }
    let mut guard = writer.lock();
    let Some(w) = guard.as_mut() else {
        return;
    };
    for &sample in data {
        if let Err(e) = w.write_sample(convert(sample)) {
            if !flags.write_error.swap(true, Ordering::SeqCst) {
                error!("WASAPI {} WAV write failed: {}", source.label(), e);
            }
            flags.abort_run();
            return;
        }
    }
    flags
        .samples_written
        .fetch_add(data.len() as u64, Ordering::Relaxed);
}

/// Checkpoint the WAV header while capture is still running.
///
/// hound only writes the RIFF/data chunk sizes in `finalize()` (or on drop), which breaks
/// this recorder in two ways:
///
/// 1. `save_clip` muxes `wasapi_loopback.wav` WHILE the capture thread is still running.
///    A data chunk length of 0 makes FFmpeg treat the file as empty -> silent clip.
/// 2. A crash/kill leaves a 44-byte header-only WAV with all captured audio unreadable.
///
/// `flush()` rewrites both size fields, flushes the BufWriter and seeks back, so capture
/// continues unaffected and the file on disk is valid up to the last checkpoint.
fn flush_writer(writer: &SharedWavWriter, flags: &CaptureFlags, source: CaptureSource) {
    let mut guard = writer.lock();
    let Some(w) = guard.as_mut() else {
        return;
    };
    if let Err(e) = w.flush() {
        // Stop capturing rather than push on.
        //
        // `hound`'s `flush()` rewrites the RIFF/data size fields and seeks back to the
        // append position. A failure can land anywhere in that sequence, so the writer's
        // file offset is no longer known to be correct — continuing to write would append
        // samples at the wrong place and produce a file that *looks* like a valid WAV but
        // decodes to garbage. A shorter recording that is correct up to the last good
        // checkpoint beats a full-length one that is silently corrupt, and the operator
        // still learns why from the exit reason below.
        if !flags.write_error.swap(true, Ordering::SeqCst) {
            error!(
                "WASAPI {} WAV checkpoint flush failed, stopping capture to avoid                  corrupting the file: {}",
                source.label(),
                e
            );
        }
        flags.should_run.store(false, Ordering::SeqCst);
    }
}

/// Run the audio capture on a dedicated thread.
/// cpal::Stream is !Send, so it must be created and used entirely within this thread.
fn run_capture_thread(
    output_path: &Path,
    flags: &Arc<CaptureFlags>,
    preferred_device: Option<String>,
    source: CaptureSource,
    init_tx: std::sync::mpsc::Sender<Result<()>>,
) -> Result<()> {
    use cpal::traits::{DeviceTrait, StreamTrait};

    // Get WASAPI host
    let host = match get_wasapi_host() {
        Ok(h) => h,
        Err(e) => {
            return Err(fail_init(
                flags,
                &init_tx,
                format!("Failed to initialize WASAPI host: {}", e),
            ))
        }
    };

    // Loopback reads from an OUTPUT (render) device in capture mode; the microphone
    // reads from an INPUT (capture) device. Both then build an input stream.
    let device = match source {
        CaptureSource::Loopback => select_output_device(&host, preferred_device.as_deref()),
        CaptureSource::Microphone => select_input_device(&host, preferred_device.as_deref()),
    };
    let device = match device {
        Some(d) => d,
        None => {
            return Err(fail_init(
                flags,
                &init_tx,
                format!("No audio {} device found", source.label()),
            ))
        }
    };

    let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
    info!("WASAPI {} device: {}", source.label(), device_name);

    let supported_config = match source {
        CaptureSource::Loopback => device.default_output_config(),
        CaptureSource::Microphone => device.default_input_config(),
    };
    let supported_config = match supported_config {
        Ok(c) => c,
        Err(e) => {
            return Err(fail_init(
                flags,
                &init_tx,
                format!("Failed to get default {} config: {}", source.label(), e),
            ))
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
            return Err(fail_init(
                flags,
                &init_tx,
                format!("Failed to create WAV file: {}", e),
            ))
        }
    };
    let writer: SharedWavWriter = Arc::new(parking_lot::Mutex::new(Some(writer)));

    let sample_format = supported_config.sample_format();

    // The error callback must NOT touch the owner's session state: it only aborts the
    // capture run (so the thread exits and finalizes the WAV) and records the cause, which
    // keeps `stop()` able to join the thread and still return the partial WAV.
    let err_flags = Arc::clone(flags);
    let err_callback = move |err: cpal::StreamError| {
        error!("WASAPI {} stream error: {}", source.label(), err);
        err_flags.stream_error.store(true, Ordering::SeqCst);
        err_flags.abort_run();
    };

    let config: cpal::StreamConfig = supported_config.into();

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let writer_cb = Arc::clone(&writer);
            let flags_cb = Arc::clone(flags);
            device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    write_block(&writer_cb, &flags_cb, source, data, f32_to_i16);
                },
                err_callback,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let writer_cb = Arc::clone(&writer);
            let flags_cb = Arc::clone(flags);
            device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    write_block(&writer_cb, &flags_cb, source, data, |s| s);
                },
                err_callback,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let writer_cb = Arc::clone(&writer);
            let flags_cb = Arc::clone(flags);
            device.build_input_stream(
                &config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    write_block(&writer_cb, &flags_cb, source, data, u16_to_i16);
                },
                err_callback,
                None,
            )
        }
        format => {
            return Err(fail_init(
                flags,
                &init_tx,
                format!("Unsupported sample format: {:?}", format),
            ))
        }
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            return Err(fail_init(
                flags,
                &init_tx,
                format!("Failed to build WASAPI input stream: {}", e),
            ))
        }
    };

    if let Err(e) = stream.play() {
        return Err(fail_init(
            flags,
            &init_tx,
            format!("Failed to start WASAPI stream: {}", e),
        ));
    }

    // Signal successful initialization
    let _ = init_tx.send(Ok(()));

    // Keep thread alive while capturing -- poll the flag.
    // `should_run` was armed by `start()` before this thread was spawned, so the first
    // check cannot lose a race with the owner; `ran` records whether the loop body ever
    // executed so an immediate exit is reported as NeverStarted rather than a normal stop.
    let mut ran = false;
    let mut last_flush = Instant::now();
    while flags.should_run.load(Ordering::SeqCst) {
        ran = true;
        std::thread::sleep(POLL_INTERVAL);
        if last_flush.elapsed() >= FLUSH_INTERVAL {
            flush_writer(&writer, flags, source);
            last_flush = Instant::now();
        }
    }

    // Drop stream to stop capture callbacks
    drop(stream);

    // Finalize WAV: take the writer out so no callback can resurrect it, then write the
    // final header. Taking it also makes a late callback a no-op instead of a panic.
    if let Some(w) = writer.lock().take() {
        if let Err(e) = w.finalize() {
            warn!(
                "WASAPI {} failed to finalize WAV file: {}",
                source.label(),
                e
            );
        }
    }

    // ALWAYS log why we are leaving: "exiting" without a cause is what made the silent
    // 44-byte-WAV bug take a full game session to diagnose.
    let reason = classify_exit(
        ran,
        flags.stop_requested.load(Ordering::SeqCst),
        flags.stream_error.load(Ordering::SeqCst),
        flags.write_error.load(Ordering::SeqCst),
    );
    flags.record_exit(reason);
    let samples = flags.samples_written.load(Ordering::SeqCst);
    if reason.is_error() {
        error!(
            "WASAPI {} capture thread exiting: {} ({} samples written)",
            source.label(),
            reason.describe(),
            samples
        );
    } else {
        info!(
            "WASAPI {} capture thread exiting: {} ({} samples written)",
            source.label(),
            reason.describe(),
            samples
        );
    }
    Ok(())
}

/// Enumerate device names for one endpoint kind (`input` = capture/microphone,
/// otherwise output/render). The returned names are exactly what cpal reports and
/// what the capture selectors match against, so they can be stored in settings and
/// used to request a specific device at capture time.
///
/// Windows-only: the sole callers (`enumerate_system_audio_devices` /
/// `enumerate_microphone_devices`) are cfg(windows)-gated, and non-Windows
/// builds would otherwise fail `clippy -D warnings` on dead_code.
#[cfg(target_os = "windows")]
fn enumerate_device_names(input: bool) -> Vec<String> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = match get_wasapi_host() {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("enumerate_device_names: failed to get WASAPI host: {}", e);
            return vec![];
        }
    };

    let devices = if input {
        host.input_devices().map(|it| it.collect::<Vec<_>>())
    } else {
        host.output_devices().map(|it| it.collect::<Vec<_>>())
    };

    match devices {
        Ok(devices) => devices
            .into_iter()
            .filter_map(|d| {
                let name = d.name().unwrap_or_default();
                if name.is_empty() {
                    None
                } else {
                    Some(name)
                }
            })
            .collect(),
        Err(e) => {
            tracing::warn!(
                "enumerate_device_names: failed to list {} devices: {}",
                if input { "input" } else { "output" },
                e
            );
            vec![]
        }
    }
}

/// Shared contract: enumerate system-audio (output/render) device names for the
/// settings dropdown. These are the WASAPI loopback capture candidates.
#[cfg(windows)]
pub fn enumerate_system_audio_devices() -> Vec<String> {
    enumerate_device_names(false)
}

/// Enumerate microphone (input/capture) device names for the settings dropdown.
#[cfg(windows)]
pub fn enumerate_microphone_devices() -> Vec<String> {
    enumerate_device_names(true)
}

/// Enumerate available audio output devices suitable for WASAPI loopback capture.
///
/// Returns a list of `(device_id, device_name)` pairs where `device_id` is the
/// cpal device name (which is also how cpal selects devices), so it can be stored
/// in `AudioSettings::audio_device_id` and used to request a specific device at
/// capture time. Kept for backward compatibility with callers that expect the
/// pair form; new code should prefer `enumerate_system_audio_devices()`.
#[cfg(windows)]
pub fn enumerate_audio_devices() -> Vec<(String, String)> {
    enumerate_system_audio_devices()
        .into_iter()
        .map(|name| (name.clone(), name))
        .collect()
}

/// Non-Windows stubs — return empty lists on unsupported platforms.
#[cfg(not(windows))]
pub fn enumerate_system_audio_devices() -> Vec<String> {
    vec![]
}

#[cfg(not(windows))]
pub fn enumerate_microphone_devices() -> Vec<String> {
    vec![]
}

#[cfg(not(windows))]
pub fn enumerate_audio_devices() -> Vec<(String, String)> {
    vec![]
}

/// Select the output device to capture from.
///
/// When `preferred` is set, matches it case-insensitively against the available output
/// devices (exact match first, then substring). Falls back to the system default output
/// device (with a warning) when the preferred device is not set or cannot be found.
fn select_output_device(host: &cpal::Host, preferred: Option<&str>) -> Option<cpal::Device> {
    use cpal::traits::{DeviceTrait, HostTrait};

    if let Some(name) = preferred.map(str::trim).filter(|s| !s.is_empty()) {
        let want = name.to_lowercase();
        if let Ok(devices) = host.output_devices() {
            let mut candidates: Vec<cpal::Device> = devices.collect();
            // Exact (case-insensitive) match preferred over substring match.
            if let Some(dev) = candidates
                .iter()
                .position(|d| d.name().map(|n| n.to_lowercase() == want).unwrap_or(false))
            {
                info!("WASAPI: using configured output device '{}'", name);
                return Some(candidates.swap_remove(dev));
            }
            if let Some(dev) = candidates.iter().position(|d| {
                d.name()
                    .map(|n| n.to_lowercase().contains(&want))
                    .unwrap_or(false)
            }) {
                info!("WASAPI: matched output device by substring for '{}'", name);
                return Some(candidates.swap_remove(dev));
            }
        }
        warn!(
            "WASAPI: configured output device '{}' not found; falling back to system default",
            name
        );
    }

    host.default_output_device()
}

/// Select the input (microphone) device to capture from.
///
/// Mirrors `select_output_device`: when `preferred` is set, matches it
/// case-insensitively against the available input devices (exact match first, then
/// substring). Falls back to the system default input device (with a warning) when
/// the preferred device is not set or cannot be found.
fn select_input_device(host: &cpal::Host, preferred: Option<&str>) -> Option<cpal::Device> {
    use cpal::traits::{DeviceTrait, HostTrait};

    if let Some(name) = preferred.map(str::trim).filter(|s| !s.is_empty()) {
        let want = name.to_lowercase();
        if let Ok(devices) = host.input_devices() {
            let mut candidates: Vec<cpal::Device> = devices.collect();
            // Exact (case-insensitive) match preferred over substring match.
            if let Some(dev) = candidates
                .iter()
                .position(|d| d.name().map(|n| n.to_lowercase() == want).unwrap_or(false))
            {
                info!("WASAPI: using configured microphone device '{}'", name);
                return Some(candidates.swap_remove(dev));
            }
            if let Some(dev) = candidates.iter().position(|d| {
                d.name()
                    .map(|n| n.to_lowercase().contains(&want))
                    .unwrap_or(false)
            }) {
                info!(
                    "WASAPI: matched microphone device by substring for '{}'",
                    name
                );
                return Some(candidates.swap_remove(dev));
            }
        }
        warn!(
            "WASAPI: configured microphone device '{}' not found; falling back to system default",
            name
        );
    }

    host.default_input_device()
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

/// Convert an unsigned 16-bit sample (midpoint 32768) to signed i16 for WAV writing.
#[inline]
fn u16_to_i16(sample: u16) -> i16 {
    (sample as i32 - 32768) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read the `data` chunk length recorded in a WAV header, without depending on a
    /// fixed header size (hound may emit an extensible fmt chunk).
    fn wav_data_chunk_len(path: &Path) -> Option<u32> {
        let bytes = std::fs::read(path).ok()?;
        let pos = bytes.windows(4).position(|w| w == b"data")?;
        let len = bytes.get(pos + 4..pos + 8)?;
        Some(u32::from_le_bytes([len[0], len[1], len[2], len[3]]))
    }

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
    fn test_u16_to_i16_conversion() {
        // Unsigned midpoint is silence; the endpoints must not wrap around.
        assert_eq!(u16_to_i16(32768), 0);
        assert_eq!(u16_to_i16(0), i16::MIN);
        assert_eq!(u16_to_i16(u16::MAX), i16::MAX);
    }

    #[test]
    fn test_wasapi_capture_creation() {
        let temp_dir = std::env::temp_dir().join("lolshorts_wasapi_test");
        let result = WasapiCapture::new(&temp_dir, None);
        assert!(result.is_ok());

        let capture = result.unwrap();
        assert!(!capture.flags.is_capturing.load(Ordering::SeqCst));
        assert!(capture.output_path().ends_with("wasapi_loopback.wav"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mic_capture_creation_writes_to_mic_wav() {
        // Microphone capture must target the shared `mic_capture.wav` contract path,
        // distinct from the loopback `wasapi_loopback.wav`, so both can coexist in the
        // same segment directory without clobbering each other.
        let temp_dir = std::env::temp_dir().join("lolshorts_mic_test");
        let result = WasapiCapture::new_microphone(&temp_dir, None);
        assert!(result.is_ok());

        let capture = result.unwrap();
        assert!(!capture.flags.is_capturing.load(Ordering::SeqCst));
        assert_eq!(capture.source, CaptureSource::Microphone);
        assert!(capture.output_path().ends_with("mic_capture.wav"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_wasapi_stop_when_not_capturing() {
        let temp_dir = std::env::temp_dir().join("lolshorts_wasapi_stop_test");
        let mut capture = WasapiCapture::new(&temp_dir, None).unwrap();

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

    // --- exit-reason classification (pure logic; no audio device required) ---

    #[test]
    fn test_classify_exit_normal_stop() {
        // Loop ran and the owner asked to stop: the only non-error outcome.
        let reason = classify_exit(true, true, false, false);
        assert_eq!(reason, ExitReason::Requested);
        assert!(!reason.is_error());
    }

    #[test]
    fn test_classify_exit_never_started_is_the_silent_wav_bug() {
        // The regression this unit fixes: the poll loop exited before its first
        // iteration and nobody had requested a stop => zero samples, 44-byte WAV.
        // It must NOT be reported as a normal shutdown.
        let reason = classify_exit(false, false, false, false);
        assert_eq!(reason, ExitReason::NeverStarted);
        assert!(reason.is_error());
    }

    #[test]
    fn test_classify_exit_errors_win_over_stop_request() {
        // A stream/write failure stays visible even when a stop was requested afterwards,
        // otherwise the actionable cause is masked by a clean-looking shutdown.
        assert_eq!(
            classify_exit(true, true, true, false),
            ExitReason::StreamError
        );
        assert_eq!(
            classify_exit(true, true, false, true),
            ExitReason::WriteError
        );
        // Stream errors outrank write errors: the write failure is usually a consequence.
        assert_eq!(
            classify_exit(true, false, true, true),
            ExitReason::StreamError
        );
    }

    #[test]
    fn test_classify_exit_flag_cleared_without_reason() {
        // Ran, no stop request, no error: something cleared the run flag unexpectedly.
        let reason = classify_exit(true, false, false, false);
        assert_eq!(reason, ExitReason::FlagCleared);
        assert!(reason.is_error());
    }

    #[test]
    fn test_exit_reason_u8_roundtrip_and_descriptions() {
        let all = [
            ExitReason::Requested,
            ExitReason::StreamError,
            ExitReason::WriteError,
            ExitReason::InitFailed,
            ExitReason::NeverStarted,
            ExitReason::FlagCleared,
        ];
        for reason in all {
            assert_eq!(ExitReason::from_u8(reason.as_u8()), Some(reason));
            assert!(!reason.describe().is_empty());
        }
        // The sentinel must never decode to a real reason, or "not recorded" would be
        // reported as a normal shutdown.
        assert_eq!(ExitReason::from_u8(EXIT_REASON_UNSET), None);
    }

    #[test]
    fn test_flags_arm_clears_previous_run_state() {
        // `arm()` is the fix for the start/stop race: the run flag must be live (and all
        // stale error state cleared) BEFORE the capture thread is spawned.
        let flags = CaptureFlags::new();
        assert!(!flags.should_run.load(Ordering::SeqCst));

        flags.stream_error.store(true, Ordering::SeqCst);
        flags.write_error.store(true, Ordering::SeqCst);
        flags.stop_requested.store(true, Ordering::SeqCst);
        flags.samples_written.store(1234, Ordering::SeqCst);
        flags.record_exit(ExitReason::StreamError);

        flags.arm();

        assert!(flags.should_run.load(Ordering::SeqCst));
        assert!(!flags.stop_requested.load(Ordering::SeqCst));
        assert!(!flags.stream_error.load(Ordering::SeqCst));
        assert!(!flags.write_error.load(Ordering::SeqCst));
        assert_eq!(flags.samples_written.load(Ordering::SeqCst), 0);
        assert_eq!(flags.exit_reason(), None);
    }

    #[test]
    fn test_flags_request_stop_and_abort_are_distinguishable() {
        // A stream error must stop the thread WITHOUT looking like a requested shutdown,
        // and must not touch the owner's session state (that is what made `stop()`
        // return None and skip the join).
        let flags = CaptureFlags::new();
        flags.arm();
        flags.is_capturing.store(true, Ordering::SeqCst);

        flags.abort_run();
        assert!(!flags.should_run.load(Ordering::SeqCst));
        assert!(!flags.stop_requested.load(Ordering::SeqCst));
        assert!(flags.is_capturing.load(Ordering::SeqCst));

        flags.arm();
        flags.request_stop();
        assert!(!flags.should_run.load(Ordering::SeqCst));
        assert!(flags.stop_requested.load(Ordering::SeqCst));
    }

    // --- WAV header / size handling ---

    #[test]
    fn test_wav_header_size_is_zero_until_flushed() {
        // Documents why `flush_writer` exists: hound only writes the data chunk length in
        // finalize()/drop, so a WAV that is still being captured advertises 0 bytes of
        // audio. save_clip muxes this file WHILE capture runs, and FFmpeg reads a
        // zero-length data chunk as "no audio" -> silent clip.
        let dir = std::env::temp_dir().join("lolshorts_wav_flush_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("checkpoint.wav");
        let _ = std::fs::remove_file(&path);

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..100i16 {
            writer.write_sample(i).unwrap();
        }

        // Not flushed yet: the file on disk advertises no audio at all — the header may
        // not even have left hound's BufWriter, so the `data` chunk can be missing
        // entirely. Either way a reader (FFmpeg) finds zero bytes of audio.
        assert_eq!(wav_data_chunk_len(&path).unwrap_or(0), 0);

        // After a checkpoint the on-disk file is readable up to this point
        // (100 samples * 2 bytes).
        writer.flush().unwrap();
        assert_eq!(wav_data_chunk_len(&path), Some(200));

        // Capture continues after the checkpoint; finalize covers everything.
        for i in 0..100i16 {
            writer.write_sample(i).unwrap();
        }
        writer.finalize().unwrap();
        assert_eq!(wav_data_chunk_len(&path), Some(400));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_wav_flush_keeps_file_readable_mid_capture() {
        // The complement of the previous test: after a checkpoint the WAV parses as a
        // valid file with the expected sample count, which is what FFmpeg needs when
        // save_clip muxes a still-open capture.
        let dir = std::env::temp_dir().join("lolshorts_wav_readable_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("readable.wav");
        let _ = std::fs::remove_file(&path);

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..64i16 {
            writer.write_sample(i * 100).unwrap();
        }
        writer.flush().unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.len(), 64);
        assert_eq!(reader.spec().sample_rate, 48_000);
        drop(reader);

        writer.finalize().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
