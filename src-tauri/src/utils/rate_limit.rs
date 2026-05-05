use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum CommandGroup {
    Recording,
    Upload,
    Settings,
}

pub struct RateLimiter {
    last_calls: Mutex<HashMap<CommandGroup, Instant>>,
    limits: HashMap<CommandGroup, Duration>,
}

impl RateLimiter {
    pub fn new() -> Self {
        let mut limits = HashMap::new();
        limits.insert(CommandGroup::Recording, Duration::from_secs(2));
        limits.insert(CommandGroup::Upload, Duration::from_secs(5));
        limits.insert(CommandGroup::Settings, Duration::from_secs(1));
        Self {
            last_calls: Mutex::new(HashMap::new()),
            limits,
        }
    }

    /// Check if command group is allowed. Returns Ok(()) if allowed, Err(remaining_wait) if rate limited.
    pub fn check(&self, group: CommandGroup) -> Result<(), Duration> {
        let mut last_calls = self.last_calls.lock().unwrap();
        let limit = self
            .limits
            .get(&group)
            .copied()
            .unwrap_or(Duration::from_secs(1));
        if let Some(last) = last_calls.get(&group) {
            let elapsed = last.elapsed();
            if elapsed < limit {
                return Err(limit - elapsed);
            }
        }
        last_calls.insert(group, Instant::now());
        Ok(())
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_first_call_always_passes() {
        let limiter = RateLimiter::new();
        assert!(limiter.check(CommandGroup::Recording).is_ok());
    }

    #[test]
    fn test_rapid_second_call_rejected() {
        let limiter = RateLimiter::new();
        limiter.check(CommandGroup::Recording).unwrap();
        assert!(limiter.check(CommandGroup::Recording).is_err());
    }

    #[test]
    fn test_different_groups_independent() {
        let limiter = RateLimiter::new();
        limiter.check(CommandGroup::Recording).unwrap();
        assert!(limiter.check(CommandGroup::Upload).is_ok());
    }

    #[test]
    fn test_call_after_cooldown_passes() {
        let limiter = RateLimiter::new();
        limiter.check(CommandGroup::Settings).unwrap();
        thread::sleep(Duration::from_millis(1100));
        assert!(limiter.check(CommandGroup::Settings).is_ok());
    }

    #[test]
    fn test_returns_remaining_wait_time() {
        let limiter = RateLimiter::new();
        limiter.check(CommandGroup::Upload).unwrap();
        if let Err(remaining) = limiter.check(CommandGroup::Upload) {
            assert!(remaining.as_secs() <= 5);
            assert!(remaining.as_millis() > 0);
        } else {
            panic!("Should have been rate limited");
        }
    }

    #[test]
    fn test_all_groups_pass_first_call() {
        let limiter = RateLimiter::new();
        assert!(limiter.check(CommandGroup::Recording).is_ok());

        let limiter2 = RateLimiter::new();
        assert!(limiter2.check(CommandGroup::Upload).is_ok());

        let limiter3 = RateLimiter::new();
        assert!(limiter3.check(CommandGroup::Settings).is_ok());
    }

    #[test]
    fn test_settings_rate_limit_is_1_second() {
        let limiter = RateLimiter::new();
        limiter.check(CommandGroup::Settings).unwrap();
        let err = limiter.check(CommandGroup::Settings).unwrap_err();
        // Settings limit is 1s, so remaining should be < 1s
        assert!(err.as_secs() <= 1);
    }

    #[test]
    fn test_default_creates_same_as_new() {
        let limiter = RateLimiter::default();
        // First call should always pass
        assert!(limiter.check(CommandGroup::Recording).is_ok());
    }

    #[test]
    fn test_multiple_groups_can_each_call_once() {
        let limiter = RateLimiter::new();
        assert!(limiter.check(CommandGroup::Recording).is_ok());
        assert!(limiter.check(CommandGroup::Upload).is_ok());
        assert!(limiter.check(CommandGroup::Settings).is_ok());
        // Second call to any should be rate-limited
        assert!(limiter.check(CommandGroup::Recording).is_err());
        assert!(limiter.check(CommandGroup::Upload).is_err());
        assert!(limiter.check(CommandGroup::Settings).is_err());
    }
}
