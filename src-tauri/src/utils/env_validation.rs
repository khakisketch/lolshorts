use serde::Serialize;

#[derive(Debug, Default, Serialize)]
pub struct EnvValidationResult {
    pub required_missing: Vec<String>,
    pub optional_missing: Vec<String>,
}

/// Validate environment variables at startup.
/// Returns missing required/optional vars. Does not block startup.
pub fn validate_env() -> EnvValidationResult {
    let mut result = EnvValidationResult::default();

    // Required for auth
    for var in ["SUPABASE_URL", "SUPABASE_ANON_KEY"] {
        if std::env::var(var).unwrap_or_default().is_empty() {
            result.required_missing.push(var.to_string());
        }
    }

    // Optional runtime integrations. YouTube OAuth is read by the Rust backend,
    // not by Vite, so these names must match init_youtube_manager.
    for var in [
        "SENTRY_DSN",
        "YOUTUBE_CLIENT_ID",
        "YOUTUBE_CLIENT_SECRET",
        "YOUTUBE_REDIRECT_URI",
    ] {
        if std::env::var(var).unwrap_or_default().is_empty() {
            result.optional_missing.push(var.to_string());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_env_returns_result() {
        // Should not panic regardless of environment
        let result = validate_env();
        // required_missing may contain vars if not set in test env
        let _ = result.required_missing;
        let _ = result.optional_missing;
    }
}
