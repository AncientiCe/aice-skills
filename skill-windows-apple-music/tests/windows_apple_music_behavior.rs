use skill_windows_apple_music::{WindowsAppleMusic, WindowsAppleMusicSkill};

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
async fn parity_actions_are_supported_in_test_mode() {
    let skill = WindowsAppleMusicSkill::new_for_tests();
    for action in [
        "play",
        "pause",
        "resume",
        "stop",
        "next",
        "previous",
        "shuffle_on",
        "shuffle_off",
        "status",
    ] {
        if cfg!(target_os = "windows") {
            let target = if action == "play" {
                Some("around the world")
            } else {
                None
            };
            let result = skill.execute(Some(action), target).await.must();
            assert!(!result.summary.is_empty());
        } else {
            let err = skill
                .execute(Some(action), Some("around the world"))
                .await
                .must_err();
            assert!(format!("{err}").contains("requires Windows"));
        }
    }
}

#[tokio::test]
async fn play_requires_target() {
    let skill = WindowsAppleMusicSkill::new_for_tests();
    if cfg!(target_os = "windows") {
        let err = skill.execute(Some("play"), None).await.must_err();
        assert!(format!("{err}").contains("no active source"));
    } else {
        let err = skill.execute(Some("play"), None).await.must_err();
        assert!(format!("{err}").contains("requires Windows"));
    }
}

#[tokio::test]
async fn non_windows_returns_platform_error() {
    if cfg!(target_os = "windows") {
        return;
    }
    let skill = WindowsAppleMusicSkill::new_for_tests();
    let err = skill.execute(Some("status"), None).await.must_err();
    assert!(format!("{err}").contains("requires Windows"));
}
