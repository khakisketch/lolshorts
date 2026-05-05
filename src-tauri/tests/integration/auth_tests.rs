// Integration tests for authentication system
#![cfg(test)]

use lolshorts::auth::middleware::{require_auth, require_tier};
use lolshorts::auth::{AuthError, AuthManager, SubscriptionTier, User};
use std::sync::Arc;
use tokio;

#[tokio::test]
async fn test_auth_manager_initialization() {
    let auth = Arc::new(AuthManager::new());

    assert!(!auth.is_authenticated());
    assert!(auth.get_current_user().unwrap().is_none());
}

#[tokio::test]
async fn test_successful_login() {
    let auth = Arc::new(AuthManager::new());

    let user = User {
        id: "test-user-123".to_string(),
        email: "test@example.com".to_string(),
        tier: SubscriptionTier::Free,
        access_token: "access_token_123".to_string(),
        refresh_token: "refresh_token_123".to_string(),
        expires_at: chrono::Utc::now().timestamp() + 3600,
    };

    let result = auth.login(user.clone());
    assert!(result.is_ok());
    assert!(auth.is_authenticated());

    let current_user = auth.get_current_user().unwrap().unwrap();
    assert_eq!(current_user.id, user.id);
    assert_eq!(current_user.email, user.email);
}

#[tokio::test]
async fn test_logout() {
    let auth = Arc::new(AuthManager::new());

    let user = User {
        id: "test-user-123".to_string(),
        email: "test@example.com".to_string(),
        tier: SubscriptionTier::Free,
        access_token: "access_token_123".to_string(),
        refresh_token: "refresh_token_123".to_string(),
        expires_at: chrono::Utc::now().timestamp() + 3600,
    };

    auth.login(user).unwrap();
    assert!(auth.is_authenticated());

    let result = auth.logout();
    assert!(result.is_ok());
    assert!(!auth.is_authenticated());
    assert!(auth.get_current_user().unwrap().is_none());
}

#[tokio::test]
async fn test_require_auth_when_authenticated() {
    let auth = Arc::new(AuthManager::new());

    let user = User {
        id: "test-user-123".to_string(),
        email: "test@example.com".to_string(),
        tier: SubscriptionTier::Free,
        access_token: "access_token_123".to_string(),
        refresh_token: "refresh_token_123".to_string(),
        expires_at: chrono::Utc::now().timestamp() + 3600,
    };

    auth.login(user.clone()).unwrap();

    let result = require_auth(&auth);
    assert!(result.is_ok());

    let returned_user = result.unwrap();
    assert_eq!(returned_user.id, user.id);
}

#[tokio::test]
async fn test_require_auth_when_not_authenticated() {
    let auth = Arc::new(AuthManager::new());

    let result = require_auth(&auth);
    assert!(result.is_err());

    match result {
        Err(AuthError::NotAuthenticated) => (),
        _ => panic!("Expected NotAuthenticated error"),
    }
}

#[tokio::test]
async fn test_require_tier_free_user_free_feature() {
    let auth = Arc::new(AuthManager::new());

    let user = User {
        id: "test-user-123".to_string(),
        email: "test@example.com".to_string(),
        tier: SubscriptionTier::Free,
        access_token: "access_token_123".to_string(),
        refresh_token: "refresh_token_123".to_string(),
        expires_at: chrono::Utc::now().timestamp() + 3600,
    };

    auth.login(user).unwrap();

    let result = require_tier(&auth, SubscriptionTier::Free);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_require_tier_free_user_pro_feature() {
    let auth = Arc::new(AuthManager::new());

    let user = User {
        id: "test-user-123".to_string(),
        email: "test@example.com".to_string(),
        tier: SubscriptionTier::Free,
        access_token: "access_token_123".to_string(),
        refresh_token: "refresh_token_123".to_string(),
        expires_at: chrono::Utc::now().timestamp() + 3600,
    };

    auth.login(user).unwrap();

    let result = require_tier(&auth, SubscriptionTier::Pro);
    assert!(result.is_err());

    match result {
        Err(AuthError::Failed(msg)) => {
            assert!(msg.contains("PRO subscription required"));
        }
        _ => panic!("Expected Failed error with PRO message"),
    }
}

#[tokio::test]
async fn test_require_tier_pro_user_any_feature() {
    let auth = Arc::new(AuthManager::new());

    let user = User {
        id: "test-user-123".to_string(),
        email: "test@example.com".to_string(),
        tier: SubscriptionTier::Pro,
        access_token: "access_token_123".to_string(),
        refresh_token: "refresh_token_123".to_string(),
        expires_at: chrono::Utc::now().timestamp() + 3600,
    };

    auth.login(user).unwrap();

    // PRO users can access FREE features
    let result_free = require_tier(&auth, SubscriptionTier::Free);
    assert!(result_free.is_ok());

    // PRO users can access PRO features
    let result_pro = require_tier(&auth, SubscriptionTier::Pro);
    assert!(result_pro.is_ok());
}

#[tokio::test]
async fn test_token_expiration_check() {
    let auth = Arc::new(AuthManager::new());

    // Create user with expired token (1 hour ago)
    let expired_user = User {
        id: "test-user-123".to_string(),
        email: "test@example.com".to_string(),
        tier: SubscriptionTier::Free,
        access_token: "access_token_123".to_string(),
        refresh_token: "refresh_token_123".to_string(),
        expires_at: chrono::Utc::now().timestamp() - 3600,
    };

    auth.login(expired_user.clone()).unwrap();

    let is_expired = lolshorts::auth::middleware::is_token_expired(&expired_user);
    assert!(is_expired);

    // Create user with valid token (1 hour in future)
    let valid_user = User {
        id: "test-user-456".to_string(),
        email: "valid@example.com".to_string(),
        tier: SubscriptionTier::Free,
        access_token: "access_token_456".to_string(),
        refresh_token: "refresh_token_456".to_string(),
        expires_at: chrono::Utc::now().timestamp() + 3600,
    };

    let is_expired_valid = lolshorts::auth::middleware::is_token_expired(&valid_user);
    assert!(!is_expired_valid);
}

#[tokio::test]
async fn test_concurrent_auth_operations() {
    use tokio::task;

    let auth = Arc::new(AuthManager::new());

    let user = User {
        id: "test-user-123".to_string(),
        email: "test@example.com".to_string(),
        tier: SubscriptionTier::Free,
        access_token: "access_token_123".to_string(),
        refresh_token: "refresh_token_123".to_string(),
        expires_at: chrono::Utc::now().timestamp() + 3600,
    };

    auth.login(user).unwrap();

    // Spawn multiple concurrent auth checks
    let mut handles = vec![];
    for _ in 0..10 {
        let auth_clone = Arc::clone(&auth);
        let handle = task::spawn(async move { require_auth(&auth_clone) });
        handles.push(handle);
    }

    // All should succeed
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
}
