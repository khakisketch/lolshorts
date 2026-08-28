pub mod client;

pub use client::{SupabaseClient, SupabaseConfig};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SupabaseError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Supabase API error: {0}")]
    ApiError(String),

    #[error("Invalid response from Supabase: {0}")]
    InvalidResponse(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

pub type Result<T> = std::result::Result<T, SupabaseError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupabaseUser {
    pub id: String,
    pub email: String,
    pub email_confirmed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub app_metadata: serde_json::Value,
    pub user_metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LicenseStatus {
    #[serde(rename = "active", alias = "ACTIVE")]
    Active,
    #[serde(rename = "expired", alias = "EXPIRED")]
    Expired,
    #[serde(
        rename = "cancelled",
        alias = "CANCELLED",
        alias = "canceled",
        alias = "CANCELED"
    )]
    Cancelled,
    #[serde(rename = "inactive", alias = "INACTIVE")]
    Inactive,
    #[serde(rename = "past_due", alias = "PAST_DUE")]
    PastDue,
    #[serde(rename = "none", alias = "NONE")]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub id: String,
    pub user_id: String,
    pub tier: String,
    pub status: LicenseStatus,
    pub created_at: String,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub stripe_subscription_id: Option<String>,
    #[serde(default)]
    pub stripe_customer_id: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = SupabaseError::Unauthorized("Invalid token".to_string());
        assert_eq!(error.to_string(), "Unauthorized: Invalid token");

        let error = SupabaseError::ApiError("Rate limit exceeded".to_string());
        assert_eq!(error.to_string(), "Supabase API error: Rate limit exceeded");
    }
}
