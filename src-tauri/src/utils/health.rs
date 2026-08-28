use serde::Serialize;

use crate::public_service_config::{PublicServiceStatus, ServiceConfigStatus};

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

fn public_config_check(
    key: &'static str,
    label: &'static str,
    status: &ServiceConfigStatus,
) -> DiagnosticCheck {
    if status.configured {
        diagnostic_check(
            key,
            label,
            DiagnosticState::Ok,
            format!("{label} is configured for this build."),
            "No action required.",
        )
    } else {
        diagnostic_check(
            key,
            label,
            DiagnosticState::Warning,
            format!(
                "{label} is unavailable ({}).",
                status
                    .error_code
                    .as_deref()
                    .unwrap_or("PUBLIC_CONFIG_MISSING")
            ),
            "Install a production build with the required public client configuration embedded.",
        )
    }
}

fn optional_public_config_check(
    key: &'static str,
    label: &'static str,
    status: &ServiceConfigStatus,
) -> DiagnosticCheck {
    if status.configured {
        diagnostic_check(
            key,
            label,
            DiagnosticState::Ok,
            format!("{label} is configured for this build."),
            "No action required.",
        )
    } else {
        diagnostic_check(
            key,
            label,
            DiagnosticState::Warning,
            format!(
                "{label} is not configured; this optional service is disabled ({}).",
                status
                    .error_code
                    .as_deref()
                    .unwrap_or("OPTIONAL_PUBLIC_CONFIG_MISSING")
            ),
            "Optional: set SENTRY_DSN/VITE_SENTRY_DSN in the release environment to enable anonymous crash reports.",
        )
    }
}

pub fn get_diagnostics_status(public_status: &PublicServiceStatus) -> DiagnosticsStatus {
    let checks = vec![
        public_config_check(
            "release_config",
            "Public release configuration",
            &public_status.release_config,
        ),
        public_config_check(
            "supabase_config",
            "Supabase public client",
            &public_status.supabase,
        ),
        public_config_check(
            "youtube_config",
            "YouTube desktop OAuth",
            &public_status.youtube,
        ),
        public_config_check(
            "updater_pubkey",
            "Updater public key",
            &public_status.updater,
        ),
        optional_public_config_check(
            "telemetry_config",
            "Anonymous error telemetry (optional)",
            &public_status.telemetry,
        ),
    ];

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
        let public_status = crate::public_service_config::PublicServiceConfig::load().status();
        let status = get_diagnostics_status(&public_status);

        assert!(status
            .checks
            .iter()
            .any(|check| check.key == "release_config"));
        assert!(status
            .checks
            .iter()
            .any(|check| check.key == "telemetry_config"));
        assert!(status
            .checks
            .iter()
            .any(|check| check.key == "updater_pubkey"));
        assert!(status
            .checks
            .iter()
            .any(|check| check.key == "youtube_config"));
    }

    #[test]
    fn diagnostics_status_serializes_actionable_fields() {
        let public_status = crate::public_service_config::PublicServiceConfig::load().status();
        let status = get_diagnostics_status(&public_status);
        let json = serde_json::to_string(&status).unwrap();

        assert!(json.contains("overall_status"));
        assert!(json.contains("checks"));
        assert!(json.contains("message"));
        assert!(json.contains("action"));
    }
}
