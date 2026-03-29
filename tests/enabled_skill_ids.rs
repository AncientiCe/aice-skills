use core_skills::ENABLED_SKILL_IDS;

#[cfg(not(any(feature = "macos", feature = "windows", feature = "ios")))]
#[test]
fn core_common_always_includes_only_common_core_when_no_os_bundle_enabled() {
    let expected = ["skill_weather", "skill_time", "skill_distance"];
    assert_eq!(ENABLED_SKILL_IDS, expected);
}

#[cfg(feature = "macos")]
#[test]
fn macos_bundle_includes_macos_frontend_skills() {
    assert!(ENABLED_SKILL_IDS.contains(&"skill_media"));
    assert!(ENABLED_SKILL_IDS.contains(&"skill_screenshot"));
}

#[cfg(any(feature = "windows", feature = "ios"))]
#[test]
fn windows_and_ios_bundles_currently_only_include_core_common_skills() {
    let expected = ["skill_weather", "skill_time", "skill_distance"];
    assert_eq!(ENABLED_SKILL_IDS, expected);
}
