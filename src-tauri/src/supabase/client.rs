use super::{License, Result, SupabaseError, SupabaseUser};
use reqwest::Client;
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
pub struct SupabaseConfig {
    pub project_url: String,
    pub anon_key: String,
}

impl SupabaseConfig {
    pub fn from_env() -> Result<Self> {
        let project_url = std::env::var("SUPABASE_URL")
            .map_err(|_| SupabaseError::ConfigError("SUPABASE_URL not set".to_string()))?;

        let anon_key = std::env::var("SUPABASE_ANON_KEY")
            .map_err(|_| SupabaseError::ConfigError("SUPABASE_ANON_KEY not set".to_string()))?;

        Ok(Self {
            project_url,
            anon_key,
        })
    }

    pub fn new(project_url: String, anon_key: String) -> Self {
        Self {
            project_url,
            anon_key,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SupabaseClient {
    client: Client,
    config: SupabaseConfig,
}

impl SupabaseClient {
    pub fn new(config: SupabaseConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| {
                SupabaseError::ConfigError(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self { client, config })
    }

    pub fn from_env() -> Result<Self> {
        let config = SupabaseConfig::from_env()?;
        Self::new(config)
    }

    /// Get user details using an access token
    pub async fn get_user(&self, access_token: &str) -> Result<SupabaseUser> {
        debug!("Fetching user details");

        let url = format!("{}/auth/v1/user", self.config.project_url);

        let response = self
            .client
            .get(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await?;

        if response.status().is_success() {
            let user: SupabaseUser = response.json().await.map_err(|e| {
                error!("Failed to parse user response: {}", e);
                SupabaseError::InvalidResponse(e.to_string())
            })?;

            debug!("User details fetched successfully");
            Ok(user)
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("Failed to fetch user: {} - {}", status, error_text);

            if status == reqwest::StatusCode::UNAUTHORIZED {
                Err(SupabaseError::Unauthorized(
                    "Access token expired or invalid".to_string(),
                ))
            } else {
                Err(SupabaseError::ApiError(error_text))
            }
        }
    }

    /// Sign out (revoke tokens)
    pub async fn sign_out(&self, access_token: &str) -> Result<()> {
        info!("Signing out user");

        let url = format!("{}/auth/v1/logout", self.config.project_url);

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await?;

        if response.status().is_success() {
            info!("Sign out successful");
            Ok(())
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("Sign out failed: {}", error_text);
            Err(SupabaseError::ApiError(error_text))
        }
    }

    /// Get the Supabase configuration
    pub fn config(&self) -> &SupabaseConfig {
        &self.config
    }

    /// Get the project URL
    pub fn project_url(&self) -> &str {
        &self.config.project_url
    }

    /// Get the anon key
    pub fn anon_key(&self) -> &str {
        &self.config.anon_key
    }

    /// Get user's license from the authoritative Supabase user_licenses table.
    pub async fn get_user_license(
        &self,
        user_id: &str,
        access_token: &str,
    ) -> Result<Option<License>> {
        let url = format!("{}/rest/v1/user_licenses", self.config.project_url);

        let response = self
            .client
            .get(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .query(&[("user_id", format!("eq.{}", user_id))])
            .query(&[("select", "*")])
            .query(&[("order", "created_at.desc")])
            .send()
            .await?;

        if response.status().is_success() {
            let licenses: Vec<License> = response.json().await.map_err(|e| {
                SupabaseError::InvalidResponse(format!("Failed to parse license: {}", e))
            })?;

            Ok(select_current_license(licenses))
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            tracing::error!("Failed to fetch license: {} - {}", status, error_text);
            Err(SupabaseError::ApiError(format!(
                "Failed to fetch license: {}",
                status
            )))
        }
    }

    /// Invoke a Supabase Edge Function with the user's access token.
    ///
    /// Posts `body` as JSON to `{project_url}/functions/v1/{name}` with the
    /// caller's bearer token so the function can verify the JWT (mirrors the
    /// `apikey` + `Authorization` header convention used elsewhere in this
    /// client). Uses a deliberately short per-request timeout: callers treat a
    /// failure here as "server unreachable" and fall back to their local path,
    /// so a slow function must not stall a user-facing action.
    pub async fn call_edge_function<B: serde::Serialize>(
        &self,
        name: &str,
        access_token: &str,
        body: &B,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/functions/v1/{}",
            self.config.project_url.trim_end_matches('/'),
            name
        );

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(5))
            .json(body)
            .send()
            .await?;

        if response.status().is_success() {
            response.json().await.map_err(|e| {
                SupabaseError::InvalidResponse(format!(
                    "Failed to parse edge function '{}' response: {}",
                    name, e
                ))
            })
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            tracing::warn!(
                "Edge function '{}' returned {}: {}",
                name,
                status,
                error_text
            );
            Err(SupabaseError::ApiError(format!(
                "Edge function '{}' failed: {} - {}",
                name, status, error_text
            )))
        }
    }

    /// Generic database query method
    ///
    /// # Arguments
    /// * `table` - The table name to query
    /// * `select` - Columns to select (e.g., "*" or "id,name,email")
    /// * `filters` - Query filters as (column, value) tuples (e.g., [("status", "eq.active")])
    /// * `access_token` - User's access token for authentication
    ///
    /// # Returns
    /// JSON value containing the query results
    pub async fn query(
        &self,
        table: &str,
        select: &str,
        filters: &[(&str, &str)],
        access_token: &str,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/rest/v1/{}", self.config.project_url, table);

        let mut request = self
            .client
            .get(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .query(&[("select", select)]);

        // Add filters
        for (key, value) in filters {
            request = request.query(&[(key, value)]);
        }

        let response = request.send().await?;

        if response.status().is_success() {
            let data: serde_json::Value = response.json().await.map_err(|e| {
                error!("Failed to parse query response: {}", e);
                SupabaseError::InvalidResponse(e.to_string())
            })?;

            debug!("Query successful on table: {}", table);
            Ok(data)
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("Query failed on {}: {} - {}", table, status, error_text);
            Err(SupabaseError::ApiError(error_text))
        }
    }

    /// Generic database insert method
    ///
    /// # Arguments
    /// * `table` - The table name to insert into
    /// * `data` - The data to insert as a JSON-serializable value
    /// * `access_token` - User's access token for authentication
    ///
    /// # Returns
    /// JSON value containing the inserted record(s)
    pub async fn insert<T: serde::Serialize>(
        &self,
        table: &str,
        data: &T,
        access_token: &str,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/rest/v1/{}", self.config.project_url, table);

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .header("Prefer", "return=representation") // Return inserted data
            .json(data)
            .send()
            .await?;

        if response.status().is_success() {
            let result: serde_json::Value = response.json().await.map_err(|e| {
                error!("Failed to parse insert response: {}", e);
                SupabaseError::InvalidResponse(e.to_string())
            })?;

            info!("Insert successful on table: {}", table);
            Ok(result)
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("Insert failed on {}: {} - {}", table, status, error_text);
            Err(SupabaseError::ApiError(format!(
                "Insert failed: {}",
                error_text
            )))
        }
    }

    /// Client-side license upgrades are intentionally disabled.
    ///
    /// Live billing is deferred. When payment is enabled, a trusted server-side
    /// payment approval/webhook path must update `payments`, `subscriptions`,
    /// and `user_licenses`; the desktop client must not grant PRO directly.
    pub async fn upgrade_user_license(
        &self,
        _user_id: &str,
        _access_token: &str,
        _order_id: &str,
        _payment_key: &str,
        _amount: i64,
    ) -> Result<()> {
        Err(SupabaseError::ApiError(
            "Client-side license upgrades are disabled while payment is deferred".to_string(),
        ))
    }

    /// Generic database update method
    ///
    /// # Arguments
    /// * `table` - The table name to update
    /// * `data` - The data to update as a JSON-serializable value
    /// * `filters` - Query filters to identify which rows to update (e.g., [("id", "eq.123")])
    /// * `access_token` - User's access token for authentication
    ///
    /// # Returns
    /// JSON value containing the updated record(s)
    pub async fn update<T: serde::Serialize>(
        &self,
        table: &str,
        data: &T,
        filters: &[(&str, &str)],
        access_token: &str,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/rest/v1/{}", self.config.project_url, table);

        let mut request = self
            .client
            .patch(&url)
            .header("apikey", &self.config.anon_key)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .header("Prefer", "return=representation") // Return updated data
            .json(data);

        // Add filters
        for (key, value) in filters {
            request = request.query(&[(key, value)]);
        }

        let response = request.send().await?;

        if response.status().is_success() {
            let result: serde_json::Value = response.json().await.map_err(|e| {
                error!("Failed to parse update response: {}", e);
                SupabaseError::InvalidResponse(e.to_string())
            })?;

            info!("Update successful on table: {}", table);
            Ok(result)
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("Update failed on {}: {} - {}", table, status, error_text);
            Err(SupabaseError::ApiError(format!(
                "Update failed: {}",
                error_text
            )))
        }
    }
}

fn select_current_license(licenses: Vec<License>) -> Option<License> {
    licenses
        .iter()
        .find(|license| {
            license.tier == "PRO"
                && matches!(&license.status, super::LicenseStatus::Active)
                && license_not_expired(license.expires_at.as_deref())
        })
        .cloned()
        .or_else(|| {
            licenses
                .iter()
                .find(|license| {
                    matches!(&license.status, super::LicenseStatus::Active)
                        && license_not_expired(license.expires_at.as_deref())
                })
                .cloned()
        })
        .or_else(|| licenses.into_iter().next())
}

fn license_not_expired(expires_at: Option<&str>) -> bool {
    let Some(expires_at) = expires_at else {
        return true;
    };

    chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|expires_at| expires_at.with_timezone(&chrono::Utc) > chrono::Utc::now())
        .unwrap_or_else(|e| {
            tracing::warn!(
                "Invalid license expires_at '{}'; failing closed to expired: {}",
                expires_at,
                e
            );
            false
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_license(
        tier: &str,
        status: super::super::LicenseStatus,
        expires_at: Option<String>,
    ) -> License {
        License {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: "user-id".to_string(),
            tier: tier.to_string(),
            status,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at,
            stripe_subscription_id: None,
            stripe_customer_id: None,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_config_from_env_missing() {
        // Clear environment variables if set
        std::env::remove_var("SUPABASE_URL");
        std::env::remove_var("SUPABASE_ANON_KEY");

        let result = SupabaseConfig::from_env();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_new() {
        let config = SupabaseConfig::new(
            "https://example.supabase.co".to_string(),
            "test-anon-key".to_string(),
        );

        assert_eq!(config.project_url, "https://example.supabase.co");
        assert_eq!(config.anon_key, "test-anon-key");
    }

    #[test]
    fn select_current_license_prefers_active_pro_license() {
        let licenses = vec![
            test_license("FREE", super::super::LicenseStatus::Active, None),
            test_license("PRO", super::super::LicenseStatus::Active, None),
        ];

        let selected = select_current_license(licenses).unwrap();

        assert_eq!(selected.tier, "PRO");
        assert!(matches!(
            selected.status,
            super::super::LicenseStatus::Active
        ));
    }

    #[test]
    fn select_current_license_ignores_expired_active_pro_license() {
        let expired_at = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(1))
            .unwrap()
            .to_rfc3339();
        let licenses = vec![
            test_license("PRO", super::super::LicenseStatus::Active, Some(expired_at)),
            test_license("FREE", super::super::LicenseStatus::Active, None),
        ];

        let selected = select_current_license(licenses).unwrap();

        assert_eq!(selected.tier, "FREE");
    }
}
