use skill_record_window::{MacOsRecordWindowSkill, RecordWindowSkill, RecordWindowSkillError};

#[tokio::test]
async fn dry_run_records_with_defaults() {
    let skill = MacOsRecordWindowSkill::new_for_tests();
    let result = skill.execute(Some("Safari"), None, Some(5)).await;
    assert!(result.is_ok());
    let result = match result {
        Ok(value) => value,
        Err(error) => panic!("expected Ok(..) in test, got Err: {error:?}"),
    };
    assert_eq!(result.app_name, "Safari");
    assert_eq!(result.duration_secs, Some(5));
    assert!(result.path.to_string_lossy().ends_with(".mp4"));
}

#[tokio::test]
async fn dry_run_app_name_is_required() {
    let skill = MacOsRecordWindowSkill::new_for_tests();
    let result = skill.execute(None, None, Some(5)).await;
    assert!(matches!(result, Err(RecordWindowSkillError::Execution(_))));
}

#[tokio::test]
async fn dry_run_empty_app_name_is_rejected() {
    let skill = MacOsRecordWindowSkill::new_for_tests();
    let result = skill.execute(Some("   "), None, Some(5)).await;
    assert!(matches!(result, Err(RecordWindowSkillError::Execution(_))));
}

#[tokio::test]
async fn dry_run_filename_without_extension_gets_mp4() {
    let skill = MacOsRecordWindowSkill::new_for_tests();
    let result = skill
        .execute(Some("Xcode"), Some("session-capture"), Some(3))
        .await;
    let result = match result {
        Ok(value) => value,
        Err(error) => panic!("expected Ok(..) in test, got Err: {error:?}"),
    };
    assert!(result
        .path
        .to_string_lossy()
        .ends_with("session-capture.mp4"));
}

#[test]
fn ffmpeg_not_found_error_includes_install_hint() {
    let error = RecordWindowSkillError::FfmpegNotFound;
    let message = error.to_string();
    assert!(message.contains("ffmpeg"));
    assert!(message.contains("brew install ffmpeg"));
}

#[test]
fn audio_device_not_found_error_includes_blackhole_hint() {
    let error = RecordWindowSkillError::AudioDeviceNotFound("Aggregate Device".to_string());
    let message = error.to_string();
    assert!(message.contains("blackhole"));
    assert!(message.contains("Audio MIDI Setup"));
}
