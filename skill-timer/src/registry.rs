//! In-process registry for multiple named concurrent timers.
//!
//! The registry tracks active timers by name (auto-naming if not provided),
//! supports querying remaining time, listing active timers, and cancellation.
//! It is intentionally process-local: timers are owned by the running session.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Snapshot of an active timer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimerStatus {
    pub name: String,
    pub total_seconds: u64,
    pub remaining_seconds: u64,
    pub expired: bool,
}

#[derive(Clone, Debug)]
struct ScheduledTimer {
    name: String,
    started_at: Instant,
    duration: Duration,
}

impl ScheduledTimer {
    fn remaining(&self, now: Instant) -> Duration {
        self.duration
            .saturating_sub(now.saturating_duration_since(self.started_at))
    }

    fn is_expired(&self, now: Instant) -> bool {
        self.remaining(now).is_zero()
    }

    fn to_status(&self, now: Instant) -> TimerStatus {
        let remaining = self.remaining(now);
        TimerStatus {
            name: self.name.clone(),
            total_seconds: self.duration.as_secs(),
            remaining_seconds: remaining.as_secs(),
            expired: remaining.is_zero(),
        }
    }
}

/// Registry of named concurrent timers. Cheap to clone — shares state.
#[derive(Clone, Default)]
pub struct TimerRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

#[derive(Default)]
struct RegistryInner {
    timers: HashMap<String, ScheduledTimer>,
    /// Monotonically increasing count of timers added to derive ordinal names.
    counter: u64,
}

impl TimerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a timer. Returns the resolved name.
    pub fn add(&self, name: Option<&str>, duration: Duration) -> String {
        self.add_at(name, duration, Instant::now())
    }

    fn add_at(&self, name: Option<&str>, duration: Duration, now: Instant) -> String {
        let mut inner = self.lock();
        inner.prune(now);
        inner.counter = inner.counter.saturating_add(1);
        let resolved = match name {
            Some(n) if !n.trim().is_empty() => {
                let base = n.trim().to_string();
                inner.unique_name(base)
            }
            _ => format!(
                "{} timer",
                crate::macos_clock::MacOsClockTimerSkill::ordinal_name(inner.counter)
            ),
        };
        inner.timers.insert(
            resolved.clone(),
            ScheduledTimer {
                name: resolved.clone(),
                started_at: now,
                duration,
            },
        );
        resolved
    }

    pub fn remaining(&self, name: &str) -> Option<TimerStatus> {
        self.remaining_at(name, Instant::now())
    }

    fn remaining_at(&self, name: &str, now: Instant) -> Option<TimerStatus> {
        let mut inner = self.lock();
        inner.prune(now);
        inner.timers.get(name).map(|t| t.to_status(now))
    }

    pub fn list(&self) -> Vec<TimerStatus> {
        self.list_at(Instant::now())
    }

    fn list_at(&self, now: Instant) -> Vec<TimerStatus> {
        let mut inner = self.lock();
        inner.prune(now);
        let mut out: Vec<TimerStatus> = inner.timers.values().map(|t| t.to_status(now)).collect();
        out.sort_by(|a, b| a.remaining_seconds.cmp(&b.remaining_seconds));
        out
    }

    pub fn cancel(&self, name: &str) -> bool {
        let mut inner = self.lock();
        inner.timers.remove(name).is_some()
    }

    /// Number of active (unexpired) timers.
    pub fn active_count(&self) -> usize {
        let now = Instant::now();
        let mut inner = self.lock();
        inner.prune(now);
        inner.timers.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RegistryInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl RegistryInner {
    fn prune(&mut self, now: Instant) {
        self.timers.retain(|_, t| !t.is_expired(now));
    }

    fn unique_name(&self, base: String) -> String {
        if !self.timers.contains_key(&base) {
            return base;
        }
        for i in 2..u32::MAX {
            let candidate = format!("{base} {i}");
            if !self.timers.contains_key(&candidate) {
                return candidate;
            }
        }
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_returns_unique_names() {
        let reg = TimerRegistry::new();
        let a = reg.add(Some("pasta"), Duration::from_secs(60));
        let b = reg.add(Some("pasta"), Duration::from_secs(60));
        assert_eq!(a, "pasta");
        assert_eq!(b, "pasta 2");
    }

    #[test]
    fn add_uses_ordinal_when_unnamed() {
        let reg = TimerRegistry::new();
        let a = reg.add(None, Duration::from_secs(60));
        let b = reg.add(None, Duration::from_secs(60));
        assert_eq!(a, "first timer");
        assert_eq!(b, "second timer");
    }

    #[test]
    fn remaining_decreases_over_time() {
        let reg = TimerRegistry::new();
        let now = Instant::now();
        reg.add_at(Some("steak"), Duration::from_secs(120), now);
        let later = now + Duration::from_secs(30);
        let status = reg.remaining_at("steak", later).expect("should exist");
        assert_eq!(status.total_seconds, 120);
        assert_eq!(status.remaining_seconds, 90);
        assert!(!status.expired);
    }

    #[test]
    fn expired_timers_are_pruned_on_access() {
        let reg = TimerRegistry::new();
        let now = Instant::now();
        reg.add_at(Some("brief"), Duration::from_secs(10), now);
        let later = now + Duration::from_secs(11);
        assert!(reg.list_at(later).is_empty());
        assert!(reg.remaining_at("brief", later).is_none());
    }

    #[test]
    fn cancel_removes_named_timer() {
        let reg = TimerRegistry::new();
        reg.add(Some("a"), Duration::from_secs(60));
        assert!(reg.cancel("a"));
        assert!(reg.remaining("a").is_none());
        assert!(!reg.cancel("a"));
    }

    #[test]
    fn list_sorted_by_remaining() {
        let reg = TimerRegistry::new();
        let now = Instant::now();
        reg.add_at(Some("long"), Duration::from_secs(300), now);
        reg.add_at(Some("short"), Duration::from_secs(60), now);
        let names: Vec<String> = reg.list_at(now).into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["short".to_string(), "long".to_string()]);
    }
}
