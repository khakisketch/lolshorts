use anyhow::{Context, Result};
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};
use tracing::{debug, info, warn};
use warp::Filter;

/// OAuth callback result
#[derive(Debug)]
pub struct OAuthCallback {
    pub code: String,
    pub state: String,
}

/// Outcome sent from the HTTP handler to the waiting [`CallbackServer::start_and_wait`]
/// caller: either the parsed `code`/`state` pair, or a description of why the
/// authorization did not succeed (the user declined consent — Google redirects
/// with `?error=access_denied` and no `code`/`state` — or the callback was
/// otherwise malformed).
type CallbackOutcome = Result<OAuthCallback, String>;

/// OAuth callback server for handling Google OAuth redirects
pub struct CallbackServer {
    port: u16,
    /// Path the callback must arrive on, always starting with `/` (e.g. `/oauth/callback`).
    path: String,
    callback_tx: Arc<RwLock<Option<oneshot::Sender<CallbackOutcome>>>>,
    shutdown_tx: Arc<RwLock<Option<oneshot::Sender<()>>>>,
}

impl CallbackServer {
    /// Create new callback server bound to an explicit port/path.
    ///
    /// Prefer [`CallbackServer::from_redirect_uri`] in production code so the
    /// listener always stays in sync with the configured OAuth redirect URI
    /// (single source of truth). This constructor exists for tests and any
    /// caller that has already validated a port/path pair.
    ///
    /// # Arguments
    /// * `port` - Port to listen on
    /// * `path` - Path to accept the callback on (leading `/` optional, e.g. "oauth/callback")
    pub fn new(port: u16, path: impl Into<String>) -> Self {
        let raw_path = path.into();
        let normalized = format!("/{}", raw_path.trim_matches('/'));
        Self {
            port,
            path: normalized,
            callback_tx: Arc::new(RwLock::new(None)),
            shutdown_tx: Arc::new(RwLock::new(None)),
        }
    }

    /// Build a callback server whose port and path are derived from the
    /// configured `YOUTUBE_REDIRECT_URI`. This is the SSOT for where the
    /// local OAuth listener binds: the redirect URI handed to Google and the
    /// server that actually receives the callback can never drift apart.
    pub fn from_redirect_uri(redirect_uri: &str) -> Result<Self> {
        let (port, path) = parse_redirect_uri(redirect_uri)?;
        Ok(Self::new(port, path))
    }

    /// Start the callback server and wait for OAuth callback
    ///
    /// Returns the authorization code and state when received
    pub async fn start_and_wait(&self) -> Result<OAuthCallback> {
        info!("Starting OAuth callback server on port {}", self.port);

        // Create oneshot channel for callback
        let (tx, rx) = oneshot::channel();
        {
            let mut callback_tx = self.callback_tx.write().await;
            *callback_tx = Some(tx);
        }

        // Create shutdown channel for graceful server termination
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        {
            let mut stored_shutdown = self.shutdown_tx.write().await;
            *stored_shutdown = Some(shutdown_tx);
        }

        // Clone Arc for the route handler
        let callback_tx_clone = Arc::clone(&self.callback_tx);
        let shutdown_tx_clone = Arc::clone(&self.shutdown_tx);
        let expected_path = self.path.clone();

        // Define callback route: only the exact path derived from YOUTUBE_REDIRECT_URI
        // (SSOT, see `from_redirect_uri`) is accepted; everything else is rejected.
        //
        // `CallbackParams` fields are all `Option` so this deserializes both shapes
        // Google can send: `code`+`state` on approval, or just `error` (e.g.
        // `access_denied`) when the user declines consent. Previously `code`/`state`
        // were required, so a decline redirect failed `warp::query` deserialization
        // outright, the route never matched, and the flow silently sat until the
        // 120s timeout in `start_and_wait` before `youtube-auth-failed` fired.
        let callback_route = warp::path::full()
            .and(warp::query::<CallbackParams>())
            .and_then(
                move |full_path: warp::path::FullPath, params: CallbackParams| {
                    let callback_tx = Arc::clone(&callback_tx_clone);
                    let shutdown_tx = Arc::clone(&shutdown_tx_clone);
                    let expected_path = expected_path.clone();

                    async move {
                        if full_path.as_str() != expected_path {
                            return Err(warp::reject::not_found());
                        }

                        let outcome = interpret_callback_params(params);
                        let response_html = if outcome.is_ok() {
                            SUCCESS_HTML
                        } else {
                            CANCELLED_HTML
                        };

                        tokio::spawn(async move {
                            // Send callback result (success or rejection/error)
                            let mut tx_lock = callback_tx.write().await;
                            if let Some(tx) = tx_lock.take() {
                                let _ = tx.send(outcome);
                            }

                            // Signal server shutdown after small delay for response
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            let mut shutdown_lock = shutdown_tx.write().await;
                            if let Some(tx) = shutdown_lock.take() {
                                let _ = tx.send(());
                            }
                        });

                        Ok::<_, warp::Rejection>(warp::reply::html(response_html))
                    }
                },
            );

        // Bind to address.
        //
        // This always binds the IPv4 loopback (127.0.0.1), even though
        // `YOUTUBE_REDIRECT_URI` (validated in `parse_redirect_uri`) may say
        // `localhost` rather than `127.0.0.1`. `localhost` can in principle
        // resolve to the IPv6 loopback (`::1`) instead, which would make the
        // browser's request never reach an IPv4-only listener. This is left
        // as-is intentionally: we have not observed this failure in the field
        // (the browsers/OS resolvers this app targets consistently prefer/try
        // 127.0.0.1 for `localhost`), and switching the bind based on the
        // parsed host — or binding both stacks — is exactly the kind of
        // change that should be driven by a reproduced report, not spec
        // speculation. See task P2 item 4.
        let addr = (Ipv4Addr::new(127, 0, 0, 1), self.port);

        debug!(
            "Callback server listening on http://localhost:{}{}",
            self.port, self.path
        );

        // Start server with graceful shutdown signal
        let server = warp::serve(callback_route);
        let (_, server_task) = server.bind_with_graceful_shutdown(addr, async move {
            // Wait for shutdown signal
            let _ = shutdown_rx.await;
            info!("OAuth callback server shutting down gracefully");
        });

        let server_handle = tokio::spawn(server_task);

        // Wait for callback with timeout (2 minutes). A rejection (e.g. the user
        // declined consent) resolves `rx` immediately via `interpret_callback_params`
        // above, so this returns well before the timeout in that case — the
        // `map_err` below turns it into an `Err` here so `youtube_start_auth_with_server`
        // can emit `youtube-auth-failed` right away instead of waiting 2 minutes.
        let callback = tokio::time::timeout(std::time::Duration::from_secs(120), rx)
            .await
            .context("OAuth callback timeout (2 minutes)")?
            .context("Failed to receive OAuth callback")?
            .map_err(|reason| anyhow::anyhow!("OAuth authorization was not granted: {}", reason))?;

        info!("OAuth callback received successfully");

        // Give server time to send response and shutdown
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Ensure server task completes
        match tokio::time::timeout(std::time::Duration::from_secs(2), server_handle).await {
            Ok(Ok(())) => debug!("OAuth server shutdown complete"),
            Ok(Err(e)) => warn!("OAuth server task error: {}", e),
            Err(_) => warn!("OAuth server shutdown timeout"),
        }

        Ok(callback)
    }
}

/// Query parameters from OAuth callback.
///
/// All fields are `Option` because Google sends two different shapes on this
/// same redirect URI: `code`+`state` when the user approves, or just `error`
/// (e.g. `access_denied`, `admin_policy_enforced`) with no `code`/`state` at
/// all when they decline. Making `code`/`state` required previously caused
/// `warp::query::<CallbackParams>()` to reject decline redirects outright.
#[derive(Debug, serde::Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Turn parsed callback query parameters into a [`CallbackOutcome`].
///
/// An `error` parameter (present whenever the user declines consent or Google
/// otherwise refuses the request) always takes precedence, even if a partial
/// `code`/`state` happened to be present alongside it. Otherwise, both `code`
/// and `state` must be present for the exchange to proceed.
fn interpret_callback_params(params: CallbackParams) -> CallbackOutcome {
    if let Some(error) = params.error {
        return Err(error);
    }

    match (params.code, params.state) {
        (Some(code), Some(state)) => Ok(OAuthCallback { code, state }),
        _ => Err("Missing code or state in OAuth callback".to_string()),
    }
}

/// Parse a loopback OAuth redirect URI (e.g. `http://localhost:9090/oauth/callback`)
/// into the `(port, path)` the local callback server must bind to.
///
/// This mirrors the loopback-only validation `main.rs` already applies to
/// `YOUTUBE_REDIRECT_URI`, but additionally extracts the port/path so the
/// callback listener can never drift from the URI handed to Google.
fn parse_redirect_uri(redirect_uri: &str) -> Result<(u16, String)> {
    let rest = redirect_uri.strip_prefix("http://").context(
        "YOUTUBE_REDIRECT_URI must be a loopback http:// URL, e.g. http://localhost:9090/oauth/callback",
    )?;

    let mut segments = rest.splitn(2, '/');
    let authority = segments.next().unwrap_or_default();
    let path_part = segments.next().unwrap_or_default();

    let (host, port_str) = authority.split_once(':').with_context(|| {
        format!(
            "YOUTUBE_REDIRECT_URI must include an explicit port, e.g. http://localhost:9090/oauth/callback (got: {})",
            redirect_uri
        )
    })?;

    if !matches!(host, "localhost" | "127.0.0.1") {
        return Err(anyhow::anyhow!(
            "YOUTUBE_REDIRECT_URI host must be localhost or 127.0.0.1, got: {}",
            host
        ));
    }

    let port: u16 = port_str.parse().with_context(|| {
        format!(
            "YOUTUBE_REDIRECT_URI has an invalid port '{}' (got: {})",
            port_str, redirect_uri
        )
    })?;

    let path = format!("/{}", path_part.trim_matches('/'));
    if path == "/" {
        return Err(anyhow::anyhow!(
            "YOUTUBE_REDIRECT_URI must include a non-root callback path, got: {}",
            redirect_uri
        ));
    }

    Ok((port, path))
}

/// HTML response shown to user after successful OAuth
const SUCCESS_HTML: &str = r##"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>LoLShorts - Authorization Successful</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            min-height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        }
        .container {
            background: white;
            border-radius: 16px;
            padding: 48px;
            box-shadow: 0 10px 40px rgba(0,0,0,0.2);
            text-align: center;
            max-width: 500px;
        }
        .success-icon {
            width: 80px;
            height: 80px;
            margin: 0 auto 24px;
            background: #10b981;
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
        }
        .checkmark {
            width: 40px;
            height: 40px;
            border: 4px solid white;
            border-left: none;
            border-top: none;
            transform: rotate(45deg);
            margin-top: -10px;
        }
        h1 {
            color: #1f2937;
            margin: 0 0 16px;
            font-size: 28px;
        }
        p {
            color: #6b7280;
            margin: 0 0 32px;
            font-size: 16px;
            line-height: 1.6;
        }
        .button {
            background: #667eea;
            color: white;
            padding: 12px 32px;
            border-radius: 8px;
            text-decoration: none;
            display: inline-block;
            font-weight: 600;
            transition: background 0.2s;
        }
        .button:hover {
            background: #5568d3;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="success-icon">
            <div class="checkmark"></div>
        </div>
        <h1>Authorization Successful!</h1>
        <p>Your YouTube account has been successfully connected to LoLShorts. You can now close this window and return to the application.</p>
        <button class="button" onclick="window.close(); return false;">Close Window</button>
    </div>
    <script>
        setTimeout(function() { window.close(); }, 3000);
    </script>
</body>
</html>"##;

/// HTML response shown to user after they decline consent (or the callback
/// otherwise carried an `error`). Reuses the same layout/style as
/// [`SUCCESS_HTML`], recolored to a neutral/cancelled tone.
const CANCELLED_HTML: &str = r##"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>LoLShorts - Authorization Cancelled</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            min-height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #6b7280 0%, #374151 100%);
        }
        .container {
            background: white;
            border-radius: 16px;
            padding: 48px;
            box-shadow: 0 10px 40px rgba(0,0,0,0.2);
            text-align: center;
            max-width: 500px;
        }
        .cancel-icon {
            width: 80px;
            height: 80px;
            margin: 0 auto 24px;
            background: #9ca3af;
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
        }
        .cross {
            position: relative;
            width: 40px;
            height: 40px;
        }
        .cross::before, .cross::after {
            content: '';
            position: absolute;
            top: 18px;
            left: 2px;
            width: 36px;
            height: 4px;
            background: white;
            border-radius: 2px;
        }
        .cross::before {
            transform: rotate(45deg);
        }
        .cross::after {
            transform: rotate(-45deg);
        }
        h1 {
            color: #1f2937;
            margin: 0 0 16px;
            font-size: 28px;
        }
        p {
            color: #6b7280;
            margin: 0 0 32px;
            font-size: 16px;
            line-height: 1.6;
        }
        .button {
            background: #6b7280;
            color: white;
            padding: 12px 32px;
            border-radius: 8px;
            text-decoration: none;
            display: inline-block;
            font-weight: 600;
            transition: background 0.2s;
        }
        .button:hover {
            background: #4b5563;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="cancel-icon">
            <div class="cross"></div>
        </div>
        <h1>연결이 취소되었습니다</h1>
        <p>Connection cancelled — YouTube authorization was not completed. You can close this window and return to LoLShorts to try again.</p>
        <button class="button" onclick="window.close(); return false;">Close Window</button>
    </div>
    <script>
        setTimeout(function() { window.close(); }, 3000);
    </script>
</body>
</html>"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_callback_server_creation() {
        let server = CallbackServer::new(9090, "oauth/callback");
        assert_eq!(server.port, 9090);
        assert_eq!(server.path, "/oauth/callback");
    }

    #[test]
    fn new_normalizes_leading_and_trailing_slashes() {
        let server = CallbackServer::new(9090, "/oauth/callback/");
        assert_eq!(server.path, "/oauth/callback");
    }

    #[test]
    fn from_redirect_uri_extracts_port_and_path() {
        let server = CallbackServer::from_redirect_uri("http://localhost:9090/oauth/callback")
            .expect("valid redirect URI should parse");
        assert_eq!(server.port, 9090);
        assert_eq!(server.path, "/oauth/callback");
    }

    #[test]
    fn from_redirect_uri_accepts_127_0_0_1() {
        let server = CallbackServer::from_redirect_uri("http://127.0.0.1:8123/callback")
            .expect("valid redirect URI should parse");
        assert_eq!(server.port, 8123);
        assert_eq!(server.path, "/callback");
    }

    // These exercise `parse_redirect_uri` directly (rather than through
    // `CallbackServer::from_redirect_uri`) so the error cases can use
    // `unwrap_err()`, which needs the success type to implement `Debug` —
    // `(u16, String)` does, `CallbackServer` (holding oneshot senders) does not.

    #[test]
    fn from_redirect_uri_rejects_https() {
        let error = parse_redirect_uri("https://localhost:9090/oauth/callback").unwrap_err();
        assert!(error.to_string().contains("loopback http://"));
    }

    #[test]
    fn from_redirect_uri_rejects_non_loopback_host() {
        let error = parse_redirect_uri("http://example.com:9090/oauth/callback").unwrap_err();
        assert!(error.to_string().contains("localhost or 127.0.0.1"));
    }

    #[test]
    fn from_redirect_uri_rejects_missing_port() {
        let error = parse_redirect_uri("http://localhost/oauth/callback").unwrap_err();
        assert!(error.to_string().contains("explicit port"));
    }

    #[test]
    fn from_redirect_uri_rejects_invalid_port() {
        let error = parse_redirect_uri("http://localhost:abc/oauth/callback").unwrap_err();
        assert!(error.to_string().contains("invalid port"));
    }

    #[test]
    fn from_redirect_uri_rejects_root_path() {
        let error = parse_redirect_uri("http://localhost:9090/").unwrap_err();
        assert!(error.to_string().contains("non-root callback path"));

        let error_no_slash = parse_redirect_uri("http://localhost:9090").unwrap_err();
        assert!(error_no_slash
            .to_string()
            .contains("non-root callback path"));
    }

    #[test]
    fn callback_server_from_redirect_uri_surfaces_parse_errors() {
        // Sanity check that the public constructor actually returns Err (not just
        // the underlying parser) without requiring CallbackServer: Debug.
        assert!(CallbackServer::from_redirect_uri("not-a-url").is_err());
    }

    // `interpret_callback_params` covers the pure decision logic; the
    // `warp::query` tests below (`callback_route_query_filter_*`) cover the
    // regression itself — that a decline redirect (`?error=...`, no
    // `code`/`state`) actually deserializes instead of getting silently
    // rejected by warp before ever reaching this function.

    #[test]
    fn interpret_callback_params_succeeds_for_code_and_state() {
        let params = CallbackParams {
            code: Some("auth-code".to_string()),
            state: Some("csrf-state".to_string()),
            error: None,
        };
        let callback = interpret_callback_params(params).expect("code+state should parse");
        assert_eq!(callback.code, "auth-code");
        assert_eq!(callback.state, "csrf-state");
    }

    #[test]
    fn interpret_callback_params_returns_error_for_access_denied() {
        let params = CallbackParams {
            code: None,
            state: None,
            error: Some("access_denied".to_string()),
        };
        let error = interpret_callback_params(params).unwrap_err();
        assert_eq!(error, "access_denied");
    }

    #[test]
    fn interpret_callback_params_error_wins_over_partial_code_or_state() {
        // Some IdPs leave a leftover `state` alongside `error`; the rejection
        // must win regardless of what else is present.
        let params = CallbackParams {
            code: None,
            state: Some("csrf-state".to_string()),
            error: Some("access_denied".to_string()),
        };
        let error = interpret_callback_params(params).unwrap_err();
        assert_eq!(error, "access_denied");
    }

    #[test]
    fn interpret_callback_params_errors_when_code_or_state_missing_without_error() {
        let params = CallbackParams {
            code: Some("auth-code".to_string()),
            state: None,
            error: None,
        };
        let error = interpret_callback_params(params).unwrap_err();
        assert!(error.contains("Missing code or state"));
    }

    // These exercise the real `warp::query::<CallbackParams>()` filter (not
    // just the pure `interpret_callback_params` helper above) to guard the
    // actual regression: `code`/`state` used to be required, so a Google
    // decline redirect (`?error=access_denied`, no `code`/`state`) failed to
    // deserialize at all, the route silently never matched, and the flow only
    // surfaced as a 120-second timeout instead of an immediate failure.

    #[tokio::test]
    async fn callback_route_query_filter_parses_error_without_code_or_state() {
        let filter = warp::query::<CallbackParams>();
        let params = warp::test::request()
            .path("/oauth/callback?error=access_denied")
            .filter(&filter)
            .await
            .expect("an error-only query string should deserialize, not reject");

        assert_eq!(params.error.as_deref(), Some("access_denied"));
        assert_eq!(params.code, None);
        assert_eq!(params.state, None);
    }

    #[tokio::test]
    async fn callback_route_query_filter_parses_code_and_state() {
        let filter = warp::query::<CallbackParams>();
        let params = warp::test::request()
            .path("/oauth/callback?code=auth-code&state=csrf-state")
            .filter(&filter)
            .await
            .expect("code+state query string should deserialize");

        assert_eq!(params.code.as_deref(), Some("auth-code"));
        assert_eq!(params.state.as_deref(), Some("csrf-state"));
        assert_eq!(params.error, None);
    }

    // Note: Full integration tests require actual HTTP requests
    // These would be better suited for E2E tests
}
