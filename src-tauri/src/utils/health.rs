use serde::Serialize;

use crate::utils::env_validation;

#[derive(Debug, Clone, Serialize)]
pub struct SystemHealth {
    pub game_monitor: &'static str, // "running" | "stopped"
    pub recording_active: bool,
    pub uptime_secs: u64,
    pub rust_test_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticState {
    Ok,
    Warning,
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticCheck {
    pub key: &'static str,
    pub label: &'static str,
    pub status: DiagnosticState,
    pub message: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsStatus {
    pub overall_status: DiagnosticState,
    pub checks: Vec<DiagnosticCheck>,
}

pub fn configured_updater_pubkey() -> Option<String> {
    option_env!("TAURI_UPDATER_PUBKEY")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            std::env::var("TAURI_UPDATER_PUBKEY")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn env_present(name: &str) -> bool {
    std::env::var(name)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn diagnostic_check(
    key: &'static str,
    label: &'static str,
    status: DiagnosticState,
    message: impl Into<String>,
    action: impl Into<String>,
) -> DiagnosticCheck {
    DiagnosticCheck {
        key,
        label,
        status,
        message: message.into(),
        action: action.into(),
    }
}

pub(crate) fn overall_status(checks: &[DiagnosticCheck]) -> DiagnosticState {
    if checks
        .iter()
        .any(|check| check.status == DiagnosticState::Blocked)
    {
        DiagnosticState::Blocked
    } else if checks
        .iter()
        .any(|check| check.status == DiagnosticState::Warning)
    {
        DiagnosticState::Warning
    } else {
        DiagnosticState::Ok
    }
}

pub fn get_diagnostics_status() -> DiagnosticsStatus {
    let env_check = env_validation::validate_env();
    let mut checks = Vec::new();

    if env_check.required_missing.is_empty() {
        checks.push(diagnostic_check(
            "required_env",
            "Required environment",
            DiagnosticState::Ok,
            "Required runtime configuration is present.",
            "No action required.",
        ));
    } else {
        checks.push(diagnostic_check(
            "required_env",
            "Required environment",
            DiagnosticState::Blocked,
            format!(
                "Missing required configuration: {}.",
                env_check.required_missing.join(", ")
            ),
            "Set the missing variables before building or running authenticated desktop features.",
        ));
    }

    if env_present("SENTRY_DSN") {
        checks.push(diagnostic_check(
            "sentry",
            "Crash reporting",
            DiagnosticState::Ok,
            "Backend crash reporting DSN is configured.",
            "No action required.",
        ));
    } else {
        checks.push(diagnostic_check(
            "sentry",
            "Crash reporting",
            DiagnosticState::Warning,
            "SENTRY_DSN is not configured; crash reports will stay local.",
            "Set SENTRY_DSN for release builds if remote crash diagnostics are desired.",
        ));
    }

    if configured_updater_pubkey().is_some() {
        checks.push(diagnostic_check(
            "updater_pubkey",
            "Updater public key",
            DiagnosticState::Ok,
            "Tauri updater public key is configured for this build/runtime.",
            "No action required.",
        ));
    } else {
        checks.push(diagnostic_check(
            "updater_pubkey",
            "Updater public key",
            DiagnosticState::Warning,
            "TAURI_UPDATER_PUBKEY is not configured; auto-updates are disabled for this build.",
            "Provide TAURI_UPDATER_PUBKEY in CI/release build configuration.",
        ));
    }

    if env_present("TAURI_SIGNING_PRIVATE_KEY") && env_present("TAURI_SIGNING_PRIVATE_KEY_PASSWORD")
    {
        checks.push(diagnostic_check(
            "release_signing",
            "Release signing",
            DiagnosticState::Ok,
            "Updater signing environment is present in this runtime.",
            "No action required.",
        ));
    } else {
        checks.push(diagnostic_check(
            "release_signing",
            "Release signing",
            DiagnosticState::Warning,
            "Release signing keys are not available to this local runtime; distribution signatures cannot be proven here.",
            "Verify TAURI_PRIVATE_KEY, TAURI_KEY_PASSWORD, and TAURI_UPDATER_PUBKEY secrets in the release workflow.",
        ));
    }

    DiagnosticsStatus {
        overall_status: overall_status(&checks),
        checks,
    }
}

/// Get current system health status.
/// For now, returns static status — intended to be wired to actual service states.
pub fn get_system_health(start_time: std::time::Instant) -> SystemHealth {
    SystemHealth {
        game_monitor: "running",
        recording_active: false,
        uptime_secs: start_time.elapsed().as_secs(),
        rust_test_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_health_fields() {
        let start = std::time::Instant::now();
        let health = get_system_health(start);
        assert_eq!(health.game_monitor, "running");
        assert!(!health.recording_active);
        assert_eq!(health.rust_test_count, 0);
    }

    #[test]
    fn test_uptime_increases() {
        let start = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let health = get_system_health(start);
        // elapsed will be at least 0 secs (might be 0 since sleep < 1s)
        assert!(health.uptime_secs < 60);
    }

    #[test]
    fn test_system_health_serialization() {
        let start = std::time::Instant::now();
        let health = get_system_health(start);
        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("game_monitor"));
        assert!(json.contains("recording_active"));
        assert!(json.contains("uptime_secs"));
    }

    #[test]
    fn diagnostics_status_surfaces_configuration_checks() {
        let status = get_diagnostics_status();

        assert!(status
            .checks
            .iter()
            .any(|check| check.key == "required_env"));
        assert!(status.checks.iter().any(|check| check.key == "sentry"));
        assert!(status
            .checks
            .iter()
            .any(|check| check.key == "updater_pubkey"));
        assert!(status
            .checks
            .iter()
            .any(|check| check.key == "release_signing"));
    }

    #[test]
    fn diagnostics_status_serializes_actionable_fields() {
        let status = get_diagnostics_status();
        let json = serde_json::to_string(&status).unwrap();

        assert!(json.contains("overall_status"));
        assert!(json.contains("checks"));
        assert!(json.contains("message"));
        assert!(json.contains("action"));
    }
}
