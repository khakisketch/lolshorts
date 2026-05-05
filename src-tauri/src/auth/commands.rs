use super::{AuthManager, SubscriptionTier, User};
use crate::error::{AppError, AppResult};
use crate::AppState;
use tauri::State;
use tracing::{error, info, warn};

const PAYMENT_DEFERRED_REASON: &str =
    "Payment and PRO subscriptions are deferred until non-payment readiness gates pass.";
const PAYMENT_DEFERRED_NEXT_STEP: &str =
    "Enable server-side payment approval and webhook processing before live checkout.";

#[derive(Debug, Clone, serde::Serialize)]
pub struct EntitlementResponse {
    pub tier: String,
    pub status: String,
    pub expires_at: Option<String>,
    pub source: String,
    pub checked_at: String,
    pub payment_available: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSyncResponse {
    pub user: User,
    pub entitlement: EntitlementResponse,
}

fn subscription_tier_from_license(license: &crate::supabase::License) -> SubscriptionTier {
    if license.tier == "PRO" && license_is_active(license) {
        SubscriptionTier::Pro
    } else {
        SubscriptionTier::Free
    }
}

fn license_is_active(license: &crate::supabase::License) -> bool {
    matches!(&license.status, crate::supabase::LicenseStatus::Active)
        && license_not_expired(license.expires_at.as_deref())
}

fn license_not_expired(expires_at: Option<&str>) -> bool {
    let Some(expires_at) = expires_at else {
        return true;
    };

    chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|expires_at| expires_at.with_timezone(&chrono::Utc) > chrono::Utc::now())
        .unwrap_or_else(|e| {
            warn!(
                "Invalid license expires_at '{}'; failing closed to expired: {}",
                expires_at, e
            );
            false
        })
}

fn tier_from_entitlement(entitlement: &EntitlementResponse) -> SubscriptionTier {
    if entitlement.tier == "PRO" && entitlement.status == "active" {
        SubscriptionTier::Pro
    } else {
        SubscriptionTier::Free
    }
}

fn sync_cached_user_tier(auth: &AuthManager, user: &User, entitlement: &EntitlementResponse) {
    let authoritative_tier = tier_from_entitlement(entitlement);
    if user.tier == authoritative_tier {
        return;
    }

    let mut updated_user = user.clone();
    updated_user.tier = authoritative_tier;

    if let Err(e) = auth.login(updated_user) {
        warn!("Failed to sync cached user tier from entitlement: {}", e);
    }
}

fn status_string_for_license(license: &crate::supabase::License) -> String {
    if !license_not_expired(license.expires_at.as_deref()) {
        return "expired".to_string();
    }

    match &license.status {
        crate::supabase::LicenseStatus::Active => "active",
        crate::supabase::LicenseStatus::Expired => "expired",
        crate::supabase::LicenseStatus::Cancelled => "cancelled",
        crate::supabase::LicenseStatus::Inactive => "inactive",
        crate::supabase::LicenseStatus::None => "none",
    }
    .to_string()
}

fn entitlement_from_license(license: Option<crate::supabase::License>) -> EntitlementResponse {
    let (tier, status, expires_at) = match license {
        Some(license) if license_is_active(&license) => {
            let tier = if license.tier == "PRO" { "PRO" } else { "FREE" };
            (tier.to_string(), "active".to_string(), license.expires_at)
        }
        Some(license) => (
            "FREE".to_string(),
            status_string_for_license(&license),
            license.expires_at,
        ),
        None => ("FREE".to_string(), "none".to_string(), None),
    };

    EntitlementResponse {
        tier,
        status,
        expires_at,
        source: "supabase".to_string(),
        checked_at: chrono::Utc::now().to_rfc3339(),
        payment_available: false,
    }
}

#[tauri::command]
pub async fn login(state: State<'_, AppState>, email: String, password: String) -> AppResult<User> {
    info!("Login attempt for user: {}", email);

    // Get Supabase client
    let supabase_client = state
        .auth
        .get_supabase_client()
        .map_err(|e| AppError::Internal(format!("Failed to get Supabase client: {}", e)))?;

    // Authenticate with Supabase
    let session = supabase_client
        .sign_in(&email, &password)
        .await
        .map_err(|e| {
            error!("Supabase sign-in failed: {}", e);
            AppError::Auth(format!("Sign-in failed: {}", e))
        })?;

    // Fetch user's license tier from database
    let tier = match supabase_client
        .get_user_license(&session.user.id, &session.access_token)
        .await
    {
        Ok(Some(license)) => {
            info!(
                "Fetched license for user: tier={}, status={:?}",
                license.tier, license.status
            );
            subscription_tier_from_license(&license)
        }
        Ok(None) => {
            info!("No license found for user, defaulting to Free tier");
            SubscriptionTier::Free
        }
        Err(e) => {
            error!("Failed to fetch license: {}, defaulting to Free tier", e);
            SubscriptionTier::Free
        }
    };

    let user = User {
        id: session.user.id,
        email: session.user.email,
        tier,
        access_token: session.access_token,
        refresh_token: session.refresh_token,
        expires_at: session.expires_at,
    };

    let _youtube_credential_guard = state.youtube_manager.lock_credential_operations().await;
    state.youtube_manager.clear_app_auth_oauth_state().await;

    state
        .auth
        .login(user.clone())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    info!("Login successful for user: {}", user.email);
    Ok(user)
}

#[tauri::command]
pub async fn signup(
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> AppResult<User> {
    info!("Signup attempt for user: {}", email);

    // Get Supabase client
    let supabase_client = state
        .auth
        .get_supabase_client()
        .map_err(|e| AppError::Internal(format!("Failed to get Supabase client: {}", e)))?;

    // Create account with Supabase
    let session = supabase_client
        .sign_up(&email, &password)
        .await
        .map_err(|e| {
            error!("Supabase sign-up failed: {}", e);
            AppError::Auth(format!("Sign-up failed: {}", e))
        })?;

    // Fetch user's license tier from database (should be created by trigger)
    let tier = match supabase_client
        .get_user_license(&session.user.id, &session.access_token)
        .await
    {
        Ok(Some(license)) => {
            info!(
                "License created for new user: tier={}, status={:?}",
                license.tier, license.status
            );
            subscription_tier_from_license(&license)
        }
        Ok(None) | Err(_) => {
            info!("Using default Free tier for new user");
            SubscriptionTier::Free
        }
    };

    let user = User {
        id: session.user.id,
        email: session.user.email,
        tier,
        access_token: session.access_token,
        refresh_token: session.refresh_token,
        expires_at: session.expires_at,
    };

    let _youtube_credential_guard = state.youtube_manager.lock_credential_operations().await;
    state.youtube_manager.clear_app_auth_oauth_state().await;

    state
        .auth
        .login(user.clone())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    info!("Signup successful for user: {}", user.email);
    Ok(user)
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> AppResult<()> {
    let _youtube_credential_guard = state.youtube_manager.lock_credential_operations().await;
    state.youtube_manager.clear_app_auth_oauth_state().await;

    state
        .auth
        .logout()
        .map_err(|e| AppError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn get_user_status(state: State<'_, AppState>) -> AppResult<Option<User>> {
    state
        .auth
        .get_current_user()
        .map_err(|e| AppError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn get_license_info(
    state: State<'_, AppState>,
) -> AppResult<Option<crate::supabase::License>> {
    // Get current user
    let user = state
        .auth
        .get_current_user()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if let Some(user) = user {
        // Get Supabase client
        let supabase_client = state
            .auth
            .get_supabase_client()
            .map_err(|e| AppError::Internal(format!("Failed to get Supabase client: {}", e)))?;

        // Fetch license from database
        supabase_client
            .get_user_license(&user.id, &user.access_token)
            .await
            .map_err(|e| AppError::Database(e.to_string()))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn refresh_token(state: State<'_, AppState>) -> AppResult<User> {
    // Get current user
    let current_user = state
        .auth
        .get_current_user()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::Auth("No user logged in".to_string()))?;

    info!("Refreshing token for user: {}", current_user.email);

    // Get Supabase client
    let supabase_client = state
        .auth
        .get_supabase_client()
        .map_err(|e| AppError::Internal(format!("Failed to get Supabase client: {}", e)))?;

    // Refresh the session with Supabase
    let session = supabase_client
        .refresh_token(&current_user.refresh_token)
        .await
        .map_err(|e| {
            error!("Token refresh failed: {}", e);
            AppError::Auth(format!("Token refresh failed: {}", e))
        })?;

    let refreshed_license = supabase_client
        .get_user_license(&current_user.id, &session.access_token)
        .await
        .ok()
        .flatten();
    let refreshed_entitlement = entitlement_from_license(refreshed_license);
    let refreshed_tier = if refreshed_entitlement.tier == "PRO" {
        SubscriptionTier::Pro
    } else {
        SubscriptionTier::Free
    };

    // Update user with new tokens and authoritative entitlement
    let updated_user = User {
        id: current_user.id,
        email: current_user.email,
        tier: refreshed_tier,
        access_token: session.access_token,
        refresh_token: session.refresh_token,
        expires_at: session.expires_at,
    };

    // Update stored user
    state
        .auth
        .login(updated_user.clone())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    info!("Token refresh successful for user: {}", updated_user.email);
    Ok(updated_user)
}

/// License info for frontend (matches TypeScript LicenseInfo interface)
#[derive(serde::Serialize)]
pub struct LicenseInfoResponse {
    pub tier: String,
    pub expires_at: Option<String>,
    pub is_active: bool,
}

#[tauri::command]
pub async fn get_user_license(state: State<'_, AppState>) -> AppResult<LicenseInfoResponse> {
    // ... existing code ...
    let user = state
        .auth
        .get_current_user()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let user = user.ok_or_else(|| AppError::Auth("User not authenticated".to_string()))?;

    // Get Supabase client
    let supabase_client = state
        .auth
        .get_supabase_client()
        .map_err(|e| AppError::Internal(format!("Failed to get Supabase client: {}", e)))?;

    // Fetch license from database
    let license = supabase_client
        .get_user_license(&user.id, &user.access_token)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    match license {
        Some(license) => {
            let is_active = license_is_active(&license);

            Ok(LicenseInfoResponse {
                tier: if is_active && license.tier == "PRO" {
                    "PRO".to_string()
                } else {
                    "FREE".to_string()
                },
                expires_at: license.expires_at,
                is_active,
            })
        }
        None => {
            // Default to FREE tier if no license found
            Ok(LicenseInfoResponse {
                tier: "FREE".to_string(),
                expires_at: None,
                is_active: true,
            })
        }
    }
}

/// Return the current authoritative entitlement from Supabase user_licenses.
#[tauri::command]
pub async fn get_current_entitlement(
    state: State<'_, AppState>,
    _force_refresh: Option<bool>,
) -> AppResult<EntitlementResponse> {
    let user = state
        .auth
        .get_current_user()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let Some(user) = user else {
        return Ok(entitlement_from_license(None));
    };

    let supabase_client = match state.auth.get_supabase_client() {
        Ok(client) => client,
        Err(e) => {
            warn!(
                "Supabase client unavailable during entitlement refresh; failing closed to FREE: {}",
                e
            );
            let entitlement = entitlement_from_license(None);
            sync_cached_user_tier(&state.auth, &user, &entitlement);
            return Ok(entitlement);
        }
    };

    let license = match supabase_client
        .get_user_license(&user.id, &user.access_token)
        .await
    {
        Ok(license) => license,
        Err(e) => {
            warn!(
                "Authoritative entitlement lookup failed; failing closed to FREE: {}",
                e
            );
            let entitlement = entitlement_from_license(None);
            sync_cached_user_tier(&state.auth, &user, &entitlement);
            return Ok(entitlement);
        }
    };

    let entitlement = entitlement_from_license(license);
    sync_cached_user_tier(&state.auth, &user, &entitlement);
    Ok(entitlement)
}

/// Sync session from Frontend (Supabase JS SDK) to Backend
/// Validates the token by fetching user data from Supabase before storing
#[tauri::command]
pub async fn set_session(
    state: State<'_, AppState>,
    access_token: String,
    refresh_token: String,
    user_id: String,
    email: String,
    expires_at: Option<i64>,
) -> AppResult<SessionSyncResponse> {
    info!("Syncing session for user: {}", email);

    // Validate input
    if access_token.is_empty() || refresh_token.is_empty() {
        return Err(AppError::Validation("Token cannot be empty".to_string()));
    }
    if user_id.is_empty() || email.is_empty() {
        return Err(AppError::Validation(
            "User ID and email are required".to_string(),
        ));
    }

    let supabase_client = state
        .auth
        .get_supabase_client()
        .map_err(|e| AppError::Internal(format!("Failed to get Supabase client: {}", e)))?;

    let verified_user = supabase_client.get_user(&access_token).await.map_err(|e| {
        warn!(
            "Token validation failed for claimed user {}: {}",
            user_id, e
        );
        AppError::Auth(format!("Invalid token: {}", e))
    })?;

    if verified_user.id != user_id {
        warn!(
            "Rejected session sync: token subject {} does not match claimed user {}",
            verified_user.id, user_id
        );
        return Err(AppError::Auth("Token subject mismatch".to_string()));
    }

    if !verified_user.email.eq_ignore_ascii_case(&email) {
        warn!(
            "Rejected session sync: token email {} does not match claimed email {}",
            verified_user.email, email
        );
        return Err(AppError::Auth("Token email mismatch".to_string()));
    }

    let license = match supabase_client
        .get_user_license(&verified_user.id, &access_token)
        .await
    {
        Ok(license) => license,
        Err(e) => {
            warn!(
                "Failed to fetch Supabase entitlement for user {}; failing closed as FREE: {}",
                verified_user.id, e
            );
            None
        }
    };
    let entitlement = entitlement_from_license(license);
    let tier = if entitlement.tier == "PRO" {
        SubscriptionTier::Pro
    } else {
        SubscriptionTier::Free
    };

    let user = User {
        id: verified_user.id,
        email: verified_user.email,
        tier,
        access_token,
        refresh_token,
        expires_at: expires_at.unwrap_or_else(|| chrono::Utc::now().timestamp() + 3600),
    };

    let _youtube_credential_guard = state.youtube_manager.lock_credential_operations().await;
    state.youtube_manager.clear_app_auth_oauth_state().await;

    state
        .auth
        .login(user.clone())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(SessionSyncResponse { user, entitlement })
}

// ============================================================================
// Payment Commands - Deferred
// Payment, Toss checkout, and subscription enforcement remain disabled until
// non-payment readiness gates pass and a separate payment QA plan is approved.
// ============================================================================

/// Subscription details structure for frontend compatibility
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubscriptionDetails {
    pub is_active: bool,
    pub tier: String,
    pub status: String,
    pub expires_at: Option<String>,
    pub auto_renew: bool,
    pub payment_method: Option<String>,
    /// Indicates if payment system is available
    pub payment_available: bool,
    /// Message about payment availability
    pub payment_message: Option<String>,
    pub reason: String,
    pub next_required_step: String,
}

/// Get subscription details - Returns actual license status from database
#[tauri::command]
pub async fn get_subscription_details(
    state: State<'_, AppState>,
) -> Result<SubscriptionDetails, String> {
    let user = state
        .auth
        .get_current_user()
        .map_err(|e| format!("Failed to get user: {}", e))?;

    match user {
        Some(user) => {
            // Fetch actual license from database
            let supabase_client = state
                .auth
                .get_supabase_client()
                .map_err(|e| format!("Database connection error: {}", e))?;

            let license = supabase_client
                .get_user_license(&user.id, &user.access_token)
                .await
                .ok()
                .flatten();

            let entitlement = entitlement_from_license(license);
            let is_active = entitlement.status == "active";

            Ok(SubscriptionDetails {
                is_active,
                tier: entitlement.tier,
                status: entitlement.status,
                expires_at: entitlement.expires_at,
                auto_renew: false,    // Auto-renew not yet implemented
                payment_method: None, // Payment methods not yet stored
                payment_available: false,
                payment_message: Some(PAYMENT_DEFERRED_REASON.to_string()),
                reason: PAYMENT_DEFERRED_REASON.to_string(),
                next_required_step: PAYMENT_DEFERRED_NEXT_STEP.to_string(),
            })
        }
        None => {
            // Not logged in - return free tier
            Ok(SubscriptionDetails {
                is_active: false,
                tier: "FREE".to_string(),
                status: "none".to_string(),
                expires_at: None,
                auto_renew: false,
                payment_method: None,
                payment_available: false,
                payment_message: Some("Log in to view subscription details".to_string()),
                reason: "User is not authenticated.".to_string(),
                next_required_step: "Log in before viewing subscription details.".to_string(),
            })
        }
    }
}

/// Cancel subscription - Handles subscription cancellation requests
#[tauri::command]
pub async fn cancel_subscription(state: State<'_, AppState>) -> Result<String, String> {
    let user = state
        .auth
        .get_current_user()
        .map_err(|e| format!("Authentication required: {}", e))?;

    let Some(user) = user else {
        return Err("You must be logged in to manage your subscription.".to_string());
    };

    let license = state
        .auth
        .get_supabase_client()
        .map_err(|e| format!("Database connection error: {}", e))?
        .get_user_license(&user.id, &user.access_token)
        .await
        .ok()
        .flatten();
    let entitlement = entitlement_from_license(license);

    if entitlement.tier != "PRO" || entitlement.status != "active" {
        return Err("You don't have an active subscription to cancel.".to_string());
    }

    tracing::info!("Deferred subscription cancellation requested for user");

    Ok(format!(
        "{} No active subscription can be changed in this build.",
        PAYMENT_DEFERRED_REASON
    ))
}

/// Open payment page - Navigates to the payment/upgrade page
#[tauri::command]
pub async fn open_payment_page(state: State<'_, AppState>) -> Result<String, String> {
    let user = state
        .auth
        .get_current_user()
        .map_err(|e| format!("Authentication required: {}", e))?;

    if user.is_none() {
        return Err("Please log in first to upgrade to PRO.".to_string());
    }

    Ok(format!(
        "{} No checkout or paid upgrade is available in this build.",
        PAYMENT_DEFERRED_REASON
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_license(
        tier: &str,
        status: crate::supabase::LicenseStatus,
    ) -> crate::supabase::License {
        crate::supabase::License {
            id: "license-id".to_string(),
            user_id: "user-id".to_string(),
            tier: tier.to_string(),
            status,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: None,
            stripe_subscription_id: None,
            stripe_customer_id: None,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn active_pro_license_maps_to_pro_tier() {
        let license = test_license("PRO", crate::supabase::LicenseStatus::Active);

        assert_eq!(
            subscription_tier_from_license(&license),
            SubscriptionTier::Pro
        );
    }

    #[test]
    fn inactive_pro_license_maps_to_free_tier() {
        let expired = test_license("PRO", crate::supabase::LicenseStatus::Expired);
        let cancelled = test_license("PRO", crate::supabase::LicenseStatus::Cancelled);

        assert_eq!(
            subscription_tier_from_license(&expired),
            SubscriptionTier::Free
        );
        assert_eq!(
            subscription_tier_from_license(&cancelled),
            SubscriptionTier::Free
        );
    }

    #[test]
    fn active_free_license_maps_to_free_tier() {
        let license = test_license("FREE", crate::supabase::LicenseStatus::Active);

        assert_eq!(
            subscription_tier_from_license(&license),
            SubscriptionTier::Free
        );
    }

    #[test]
    fn expired_active_pro_license_fails_closed_to_free_entitlement() {
        let mut license = test_license("PRO", crate::supabase::LicenseStatus::Active);
        license.expires_at = Some(
            chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::days(1))
                .unwrap()
                .to_rfc3339(),
        );

        let entitlement = entitlement_from_license(Some(license));

        assert_eq!(entitlement.tier, "FREE");
        assert_eq!(entitlement.status, "expired");
        assert_eq!(entitlement.source, "supabase");
        assert!(!entitlement.payment_available);
    }

    #[test]
    fn invalid_license_expiry_fails_closed_to_free_entitlement() {
        let mut license = test_license("PRO", crate::supabase::LicenseStatus::Active);
        license.expires_at = Some("not-a-timestamp".to_string());

        let entitlement = entitlement_from_license(Some(license));

        assert_eq!(entitlement.tier, "FREE");
        assert_eq!(entitlement.status, "expired");
        assert_eq!(entitlement.source, "supabase");
        assert!(!entitlement.payment_available);
    }
}
