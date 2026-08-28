use std::sync::atomic::{AtomicBool, Ordering};

/// Runtime gate for the optional operational telemetry client.
///
/// The Sentry guard is created during application startup, but the user can
/// change the privacy setting while the app is running. Keeping the gate
/// separate from the client lifetime lets us honour an opt-out immediately
/// without tearing down a global Sentry client from a Tauri command.
static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);

/// Update the in-process telemetry preference.
pub fn set_enabled(enabled: bool) {
    TELEMETRY_ENABLED.store(enabled, Ordering::Release);
}

pub fn is_enabled() -> bool {
    TELEMETRY_ENABLED.load(Ordering::Acquire)
}

/// Send a privacy-minimal operational signal.
///
/// Only compile-time categories and error codes are accepted. File paths, user
/// identifiers, OAuth data, command arguments, and error messages are never
/// attached to the event.
pub fn capture_operational_error(category: &'static str, error_code: &'static str) {
    // A configured DSN is not consent. The persisted user preference is
    // applied by `main` and updated by settings commands; keep this check at
    // the final send boundary so every caller follows the same policy.
    if !TELEMETRY_ENABLED.load(Ordering::Acquire) {
        return;
    }

    sentry::with_scope(
        |scope| {
            scope.set_tag("lolshorts.category", category);
            scope.set_tag("lolshorts.error_code", error_code);
        },
        || {
            sentry::capture_message("lolshorts_operational_error", sentry::Level::Error);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_gate_can_be_toggled() {
        set_enabled(false);
        assert!(!is_enabled());

        set_enabled(true);
        assert!(is_enabled());

        // Leave the process in the privacy-safe default for the remaining
        // tests, which may call operational error paths in parallel.
        set_enabled(false);
    }
}
