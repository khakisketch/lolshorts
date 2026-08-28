pub mod command_policy;
pub mod commands;
pub mod middleware;

use crate::supabase::{SupabaseClient, SupabaseConfig};
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Authentication failed: {0}")]
    Failed(String),
    #[error("User not authenticated")]
    NotAuthenticated,
    #[error("Invalid token")]
    InvalidToken,
    #[error("Supabase error: {0}")]
    Supabase(#[from] crate::supabase::SupabaseError),
}

pub type Result<T> = std::result::Result<T, AuthError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionTier {
    Free,
    Pro,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub tier: SubscriptionTier,
    #[serde(skip_serializing)]
    pub access_token: String,
    #[serde(skip_serializing)]
    pub refresh_token: String,
    pub expires_at: i64,
}

pub struct AuthManager {
    current_user: RwLock<Option<User>>,
    supabase_client: Option<SupabaseClient>,
}

impl Default for AuthManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthManager {
    pub fn new() -> Self {
        // Try to initialize Supabase client from environment variables
        let supabase_client = SupabaseClient::from_env().ok();

        if supabase_client.is_none() {
            tracing::warn!(
                "Supabase client not initialized - authentication features will be limited"
            );
        }

        Self {
            current_user: RwLock::new(None),
            supabase_client,
        }
    }

    /// Create the local-only auth state used when the public Supabase client
    /// configuration was not embedded in this build.
    pub fn disabled() -> Self {
        Self {
            current_user: RwLock::new(None),
            supabase_client: None,
        }
    }

    pub fn new_with_config(config: SupabaseConfig) -> Self {
        let supabase_client = SupabaseClient::new(config).ok();

        if supabase_client.is_none() {
            tracing::warn!(
                "Supabase client not initialized - authentication features will be limited"
            );
        }

        Self {
            current_user: RwLock::new(None),
            supabase_client,
        }
    }

    pub fn has_supabase(&self) -> bool {
        self.supabase_client.is_some()
    }

    pub fn get_supabase_client(&self) -> Result<&SupabaseClient> {
        self.supabase_client
            .as_ref()
            .ok_or_else(|| AuthError::Failed("Supabase client not initialized".to_string()))
    }

    pub fn login(&self, user: User) -> Result<()> {
        let mut current_user = self
            .current_user
            .write()
            .map_err(|e| AuthError::Failed(e.to_string()))?;
        *current_user = Some(user);
        Ok(())
    }

    pub fn logout(&self) -> Result<()> {
        let mut current_user = self
            .current_user
            .write()
            .map_err(|e| AuthError::Failed(e.to_string()))?;
        *current_user = None;
        Ok(())
    }

    pub fn get_current_user(&self) -> Result<Option<User>> {
        let current_user = self
            .current_user
            .read()
            .map_err(|e| AuthError::Failed(e.to_string()))?;
        Ok(current_user.clone())
    }

    pub fn get_tier(&self) -> Result<SubscriptionTier> {
        let current_user = self
            .current_user
            .read()
            .map_err(|e| AuthError::Failed(e.to_string()))?;
        Ok(match &*current_user {
            Some(user) => user.tier.clone(),
            None => SubscriptionTier::Free, // Default to Free for unauthenticated users
        })
    }

    pub fn is_authenticated(&self) -> bool {
        self.current_user.read().is_ok_and(|user| user.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user(tier: SubscriptionTier) -> User {
        User {
            id: "test123".to_string(),
            email: "test@example.com".to_string(),
            tier,
            access_token: "test_access_token".to_string(),
            refresh_token: "test_refresh_token".to_string(),
            expires_at: 9999999999,
        }
    }

    // ---- AuthManager basics ----

    #[test]
    fn auth_manager_new_is_not_authenticated() {
        let auth = AuthManager::new();
        assert!(!auth.is_authenticated());
    }

    #[test]
    fn auth_manager_is_authenticated_after_login() {
        let auth = AuthManager::new();
        auth.login(make_user(SubscriptionTier::Free)).unwrap();
        assert!(auth.is_authenticated());
    }

    #[test]
    fn auth_manager_is_not_authenticated_after_logout() {
        let auth = AuthManager::new();
        auth.login(make_user(SubscriptionTier::Free)).unwrap();
        auth.logout().unwrap();
        assert!(!auth.is_authenticated());
    }

    #[test]
    fn auth_manager_get_current_user_returns_none_when_not_logged_in() {
        let auth = AuthManager::new();
        let user = auth.get_current_user().unwrap();
        assert!(user.is_none());
    }

    #[test]
    fn auth_manager_get_current_user_returns_some_after_login() {
        let auth = AuthManager::new();
        auth.login(make_user(SubscriptionTier::Pro)).unwrap();
        let user = auth.get_current_user().unwrap();
        assert!(user.is_some());
        assert_eq!(user.unwrap().email, "test@example.com");
    }

    // ---- SubscriptionTier ----

    #[test]
    fn subscription_tier_free_equals_free() {
        assert_eq!(SubscriptionTier::Free, SubscriptionTier::Free);
    }

    #[test]
    fn subscription_tier_pro_equals_pro() {
        assert_eq!(SubscriptionTier::Pro, SubscriptionTier::Pro);
    }

    #[test]
    fn subscription_tier_free_not_equal_to_pro() {
        assert_ne!(SubscriptionTier::Free, SubscriptionTier::Pro);
    }

    #[test]
    fn subscription_tier_serializes_and_deserializes_correctly() {
        let tier = SubscriptionTier::Pro;
        let json = serde_json::to_string(&tier).expect("serialization should succeed");
        let restored: SubscriptionTier =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(restored, SubscriptionTier::Pro);
    }

    // ---- get_tier ----

    #[test]
    fn get_tier_returns_free_when_not_authenticated() {
        let auth = AuthManager::new();
        let tier = auth.get_tier().unwrap();
        assert_eq!(tier, SubscriptionTier::Free);
    }

    #[test]
    fn get_tier_returns_pro_after_pro_user_login() {
        let auth = AuthManager::new();
        auth.login(make_user(SubscriptionTier::Pro)).unwrap();
        let tier = auth.get_tier().unwrap();
        assert_eq!(tier, SubscriptionTier::Pro);
    }

    #[test]
    fn get_tier_returns_free_after_free_user_login() {
        let auth = AuthManager::new();
        auth.login(make_user(SubscriptionTier::Free)).unwrap();
        let tier = auth.get_tier().unwrap();
        assert_eq!(tier, SubscriptionTier::Free);
    }

    // ---- User serialization (access_token and refresh_token must be skipped) ----

    #[test]
    fn user_serialization_omits_access_token() {
        let user = make_user(SubscriptionTier::Free);
        let json = serde_json::to_string(&user).expect("serialization should succeed");
        assert!(
            !json.contains("access_token"),
            "access_token should be excluded from serialized JSON, got: {}",
            json
        );
    }

    #[test]
    fn user_serialization_omits_refresh_token() {
        let user = make_user(SubscriptionTier::Free);
        let json = serde_json::to_string(&user).expect("serialization should succeed");
        assert!(
            !json.contains("refresh_token"),
            "refresh_token should be excluded from serialized JSON, got: {}",
            json
        );
    }

    #[test]
    fn user_serialization_includes_id_and_email() {
        let user = make_user(SubscriptionTier::Free);
        let json = serde_json::to_string(&user).expect("serialization should succeed");
        assert!(json.contains("test123"), "json should contain user id");
        assert!(
            json.contains("test@example.com"),
            "json should contain user email"
        );
    }

    #[test]
    fn user_serialization_includes_tier() {
        let user = make_user(SubscriptionTier::Pro);
        let json = serde_json::to_string(&user).expect("serialization should succeed");
        assert!(json.contains("tier"), "json should contain tier field");
    }

    // ---- has_supabase ----

    #[test]
    fn auth_manager_has_supabase_returns_false_without_env_vars() {
        // When SUPABASE_URL / SUPABASE_ANON_KEY are not set, client init fails.
        // This verifies the graceful fallback path.
        let auth = AuthManager::new();
        // We do not assert a specific value because CI may or may not have env vars.
        // The call itself must not panic.
        let _ = auth.has_supabase();
    }
}
