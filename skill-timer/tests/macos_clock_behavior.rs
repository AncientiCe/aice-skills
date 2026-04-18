//! Behavioral tests for macOS Clock timer skill (dry-run mode).

use skill_timer::{MacOsClockTimerSkill, TimerSkill};

pub trait TestOptionExt<T> {
    fn must(self) -> T;
}

impl<T> TestOptionExt<T> for Option<T> {
    fn must(self) -> T {
        match self {
            Some(value) => value,
            None => panic!("expected Some(..) in test"),
        }
    }
}

pub trait TestResultExt<T, E> {
    fn must(self) -> T;
    fn must_err(self) -> E;
}

impl<T, E: std::fmt::Debug> TestResultExt<T, E> for Result<T, E> {
    fn must(self) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("expected Ok(..) in test, got Err: {:?}", error),
        }
    }

    fn must_err(self) -> E {
        match self {
            Ok(_) => panic!("expected Err(..) in test, got Ok"),
            Err(error) => error,
        }
    }
}

#[tokio::test]
async fn dry_run_creates_named_timer() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    let result = skill
        .execute("10 minutes", Some("pasta timer"))
        .await
        .must();
    assert_eq!(result.timer_name, "pasta timer");
    assert_eq!(result.duration_seconds, 600);
    assert!(result.duration_display.contains("10 minutes"));
    assert!(result.summary.contains("pasta timer"));
}

#[tokio::test]
async fn dry_run_creates_unnamed_timer_with_ordinal_first() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    // dry_run returns None for active count → ordinal = first
    let result = skill.execute("5 minutes", None).await.must();
    assert_eq!(result.timer_name, "first timer");
    assert_eq!(result.duration_seconds, 300);
}

#[tokio::test]
async fn dry_run_handles_empty_name_as_unnamed() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    let result = skill.execute("30 seconds", Some("")).await.must();
    assert_eq!(result.timer_name, "first timer");
    assert_eq!(result.duration_seconds, 30);
}

#[tokio::test]
async fn dry_run_rejects_invalid_duration() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    let result = skill.execute("soon", None).await;
    assert!(result.is_err());
    let err = result.must_err();
    assert!(err.to_string().contains("invalid duration"));
}

#[tokio::test]
async fn dry_run_duration_with_hours_and_minutes() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    let result = skill
        .execute("1 hour 30 minutes", Some("long bake"))
        .await
        .must();
    assert_eq!(result.duration_seconds, 5400);
    assert!(result.duration_display.contains("1 hour"));
    assert!(result.duration_display.contains("30 minutes"));
}

#[tokio::test]
async fn registry_tracks_multiple_concurrent_timers() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    skill.execute("10 minutes", Some("pasta")).await.must();
    skill.execute("3 minutes", Some("steak")).await.must();
    let active = skill.list().await;
    assert_eq!(active.len(), 2);
    let names: Vec<&str> = active.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"pasta"));
    assert!(names.contains(&"steak"));
}

#[tokio::test]
async fn remaining_returns_status_for_named_timer() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    skill.execute("10 minutes", Some("eggs")).await.must();
    let status = skill.remaining("eggs").await.must();
    assert_eq!(status.name, "eggs");
    assert_eq!(status.total_seconds, 600);
    assert!(status.remaining_seconds <= 600);
    assert!(!status.expired);
}

#[tokio::test]
async fn cancel_removes_active_timer() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    skill.execute("5 minutes", Some("nap")).await.must();
    assert!(skill.cancel("nap").await);
    assert!(skill.remaining("nap").await.is_none());
    assert!(!skill.cancel("nap").await);
}

#[tokio::test]
async fn duplicate_named_timer_gets_unique_suffix() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    let a = skill.execute("5 minutes", Some("rice")).await.must();
    let b = skill.execute("5 minutes", Some("rice")).await.must();
    assert_eq!(a.timer_name, "rice");
    assert_eq!(b.timer_name, "rice 2");
}

#[tokio::test]
async fn to_prompt_context_contains_name_and_duration() {
    let skill = MacOsClockTimerSkill::new_for_tests();
    let result = skill.execute("20 minutes", Some("egg timer")).await.must();
    let context = result.to_prompt_context();
    assert!(context.contains("egg timer"));
    assert!(context.contains("20 minutes"));
}
