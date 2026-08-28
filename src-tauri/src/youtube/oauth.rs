use anyhow::{Context, Result};
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    EndpointNotSet, EndpointSet, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken,
    Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

const OAUTH_TOKEN_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// YouTube OAuth2 scopes required for upload functionality
const YOUTUBE_UPLOAD_SCOPE: &str = "https://www.googleapis.com/auth/youtube.upload";
const YOUTUBE_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/youtube.readonly";

/// Google OAuth2 endpoints
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Stored OAuth2 credentials with refresh capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YouTubeCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>, // Unix timestamp
    pub token_type: String,
}

/// OAuth2 state for PKCE flow
#[derive(Debug)]
struct OAuth2State {
    csrf_token: CsrfToken,
    pkce_verifier: PkceCodeVerifier,
}

type ConfiguredBasicClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

/// YouTube OAuth2 Client
pub struct YouTubeOAuthClient {
    oauth_client: ConfiguredBasicClient,
    http_client: reqwest::Client,
    state: Arc<RwLock<Option<OAuth2State>>>,
    credentials: Arc<RwLock<Option<YouTubeCredentials>>>,
}

impl YouTubeOAuthClient {
    /// Create new YouTube OAuth2 client
    ///
    /// # Arguments
    /// * `client_id` - Google OAuth2 client ID
    /// * `client_secret` - Google OAuth2 client secret
    /// * `redirect_uri` - OAuth2 redirect URI (must match Google Console config)
    pub fn new(client_id: String, client_secret: String, redirect_uri: String) -> Result<Self> {
        let client_id = ClientId::new(client_id);
        let client_secret = ClientSecret::new(client_secret);
        let auth_url =
            AuthUrl::new(GOOGLE_AUTH_URL.to_string()).context("Failed to create auth URL")?;
        let token_url =
            TokenUrl::new(GOOGLE_TOKEN_URL.to_string()).context("Failed to create token URL")?;
        let redirect_url =
            RedirectUrl::new(redirect_uri).context("Failed to create redirect URL")?;

        let oauth_client = BasicClient::new(client_id)
            .set_client_secret(client_secret)
            .set_auth_uri(auth_url)
            .set_token_uri(token_url)
            .set_redirect_uri(redirect_url);
        let http_client = reqwest::Client::builder()
            // OAuth endpoints must never redirect token-bearing POST requests
            // to an attacker-controlled origin.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to create OAuth HTTP client")?;

        Ok(Self {
            oauth_client,
            http_client,
            state: Arc::new(RwLock::new(None)),
            credentials: Arc::new(RwLock::new(None)),
        })
    }

    /// Generate OAuth2 authorization URL with PKCE
    ///
    /// Returns the authorization URL that should be opened in a browser
    pub async fn generate_auth_url(&self) -> Result<String> {
        // Generate PKCE challenge
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        // Generate authorization URL
        let (auth_url, csrf_token) = self
            .oauth_client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(YOUTUBE_UPLOAD_SCOPE.to_string()))
            .add_scope(Scope::new(YOUTUBE_READONLY_SCOPE.to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        // Store state for verification
        let mut state = self.state.write().await;
        *state = Some(OAuth2State {
            csrf_token,
            pkce_verifier,
        });

        info!("Generated OAuth2 authorization URL");
        debug!("Generated PKCE authorization request without logging callback state");

        Ok(auth_url.to_string())
    }

    /// Exchange authorization code for access token
    ///
    /// # Arguments
    /// * `code` - Authorization code from OAuth2 callback
    /// * `state` - CSRF state token from OAuth2 callback
    pub async fn exchange_code(&self, code: String, state: String) -> Result<YouTubeCredentials> {
        // Take the OAuth2 state to extract the pkce_verifier
        let oauth_state = self.state.write().await.take();
        let stored_state =
            oauth_state.context("No OAuth2 state found. Call generate_auth_url() first")?;

        // Verify CSRF token
        if stored_state.csrf_token.secret() != &state {
            error!("CSRF token mismatch");
            return Err(anyhow::anyhow!("CSRF token verification failed"));
        }

        let pkce_verifier = stored_state.pkce_verifier;

        // Exchange authorization code for token
        let token_response = tokio::time::timeout(
            OAUTH_TOKEN_REQUEST_TIMEOUT,
            self.oauth_client
                .exchange_code(AuthorizationCode::new(code))
                .set_pkce_verifier(pkce_verifier)
                .request_async(&self.http_client),
        )
        .await
        .context("YouTube authorization-code exchange timed out")?
        .context("Failed to exchange authorization code")?;

        // Calculate expiration time
        let expires_at = token_response
            .expires_in()
            .map(|duration| chrono::Utc::now().timestamp() + duration.as_secs() as i64);

        let credentials = YouTubeCredentials {
            access_token: token_response.access_token().secret().clone(),
            refresh_token: token_response.refresh_token().map(|t| t.secret().clone()),
            expires_at,
            token_type: "Bearer".to_string(),
        };

        // Store credentials
        let mut stored_creds = self.credentials.write().await;
        *stored_creds = Some(credentials.clone());

        info!("Successfully exchanged authorization code for access token");

        Ok(credentials)
    }

    /// Refresh access token using refresh token
    pub async fn refresh_token(&self) -> Result<YouTubeCredentials> {
        let current_creds = self.credentials.read().await;

        // Explicit clone of refresh token to avoid blocking_read issues
        let refresh_token_str = current_creds
            .as_ref()
            .and_then(|c| c.refresh_token.as_ref())
            .map(|t| t.to_string())
            .context("No refresh token available")?;

        // Store existing refresh token before releasing lock (avoid blocking_read later)
        let existing_refresh_token = current_creds
            .as_ref()
            .and_then(|c| c.refresh_token.as_ref())
            .map(|t| t.to_string());

        let refresh_token = RefreshToken::new(refresh_token_str);
        drop(current_creds); // Release lock before async call

        let token_response = tokio::time::timeout(
            OAUTH_TOKEN_REQUEST_TIMEOUT,
            self.oauth_client
                .exchange_refresh_token(&refresh_token)
                .request_async(&self.http_client),
        )
        .await
        .context("YouTube token refresh timed out")?
        .context("Failed to refresh access token")?;

        // Calculate new expiration time
        let expires_at = token_response
            .expires_in()
            .map(|duration| chrono::Utc::now().timestamp() + duration.as_secs() as i64);

        let credentials = YouTubeCredentials {
            access_token: token_response.access_token().secret().clone(),
            refresh_token: token_response
                .refresh_token()
                .map(|t| t.secret().clone())
                .or(existing_refresh_token), // Use pre-fetched value instead of blocking_read
            expires_at,
            token_type: "Bearer".to_string(),
        };

        // Update stored credentials
        let mut stored_creds = self.credentials.write().await;
        *stored_creds = Some(credentials.clone());

        info!("Successfully refreshed access token");

        Ok(credentials)
    }

    /// Check if current token is expired or about to expire
    ///
    /// Returns true if token expires within 5 minutes
    pub async fn is_token_expired(&self) -> bool {
        let creds = self.credentials.read().await;
        if let Some(credentials) = creds.as_ref() {
            if let Some(expires_at) = credentials.expires_at {
                let now = chrono::Utc::now().timestamp();
                let buffer = 300; // 5 minutes buffer
                return now >= (expires_at - buffer);
            }
        }
        true
    }

    /// Get current valid access token, refreshing if necessary
    pub async fn get_valid_token(&self) -> Result<String> {
        if self.is_token_expired().await {
            warn!("Access token expired, refreshing...");
            let credentials = self.refresh_token().await?;
            Ok(credentials.access_token)
        } else {
            let creds = self.credentials.read().await;
            creds
                .as_ref()
                .map(|c| c.access_token.clone())
                .context("No credentials available. Please authenticate first.")
        }
    }

    /// Set credentials (used for loading stored credentials)
    pub async fn set_credentials(&self, credentials: YouTubeCredentials) {
        let mut stored_creds = self.credentials.write().await;
        *stored_creds = Some(credentials);
        info!("Credentials loaded successfully");
    }

    /// Get current credentials (for storage)
    pub async fn get_credentials(&self) -> Option<YouTubeCredentials> {
        self.credentials.read().await.clone()
    }

    /// Create an independent OAuth client using the same OAuth configuration and explicit credentials.
    pub async fn clone_with_credentials(&self, credentials: YouTubeCredentials) -> Self {
        Self {
            oauth_client: self.oauth_client.clone(),
            http_client: self.http_client.clone(),
            state: Arc::new(RwLock::new(None)),
            credentials: Arc::new(RwLock::new(Some(credentials))),
        }
    }

    /// Clear stored credentials (logout)
    pub async fn clear_credentials(&self) {
        let mut stored_creds = self.credentials.write().await;
        *stored_creds = None;
        info!("Credentials cleared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_oauth_client_creation() {
        let client = YouTubeOAuthClient::new(
            "test_client_id".to_string(),
            "test_client_secret".to_string(),
            "http://localhost:8080/callback".to_string(),
        );

        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_generate_auth_url() {
        let client = YouTubeOAuthClient::new(
            "test_client_id".to_string(),
            "test_client_secret".to_string(),
            "http://localhost:8080/callback".to_string(),
        )
        .unwrap();

        let auth_url = client.generate_auth_url().await.unwrap();

        assert!(auth_url.contains("accounts.google.com"));
        assert!(auth_url.contains("client_id=test_client_id"));
        assert!(auth_url.contains("youtube.upload"));
    }

    #[tokio::test]
    async fn test_token_expiration_check() {
        let client = YouTubeOAuthClient::new(
            "test_client_id".to_string(),
            "test_client_secret".to_string(),
            "http://localhost:8080/callback".to_string(),
        )
        .unwrap();

        // Initially should be expired (no credentials)
        assert!(client.is_token_expired().await);

        // Set expired credentials
        let expired_creds = YouTubeCredentials {
            access_token: "test_token".to_string(),
            refresh_token: Some("test_refresh".to_string()),
            expires_at: Some(chrono::Utc::now().timestamp() - 3600), // Expired 1 hour ago
            token_type: "Bearer".to_string(),
        };
        client.set_credentials(expired_creds).await;

        assert!(client.is_token_expired().await);

        // Set valid credentials
        let valid_creds = YouTubeCredentials {
            access_token: "test_token".to_string(),
            refresh_token: Some("test_refresh".to_string()),
            expires_at: Some(chrono::Utc::now().timestamp() + 3600), // Expires in 1 hour
            token_type: "Bearer".to_string(),
        };
        client.set_credentials(valid_creds).await;

        assert!(!client.is_token_expired().await);
    }

    #[tokio::test]
    async fn test_credentials_storage() {
        let client = YouTubeOAuthClient::new(
            "test_client_id".to_string(),
            "test_client_secret".to_string(),
            "http://localhost:8080/callback".to_string(),
        )
        .unwrap();

        // Initially no credentials
        assert!(client.get_credentials().await.is_none());

        // Set credentials
        let creds = YouTubeCredentials {
            access_token: "test_access".to_string(),
            refresh_token: Some("test_refresh".to_string()),
            expires_at: Some(chrono::Utc::now().timestamp() + 3600),
            token_type: "Bearer".to_string(),
        };
        client.set_credentials(creds.clone()).await;

        // Retrieve credentials
        let stored = client.get_credentials().await.unwrap();
        assert_eq!(stored.access_token, creds.access_token);
        assert_eq!(stored.refresh_token, creds.refresh_token);

        // Clear credentials
        client.clear_credentials().await;
        assert!(client.get_credentials().await.is_none());
    }
}
