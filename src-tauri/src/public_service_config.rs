use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine as _,
};
use serde::{Deserialize, Serialize};

use crate::supabase::SupabaseConfig;

const DEFAULT_YOUTUBE_REDIRECT_URI: &str = "http://localhost:9090/oauth/callback";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceConfigStatus {
    pub configured: bool,
    pub error_code: Option<String>,
}

impl ServiceConfigStatus {
    fn configured() -> Self {
        Self {
            configured: true,
            error_code: None,
        }
    }

    fn missing(error_code: &'static str) -> Self {
        Self {
            configured: false,
            error_code: Some(error_code.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicServiceStatus {
    pub release_config: ServiceConfigStatus,
    pub supabase: ServiceConfigStatus,
    pub youtube: ServiceConfigStatus,
    pub updater: ServiceConfigStatus,
    pub telemetry: ServiceConfigStatus,
}

/// Public configuration embedded into an installed desktop build.
///
/// These values are client-side identifiers, not privileged server secrets. In a
/// release build they must be supplied at compile time so an installed app never
/// depends on a user-owned `.env`. Debug builds may fall back to runtime variables
/// to keep local development convenient.
#[derive(Debug, Clone)]
pub struct PublicServiceConfig {
    supabase_url: Option<String>,
    supabase_anon_key: Option<String>,
    youtube_client_id: Option<String>,
    youtube_client_secret: Option<String>,
    youtube_redirect_uri: Option<String>,
    updater_pubkey: Option<String>,
    /// Optional crash-reporting destination. The app remains fully usable when
    /// this is absent; the release gate only requires the public service keys
    /// needed for online features and signed updates.
    sentry_dsn: Option<String>,
}

impl PublicServiceConfig {
    pub fn load() -> Self {
        Self {
            supabase_url: configured_value(option_env!("SUPABASE_URL"), "SUPABASE_URL"),
            supabase_anon_key: configured_value(
                option_env!("SUPABASE_ANON_KEY"),
                "SUPABASE_ANON_KEY",
            )
            .filter(|value| valid_supabase_anon_key(value)),
            youtube_client_id: configured_value(
                option_env!("YOUTUBE_CLIENT_ID"),
                "YOUTUBE_CLIENT_ID",
            )
            .filter(|value| {
                !value.contains("your-client-id") && value.ends_with(".apps.googleusercontent.com")
            }),
            youtube_client_secret: configured_value(
                option_env!("YOUTUBE_CLIENT_SECRET"),
                "YOUTUBE_CLIENT_SECRET",
            )
            .filter(|value| !value.contains("your-client-secret")),
            youtube_redirect_uri: configured_value(
                option_env!("YOUTUBE_REDIRECT_URI"),
                "YOUTUBE_REDIRECT_URI",
            )
            .or_else(|| Some(DEFAULT_YOUTUBE_REDIRECT_URI.to_string()))
            .filter(|value| valid_loopback_redirect(value)),
            updater_pubkey: configured_value(
                option_env!("TAURI_UPDATER_PUBKEY"),
                "TAURI_UPDATER_PUBKEY",
            ),
            sentry_dsn: configured_value(option_env!("SENTRY_DSN"), "SENTRY_DSN")
                .filter(|value| value.parse::<sentry::types::Dsn>().is_ok()),
        }
    }

    pub fn status(&self) -> PublicServiceStatus {
        let supabase = if self.supabase_config().is_some() {
            ServiceConfigStatus::configured()
        } else {
            ServiceConfigStatus::missing("SUPABASE_PUBLIC_CONFIG_MISSING")
        };
        let youtube = if self.youtube_credentials().is_some() {
            ServiceConfigStatus::configured()
        } else {
            ServiceConfigStatus::missing("YOUTUBE_PUBLIC_CONFIG_MISSING")
        };
        let updater = if self.updater_pubkey.is_some() {
            ServiceConfigStatus::configured()
        } else {
            ServiceConfigStatus::missing("UPDATER_PUBLIC_KEY_MISSING")
        };
        let telemetry = if self.sentry_dsn.is_some() {
            ServiceConfigStatus::configured()
        } else {
            ServiceConfigStatus::missing("TELEMETRY_OPTIONAL_NOT_CONFIGURED")
        };
        // Sentry is an opt-in operational aid, not a product dependency. A
        // production build without a DSN must still expose recording,
        // library, account, YouTube, and updater functionality.
        let release_config = if supabase.configured && youtube.configured && updater.configured {
            ServiceConfigStatus::configured()
        } else {
            ServiceConfigStatus::missing("PUBLIC_RELEASE_CONFIG_INCOMPLETE")
        };

        PublicServiceStatus {
            release_config,
            supabase,
            youtube,
            updater,
            telemetry,
        }
    }

    pub fn supabase_config(&self) -> Option<SupabaseConfig> {
        let project_url = self.supabase_url.clone()?;
        let anon_key = self.supabase_anon_key.clone()?;
        if !valid_supabase_url(&project_url) || !valid_supabase_anon_key(&anon_key) {
            return None;
        }
        Some(SupabaseConfig::new(project_url, anon_key))
    }

    pub fn youtube_credentials(&self) -> Option<(String, String, String)> {
        Some((
            self.youtube_client_id.clone()?,
            self.youtube_client_secret.clone()?,
            self.youtube_redirect_uri.clone()?,
        ))
    }

    pub fn updater_pubkey(&self) -> Option<String> {
        self.updater_pubkey.clone()
    }

    pub fn sentry_dsn(&self) -> Option<String> {
        self.sentry_dsn.clone()
    }
}

fn configured_value(compile_time: Option<&'static str>, runtime_name: &str) -> Option<String> {
    normalized(compile_time.map(ToOwned::to_owned)).or_else(|| {
        if cfg!(debug_assertions) {
            normalized(std::env::var(runtime_name).ok())
        } else {
            None
        }
    })
}

fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn valid_supabase_url(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return false;
    }
    let (scheme, rest) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        ("http", rest)
    } else {
        return false;
    };
    if rest.contains(['?', '#']) {
        return false;
    }
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() || authority.starts_with('.') || authority.contains('@') {
        return false;
    }

    if scheme == "http" {
        return cfg!(debug_assertions) && valid_localhost_authority(authority);
    }

    // A production URL must not contain credentials or an explicit port. The
    // project endpoint is a public HTTPS origin; rejecting both prevents an
    // accidentally malformed value such as `https://project@attacker.example`
    // from changing where auth and REST requests are sent.
    !authority.contains(':')
}

fn valid_localhost_authority(authority: &str) -> bool {
    let (host, port) = authority
        .split_once(':')
        .map_or((authority, None), |(host, port)| (host, Some(port)));
    matches!(host, "localhost" | "127.0.0.1")
        && port.is_none_or(|port| port.parse::<u16>().is_ok_and(|port| port > 0))
}

/// Reject privileged Supabase credentials before they can be embedded in a
/// desktop client. Opaque anon keys remain supported; JWT-shaped keys are
/// inspected only for the role claim and their value is never logged.
fn valid_supabase_anon_key(value: &str) -> bool {
    let normalized = value.trim();
    if normalized.is_empty() {
        return false;
    }
    let lower = normalized.to_ascii_lowercase();
    if lower.contains("service_role")
        || lower.contains("service-role")
        || lower.starts_with("sb_secret_")
        || lower.contains("your-anon-key")
    {
        return false;
    }

    let mut parts = normalized.split('.');
    let _header = parts.next();
    let payload = parts.next();
    let signature = parts.next();
    if payload.is_none() || signature.is_none() || parts.next().is_some() {
        return true;
    }

    let Some(payload) = payload else {
        return true;
    };
    let padded_payload = format!("{}{}", payload, "=".repeat((4 - (payload.len() % 4)) % 4));
    let Ok(decoded) = URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(padded_payload.as_bytes()))
    else {
        return true;
    };
    let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&decoded) else {
        return true;
    };
    claims.get("role").and_then(serde_json::Value::as_str) != Some("service_role")
}

pub fn valid_loopback_redirect(uri: &str) -> bool {
    if uri.chars().any(char::is_whitespace) {
        return false;
    }
    let Some(rest) = uri.strip_prefix("http://") else {
        return false;
    };
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    let Some((host, port)) = authority.split_once(':') else {
        return false;
    };

    matches!(host, "localhost" | "127.0.0.1")
        && port.parse::<u16>().is_ok_and(|port| port > 0)
        && !path.is_empty()
        && path != "/"
        && !path.contains(['?', '#'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_config() -> PublicServiceConfig {
        PublicServiceConfig {
            supabase_url: Some("https://example.supabase.co".to_string()),
            supabase_anon_key: Some("public-anon-key".to_string()),
            youtube_client_id: Some("client.apps.googleusercontent.com".to_string()),
            youtube_client_secret: Some("desktop-public-secret".to_string()),
            youtube_redirect_uri: Some(DEFAULT_YOUTUBE_REDIRECT_URI.to_string()),
            updater_pubkey: Some("public-updater-key".to_string()),
            sentry_dsn: Some("https://public@example.ingest.sentry.io/1".to_string()),
        }
    }

    #[test]
    fn status_exposes_flags_and_codes_without_values() {
        let status = complete_config().status();
        let json = serde_json::to_string(&status).unwrap();

        assert!(status.release_config.configured);
        assert!(!json.contains("public-anon-key"));
        assert!(!json.contains("desktop-public-secret"));
        assert!(!json.contains("public-updater-key"));
        assert!(!json.contains("example.ingest.sentry.io"));
    }

    #[test]
    fn incomplete_config_fails_closed() {
        let mut config = complete_config();
        config.youtube_client_secret = None;

        let status = config.status();
        assert!(!status.release_config.configured);
        assert_eq!(
            status.youtube.error_code.as_deref(),
            Some("YOUTUBE_PUBLIC_CONFIG_MISSING")
        );
    }

    #[test]
    fn privileged_supabase_key_fails_closed_without_exposing_value() {
        let mut config = complete_config();
        config.supabase_anon_key =
            Some("eyJhbGciOiJIUzI1NiJ9.eyJyb2xlIjoic2VydmljZV9yb2xlIn0.signature".to_string());

        let status = config.status();

        assert!(!status.supabase.configured);
        assert!(!status.release_config.configured);
    }

    #[test]
    fn padded_jwt_service_role_key_fails_closed() {
        let mut config = complete_config();
        config.supabase_anon_key =
            Some("eyJhbGciOiJIUzI1NiJ9.eyJyb2xlIjoic2VydmljZV9yb2xlIn0=.signature".to_string());

        let status = config.status();

        assert!(!status.supabase.configured);
        assert!(!status.release_config.configured);
    }

    #[test]
    fn invalid_release_supabase_url_fails_closed() {
        let mut config = complete_config();
        config.supabase_url = Some("http://example.supabase.co".to_string());

        let status = config.status();

        assert!(!status.supabase.configured);
        assert!(!status.release_config.configured);
    }

    #[test]
    fn supabase_url_rejects_credentials_and_query_components() {
        let mut config = complete_config();
        config.supabase_url = Some("https://project.supabase.co@attacker.example".to_string());
        assert!(!config.status().supabase.configured);

        config.supabase_url = Some("https://project.supabase.co?tenant=attacker".to_string());
        assert!(!config.status().supabase.configured);
    }

    #[test]
    fn empty_supabase_authority_fails_closed() {
        let mut config = complete_config();
        config.supabase_url = Some("https://".to_string());

        let status = config.status();

        assert!(!status.supabase.configured);
        assert!(!status.release_config.configured);
    }

    #[test]
    fn missing_optional_telemetry_does_not_block_release_configuration() {
        let mut config = complete_config();
        config.sentry_dsn = None;

        let status = config.status();

        assert!(status.release_config.configured);
        assert!(!status.telemetry.configured);
        assert_eq!(
            status.telemetry.error_code.as_deref(),
            Some("TELEMETRY_OPTIONAL_NOT_CONFIGURED")
        );
    }

    #[test]
    fn redirect_requires_an_http_loopback_origin() {
        assert!(valid_loopback_redirect(DEFAULT_YOUTUBE_REDIRECT_URI));
        assert!(valid_loopback_redirect("http://127.0.0.1:8080/callback"));
        assert!(!valid_loopback_redirect("https://localhost/callback"));
        assert!(!valid_loopback_redirect(
            "http://localhost.example/callback"
        ));
        assert!(!valid_loopback_redirect("http://127.0.0.1:bad/callback"));
        assert!(!valid_loopback_redirect("http://localhost/callback"));
        assert!(!valid_loopback_redirect("http://localhost:65536/callback"));
        assert!(!valid_loopback_redirect("http://localhost:9090/"));
        assert!(!valid_loopback_redirect(
            "http://localhost:9090/callback?code=1"
        ));
    }
}
