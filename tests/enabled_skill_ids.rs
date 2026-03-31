use core_skills::ENABLED_SKILL_IDS;

const CORE_COMMON_SKILLS: [&str; 8] = [
    "skill_weather",
    "skill_time",
    "skill_distance",
    "skill_sports_live",
    "skill_holiday_lookup",
    "skill_fuel_price_lookup",
    "skill_horoscope_daily",
    "skill_news_headlines",
];

#[cfg(not(any(feature = "macos", feature = "windows", feature = "ios")))]
#[test]
fn core_common_always_includes_only_common_core_when_no_os_bundle_enabled() {
    assert_eq!(ENABLED_SKILL_IDS, CORE_COMMON_SKILLS);
}

#[cfg(feature = "macos")]
#[test]
fn macos_bundle_includes_macos_frontend_skills() {
    for core_skill in CORE_COMMON_SKILLS {
        assert!(ENABLED_SKILL_IDS.contains(&core_skill));
    }
    assert!(ENABLED_SKILL_IDS.contains(&"skill_media"));
    assert!(ENABLED_SKILL_IDS.contains(&"skill_screenshot"));
}

#[cfg(any(feature = "windows", feature = "ios"))]
#[test]
fn windows_and_ios_bundles_currently_only_include_core_common_skills() {
    assert_eq!(ENABLED_SKILL_IDS, CORE_COMMON_SKILLS);
}
