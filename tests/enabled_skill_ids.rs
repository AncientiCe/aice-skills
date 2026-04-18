use core_skills::ENABLED_SKILL_IDS;

const CORE_COMMON_SKILLS: [&str; 21] = [
    "skill_weather",
    "skill_time",
    "skill_distance",
    "skill_sports_live",
    "skill_holiday_lookup",
    "skill_fuel_price_lookup",
    "skill_horoscope_daily",
    "skill_news_headlines",
    "skill_calculator",
    "skill_unit_conversion",
    "skill_currency",
    "skill_air_quality",
    "skill_dictionary",
    "skill_translate",
    "skill_spotify",
    "skill_media",
    "skill_calendar",
    "skill_meeting_notes",
    "skill_email",
    "skill_briefing",
    "skill_journal",
];

#[cfg(not(any(feature = "macos", feature = "windows", feature = "ios")))]
#[test]
fn core_common_always_includes_every_core_skill_when_no_os_bundle_enabled() {
    for skill in CORE_COMMON_SKILLS {
        assert!(
            ENABLED_SKILL_IDS.contains(&skill),
            "missing core skill {} in {:?}",
            skill,
            ENABLED_SKILL_IDS
        );
    }
    assert!(
        ENABLED_SKILL_IDS.contains(&"skill_screen_ocr"),
        "expected skill_screen_ocr in core-common build"
    );
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

#[cfg(feature = "windows")]
#[test]
fn windows_bundle_includes_windows_frontend_skill_and_core_common() {
    for core_skill in CORE_COMMON_SKILLS {
        assert!(ENABLED_SKILL_IDS.contains(&core_skill));
    }
    assert!(ENABLED_SKILL_IDS.contains(&"skill_windows_apple_music"));
}

#[cfg(feature = "ios")]
#[test]
fn ios_bundle_currently_only_includes_core_common_skills() {
    assert_eq!(ENABLED_SKILL_IDS, CORE_COMMON_SKILLS);
}
