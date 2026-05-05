//! macOS audio device enumeration and capture
//!
//! Uses Core Audio framework for detecting audio devices on macOS

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// macOS audio device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacAudioDevice {
    pub id: u32,
    pub name: String,
    pub device_type: MacAudioDeviceType,
    pub is_input: bool,
    pub is_output: bool,
    pub channels: u32,
    pub sample_rate: f64,
}

/// Audio device type for macOS
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacAudioDeviceType {
    Microphone,
    Speaker,
    Headphones,
    LineIn,
    LineOut,
    Bluetooth,
    USB,
    Unknown,
}

impl MacAudioDevice {
    pub fn is_microphone(&self) -> bool {
        matches!(self.device_type, MacAudioDeviceType::Microphone) && self.is_input
    }

    pub fn is_speaker(&self) -> bool {
        matches!(
            self.device_type,
            MacAudioDeviceType::Speaker | MacAudioDeviceType::Headphones
        ) && self.is_output
    }
}

/// macOS audio device manager
pub struct MacAudioManager {
    devices: Vec<MacAudioDevice>,
}

impl MacAudioManager {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    /// Refresh the list of available audio devices
    pub async fn refresh_devices(&mut self) -> Result<()> {
        self.devices = MacAudioManager::enumerate_audio_devices()?;
        tracing::info!("Found {} macOS audio devices", self.devices.len());
        Ok(())
    }

    /// Get all available audio devices
    pub fn get_devices(&self) -> &[MacAudioDevice] {
        &self.devices
    }

    /// Get only microphone devices
    pub fn get_microphones(&self) -> Vec<&MacAudioDevice> {
        self.devices
            .iter()
            .filter(|device| device.is_microphone())
            .collect()
    }

    /// Get only speaker/output devices
    pub fn get_speakers(&self) -> Vec<&MacAudioDevice> {
        self.devices
            .iter()
            .filter(|device| device.is_speaker())
            .collect()
    }

    /// Find device by ID
    pub fn find_device_by_id(&self, id: u32) -> Option<&MacAudioDevice> {
        self.devices.iter().find(|device| device.id == id)
    }

    /// Find device by name (partial match)
    pub fn find_device_by_name(&self, name: &str) -> Option<&MacAudioDevice> {
        self.devices
            .iter()
            .find(|device| device.name.to_lowercase().contains(&name.to_lowercase()))
    }
}

/// Enumerate audio devices using system_profiler (macOS native)
fn enumerate_audio_devices() -> Result<Vec<MacAudioDevice>> {
    let output = Command::new("system_profiler")
        .args(["SPAudioDataType", "-json"])
        .output()
        .context("Failed to run system_profiler for audio devices")?;

    if !output.status.success() {
        anyhow::bail!("system_profiler command failed");
    }

    let output_str =
        String::from_utf8(output.stdout).context("Invalid UTF-8 output from system_profiler")?;

    // Parse the JSON output
    parse_system_profiler_audio(&output_str)
}

/// Parse system_profiler JSON output for audio devices
fn parse_system_profiler_audio(json_str: &str) -> Result<Vec<MacAudioDevice>> {
    let json_data: serde_json::Value =
        serde_json::from_str(json_str).context("Failed to parse system_profiler JSON")?;

    let mut devices = Vec::new();

    if let Some(audio_data) = json_data.get("SPAudioDataType") {
        if let Some(items) = audio_data.as_array() {
            for item in items {
                if let Some(device) = parse_audio_device(item) {
                    devices.push(device);
                }
            }
        }
    }

    Ok(devices)
}

/// Parse individual audio device from system_profiler data
fn parse_audio_device(json: &serde_json::Value) -> Option<MacAudioDevice> {
    let name = json.get("_name")?.as_str()?;
    let device_id = json.get("_items")?.get(0)?.as_str()?.parse().ok()?;

    // Extract device type information
    let device_type = extract_device_type(json);

    // Check input/output capabilities
    let is_input = json
        .get("spdev_device_inputs")
        .and_then(|inputs| inputs.as_u64().ok())
        .map(|inputs| inputs > 0)
        .unwrap_or(false);

    let is_output = json
        .get("spdev_device_outputs")
        .and_then(|outputs| outputs.as_u64().ok())
        .map(|outputs| outputs > 0)
        .unwrap_or(false);

    // Extract audio properties
    let channels = json
        .get("spdev_device_channel_count")
        .and_then(|ch| ch.as_u64().ok())
        .map(|ch| ch as u32)
        .unwrap_or(2); // Default to stereo

    let sample_rate = json
        .get("spdev_device_nominal_samplerate")
        .and_then(|sr| sr.as_f64().ok())
        .unwrap_or(44100.0); // Default to 44.1kHz

    Some(MacAudioDevice {
        id: device_id,
        name: name.to_string(),
        device_type,
        is_input,
        is_output,
        channels,
        sample_rate,
    })
}

/// Extract device type from system_profiler data
fn extract_device_type(json: &serde_json::Value) -> MacAudioDeviceType {
    let transport_type = json
        .get("spdev_device_transport_type")
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let device_name = json.get("_name").and_then(|n| n.as_str()).unwrap_or("");

    // Try to determine device type from transport type and name
    if transport_type == "Bluetooth" || device_name.to_lowercase().contains("bluetooth") {
        MacAudioDeviceType::Bluetooth
    } else if transport_type == "USB" || device_name.to_lowercase().contains("usb") {
        MacAudioDeviceType::USB
    } else if device_name.to_lowercase().contains("microphone")
        || device_name.to_lowercase().contains("mic")
    {
        MacAudioDeviceType::Microphone
    } else if device_name.to_lowercase().contains("speaker")
        || device_name.to_lowercase().contains("output")
    {
        MacAudioDeviceType::Speaker
    } else if device_name.to_lowercase().contains("headphone")
        || device_name.to_lowercase().contains("headset")
    {
        MacAudioDeviceType::Headphones
    } else if device_name.to_lowercase().contains("line in") {
        MacAudioDeviceType::LineIn
    } else if device_name.to_lowercase().contains("line out") {
        MacAudioDeviceType::LineOut
    } else {
        MacAudioDeviceType::Unknown
    }
}

/// Alternative method using afinfo (if system_profiler fails)
fn enumerate_with_afinfo() -> Result<Vec<MacAudioDevice>> {
    let output = Command::new("afinfo")
        .arg("-a")
        .output()
        .context("Failed to run afinfo for audio devices")?;

    if !output.status.success() {
        anyhow::bail!("afinfo command failed");
    }

    let output_str =
        String::from_utf8(output.stdout).context("Invalid UTF-8 output from afinfo")?;

    parse_afinfo_devices(&output_str)
}

/// Parse afinfo output for audio devices (fallback method)
fn parse_afinfo_devices(output: &str) -> Result<Vec<MacAudioDevice>> {
    let mut devices = Vec::new();
    let mut current_device: Option<MacAudioDevice> = None;
    let mut device_id_counter = 0;

    for line in output.lines() {
        let line = line.trim();

        if line.starts_with("Device ") {
            // New device found
            if let Some(device) = current_device.take() {
                devices.push(device);
            }

            // Extract device name
            if let Some(name_start) = line.find('\"') {
                if let Some(name_end) = line[name_start + 1..].find('\"') {
                    let name = &line[name_start + 1..name_end];
                    current_device = Some(MacAudioDevice {
                        id: device_id_counter,
                        name: name.to_string(),
                        device_type: MacAudioDeviceType::Unknown,
                        is_input: false,
                        is_output: false,
                        channels: 2,
                        sample_rate: 44100.0,
                    });
                    device_id_counter += 1;
                }
            }
        }
    }

    // Add the last device if exists
    if let Some(device) = current_device {
        devices.push(device);
    }

    Ok(devices)
}

/// Public interface for audio device enumeration
pub async fn list_audio_devices() -> Result<Vec<MacAudioDevice>> {
    // Try system_profiler first (preferred method)
    match enumerate_audio_devices() {
        Ok(devices) => {
            if devices.is_empty() {
                tracing::warn!("No audio devices found with system_profiler, trying afinfo");
                // Fallback to afinfo
                enumerate_with_afinfo()
            } else {
                Ok(devices)
            }
        }
        Err(e) => {
            tracing::warn!("system_profiler failed, trying afinfo: {}", e);
            // Fallback to afinfo
            enumerate_with_afinfo()
        }
    }
}

/// Test audio device enumeration
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_type_extraction() {
        let json = serde_json::json!({
            "_name": "External Microphone",
            "spdev_device_transport_type": "USB",
            "spdev_device_inputs": 1,
            "spdev_device_outputs": 0,
            "spdev_device_channel_count": 2,
            "spdev_device_nominal_samplerate": 48000.0
        });

        let device = parse_audio_device(&json).unwrap();
        assert_eq!(device.device_type, MacAudioDeviceType::USB);
        assert!(device.is_microphone());
        assert!(!device.is_speaker());
        assert_eq!(device.channels, 2);
        assert_eq!(device.sample_rate, 48000.0);
    }

    #[test]
    fn test_device_type_bluetooth() {
        let json = serde_json::json!({
            "_name": "AirPods Pro",
            "spdev_device_transport_type": "Bluetooth",
            "spdev_device_inputs": 0,
            "spdev_device_outputs": 1,
            "spdev_device_channel_count": 2,
            "spdev_device_nominal_samplerate": 44100.0
        });

        let device = parse_audio_device(&json).unwrap();
        assert_eq!(device.device_type, MacAudioDeviceType::Bluetooth);
        assert!(!device.is_microphone());
        assert!(device.is_speaker());
    }

    #[test]
    fn test_device_type_unknown() {
        let json = serde_json::json!({
            "_name": "Unknown Device",
            "spdev_device_transport_type": "Internal",
            "spdev_device_inputs": 1,
            "spdev_device_outputs": 1,
            "spdev_device_channel_count": 2,
            "spdev_device_nominal_samplerate": 44100.0
        });

        let device = parse_audio_device(&json).unwrap();
        assert_eq!(device.device_type, MacAudioDeviceType::Unknown);
    }

    #[test]
    fn test_audio_manager_initialization() {
        let manager = MacAudioManager::new();
        assert_eq!(manager.get_devices().len(), 0);
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_audio_device_enumeration() {
        // This test would require actual macOS environment
        let devices = list_audio_devices().await;

        // Should find at least some devices on macOS
        if !devices.is_empty() {
            println!("Found {} audio devices:", devices.len());
            for device in devices {
                println!("  - {} ({})", device.name, device.device_type);
            }
        }
    }
}
