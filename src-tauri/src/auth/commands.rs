use super::{AuthManager, SubscriptionTier, User};
use crate::error::{AppError, AppResult};
use crate::AppState;
use serde::de::DeserializeOwned;
use tauri::State;
use tracing::{info, warn};

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

#[cfg(test)]
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
        crate::supabase::LicenseStatus::PastDue => "past_due",
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
pub async fn logout(state: State<'_, AppState>) -> AppResult<()> {
    let _youtube_credential_guard = state.youtube_manager.lock_credential_operations().await;
    state.youtube_manager.clear_app_auth_oauth_state().await;

    state
        .auth
        .logout()
        .map_err(|e| AppError::Internal(e.to_string()))
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
    // Keep account identifiers out of the persistent application log. The
    // session is still fully validated below, but an email/user id is not
    // needed to diagnose a renderer-to-Rust sync failure.
    info!("Syncing Supabase session");

    let previous_user_id = state
        .auth
        .get_current_user()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .map(|user| user.id);

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
        warn!("Token validation failed during session sync: {}", e);
        session_validation_error(e)
    })?;

    if verified_user.id != user_id {
        warn!("Rejected session sync: token subject does not match claimed user");
        return Err(AppError::Auth("Token subject mismatch".to_string()));
    }

    if !verified_user.email.eq_ignore_ascii_case(&email) {
        warn!("Rejected session sync: token email does not match claimed email");
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

    // A token refresh for the same Supabase identity must not interrupt an
    // active YouTube OAuth/upload session. Clear in-memory YouTube state only
    // on an actual account boundary (including a fresh process login).
    if session_identity_changed(previous_user_id.as_deref(), &user.id) {
        let _youtube_credential_guard = state.youtube_manager.lock_credential_operations().await;
        state.youtube_manager.clear_app_auth_oauth_state().await;
    }

    state
        .auth
        .login(user.clone())
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(SessionSyncResponse { user, entitlement })
}

// ============================================================================
// Payment Commands
// Default builds remain payment-deferred. Live billing is available only when
// LOLSHORTS_PAYMENT_ENABLED=true and every request goes through the Supabase
// billing Edge Function. The desktop client never grants PRO directly.
// ============================================================================

/// Subscription details structure for frontend compatibility
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubscriptionDetails {
    pub is_active: bool,
    pub tier: String,
    pub status: String,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub auto_renew: bool,
    pub payment_method: Option<String>,
    /// Indicates if payment system is available
    #[serde(default)]
    pub payment_available: bool,
    /// Message about payment availability
    pub payment_message: Option<String>,
    pub reason: String,
    pub next_required_step: String,
    #[serde(default)]
    pub next_billing_date: Option<String>,
    #[serde(default)]
    pub cancel_at_period_end: bool,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub checkout_url: Option<String>,
    #[serde(default)]
    pub last_payment_error: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct BillingErrorResponse {
    message: Option<String>,
    reason: Option<String>,
    next_required_step: Option<String>,
}

#[cfg(test)]
fn parse_payment_enabled(value: Option<&str>) -> bool {
    matches!(
        value.map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "1" | "true" | "yes" | "on")
    )
}

fn payment_live_enabled() -> bool {
    // This release line is the free public edition. Keep the server-backed
    // billing commands fail-closed even if a user machine defines legacy env
    // variables. Re-enabling sales requires a separately reviewed build.
    false
}

fn session_identity_changed(previous_user_id: Option<&str>, next_user_id: &str) -> bool {
    previous_user_id != Some(next_user_id)
}

fn session_validation_error(error: crate::supabase::SupabaseError) -> AppError {
    match error {
        crate::supabase::SupabaseError::HttpError(error) => AppError::Network(error.to_string()),
        error => AppError::Auth(format!("Invalid token: {error}")),
    }
}

fn billing_function_url(supabase_client: &crate::supabase::SupabaseClient) -> String {
    std::env::var("LOLSHORTS_BILLING_FUNCTION_URL").unwrap_or_else(|_| {
        format!(
            "{}/functions/v1/billing",
            supabase_client.project_url().trim_end_matches('/')
        )
    })
}

async fn call_billing_function<T: DeserializeOwned>(
    state: &AppState,
    method: reqwest::Method,
    path: &str,
    access_token: &str,
    body: Option<serde_json::Value>,
) -> Result<T, String> {
    let supabase_client = state
        .auth
        .get_supabase_client()
        .map_err(|e| format!("Billing backend configuration error: {}", e))?;
    let url = format!(
        "{}/{}",
        billing_function_url(supabase_client).trim_end_matches('/'),
        path.trim_start_matches('/')
    );

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create billing HTTP client: {}", e))?;
    let mut request = http_client
        .request(method, url)
        .header("apikey", supabase_client.anon_key())
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json");

    if let Some(body) = body {
        request = request.json(&body);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Billing server request failed: {}", e))?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read billing response: {}", e))?;

    if !status.is_success() {
        let parsed_error = serde_json::from_str::<BillingErrorResponse>(&body_text).ok();
        let message = parsed_error
            .as_ref()
            .and_then(|error| error.message.clone().or(error.reason.clone()))
            .unwrap_or_else(|| {
                if body_text.trim().is_empty() {
                    format!("Billing server returned {}", status)
                } else {
                    body_text.clone()
                }
            });
        let next_step = parsed_error
            .and_then(|error| error.next_required_step)
            .unwrap_or_else(|| "Resolve the billing server error and retry.".to_string());
        return Err(format!("{} Next step: {}", message, next_step));
    }

    serde_json::from_str::<T>(&body_text)
        .map_err(|e| format!("Invalid billing server response: {}", e))
}

async fn deferred_subscription_details(state: &AppState) -> Result<SubscriptionDetails, String> {
    let user = state
        .auth
        .get_current_user()
        .map_err(|e| format!("Failed to get user: {}", e))?;

    match user {
        Some(user) => {
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
                auto_renew: false,
                payment_method: None,
                payment_available: false,
                payment_message: Some(PAYMENT_DEFERRED_REASON.to_string()),
                reason: PAYMENT_DEFERRED_REASON.to_string(),
                next_required_step: PAYMENT_DEFERRED_NEXT_STEP.to_string(),
                next_billing_date: None,
                cancel_at_period_end: false,
                provider: Some("toss".to_string()),
                checkout_url: None,
                last_payment_error: None,
            })
        }
        None => Ok(SubscriptionDetails {
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
            next_billing_date: None,
            cancel_at_period_end: false,
            provider: Some("toss".to_string()),
            checkout_url: None,
            last_payment_error: None,
        }),
    }
}

async fn sync_entitlement_after_payment(state: &AppState, user: &User) {
    let Ok(supabase_client) = state.auth.get_supabase_client() else {
        warn!("Unable to refresh entitlement after payment: Supabase client unavailable");
        return;
    };

    let entitlement = match supabase_client
        .get_user_license(&user.id, &user.access_token)
        .await
    {
        Ok(license) => entitlement_from_license(license),
        Err(e) => {
            warn!(
                "Entitlement refresh after billing mutation failed; failing closed to FREE: {}",
                e
            );
            entitlement_from_license(None)
        }
    };

    sync_cached_user_tier(&state.auth, user, &entitlement);
}

/// Get subscription details from the authoritative billing server when live
/// billing is enabled, otherwise return explicit deferred status.
#[tauri::command]
pub async fn get_subscription_details(
    state: State<'_, AppState>,
) -> Result<SubscriptionDetails, String> {
    if !payment_live_enabled() {
        return deferred_subscription_details(state.inner()).await;
    }

    let user = state
        .auth
        .get_current_user()
        .map_err(|e| format!("Failed to get user: {}", e))?;
    let Some(user) = user else {
        return deferred_subscription_details(state.inner()).await;
    };

    call_billing_function(
        state.inner(),
        reqwest::Method::GET,
        "subscription",
        &user.access_token,
        None,
    )
    .await
}

/// Cancel subscription through the authoritative billing server.
#[tauri::command]
pub async fn cancel_subscription(
    state: State<'_, AppState>,
) -> Result<SubscriptionDetails, String> {
    if !payment_live_enabled() {
        return Err(format!(
            "{} No active subscription can be changed in this build.",
            PAYMENT_DEFERRED_REASON
        ));
    }

    let user = state
        .auth
        .get_current_user()
        .map_err(|e| format!("Authentication required: {}", e))?;
    let Some(user) = user else {
        return Err("You must be logged in to manage your subscription.".to_string());
    };

    let details = call_billing_function(
        state.inner(),
        reqwest::Method::POST,
        "cancel",
        &user.access_token,
        None,
    )
    .await?;
    sync_entitlement_after_payment(state.inner(), &user).await;
    Ok(details)
}

/// Open payment page by asking the trusted billing server to create checkout.
#[tauri::command]
pub async fn open_payment_page(
    state: State<'_, AppState>,
    period: Option<String>,
) -> Result<String, String> {
    if !payment_live_enabled() {
        return Err(format!(
            "{} No checkout or paid upgrade is available in this build.",
            PAYMENT_DEFERRED_REASON
        ));
    }

    let user = state
        .auth
        .get_current_user()
        .map_err(|e| format!("Authentication required: {}", e))?;
    let Some(user) = user else {
        return Err("Please log in first to upgrade to PRO.".to_string());
    };

    let details: SubscriptionDetails = call_billing_function(
        state.inner(),
        reqwest::Method::POST,
        "checkout",
        &user.access_token,
        Some(serde_json::json!({
            "period": period.unwrap_or_else(|| "MONTHLY".to_string())
        })),
    )
    .await?;

    details
        .checkout_url
        .ok_or_else(|| "Billing server did not return a checkout URL.".to_string())
}

/// Confirm a Toss redirect result through the trusted billing server. A success
/// response only refreshes entitlement from Supabase; it does not grant PRO
/// from client-provided payment data.
#[tauri::command]
pub async fn confirm_payment(
    state: State<'_, AppState>,
    payment_key: String,
    order_id: String,
    amount: i64,
) -> Result<SubscriptionDetails, String> {
    if !payment_live_enabled() {
        return Err(format!(
            "{} Payment confirmation is unavailable in this build.",
            PAYMENT_DEFERRED_REASON
        ));
    }

    let user = state
        .auth
        .get_current_user()
        .map_err(|e| format!("Authentication required: {}", e))?;
    let Some(user) = user else {
        return Err("Please log in before confirming payment.".to_string());
    };

    let details = call_billing_function(
        state.inner(),
        reqwest::Method::POST,
        "confirm",
        &user.access_token,
        Some(serde_json::json!({
            "paymentKey": payment_key,
            "orderId": order_id,
            "amount": amount
        })),
    )
    .await?;
    sync_entitlement_after_payment(state.inner(), &user).await;
    Ok(details)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_refresh_preserves_same_user_youtube_state() {
        assert!(!session_identity_changed(Some("user-a"), "user-a"));
        assert!(session_identity_changed(Some("user-a"), "user-b"));
        assert!(session_identity_changed(None, "user-a"));
    }

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

    #[test]
    fn past_due_pro_license_fails_closed_to_free_entitlement() {
        let license = test_license("PRO", crate::supabase::LicenseStatus::PastDue);

        let entitlement = entitlement_from_license(Some(license));

        assert_eq!(entitlement.tier, "FREE");
        assert_eq!(entitlement.status, "past_due");
        assert_eq!(entitlement.source, "supabase");
        assert!(!entitlement.payment_available);
    }

    #[test]
    fn parses_payment_enabled_flag_explicitly() {
        assert!(parse_payment_enabled(Some("true")));
        assert!(parse_payment_enabled(Some("1")));
        assert!(parse_payment_enabled(Some("YES")));
        assert!(!parse_payment_enabled(None));
        assert!(!parse_payment_enabled(Some("false")));
        assert!(!parse_payment_enabled(Some("enabled")));
    }
}
