//! Core skills: umbrella crate that links to skill crates and re-exports their API.
//!
//! Consumers (e.g. desktop-runner) depend only on `core-skills` to use enabled skill groups.
//! The backend can import `ENABLED_SKILL_IDS` for classifier prompt construction.

#[cfg(all(not(clippy), feature = "macos", feature = "windows"))]
compile_error!("features `macos` and `windows` cannot be enabled together");
#[cfg(all(not(clippy), feature = "macos", feature = "ios"))]
compile_error!("features `macos` and `ios` cannot be enabled together");
#[cfg(all(not(clippy), feature = "windows", feature = "ios"))]
compile_error!("features `windows` and `ios` cannot be enabled together");

/// Skill IDs enabled for the current feature selection.
pub static ENABLED_SKILL_IDS: &[&str] = &[
    #[cfg(feature = "core-common")]
    "skill_weather",
    #[cfg(feature = "core-common")]
    "skill_time",
    #[cfg(feature = "core-common")]
    "skill_distance",
    #[cfg(feature = "macos")]
    "skill_smart_home",
    #[cfg(feature = "macos")]
    "skill_memory",
    #[cfg(feature = "macos")]
    "skill_assistant",
    #[cfg(feature = "macos")]
    "skill_media",
    #[cfg(feature = "macos")]
    "skill_computer",
    #[cfg(feature = "macos")]
    "skill_record_window",
    #[cfg(feature = "macos")]
    "skill_screenshot",
    #[cfg(feature = "macos")]
    "skill_app_switcher",
    #[cfg(feature = "macos")]
    "skill_reminder",
    #[cfg(feature = "macos")]
    "skill_timer",
    #[cfg(feature = "macos")]
    "skill_shopping_list",
    #[cfg(feature = "macos")]
    "skill_message",
    #[cfg(feature = "macos")]
    "skill_volume",
];

/// Backward-compatible alias for enabled skills in the active build.
pub static ALL_SKILL_IDS: &[&str] = ENABLED_SKILL_IDS;

#[cfg(feature = "macos")]
pub use skill_app_switcher::{
    AppSwitcherResult, AppSwitcherSkill, AppSwitcherSkillError, MacOsAppSwitcherSkill,
    MockAppSwitcherSkill,
};
#[cfg(feature = "macos")]
pub use skill_assistant::{
    AssistantItem, AssistantResult, AssistantSkill, AssistantSkillError, MockAssistantSkill,
};
#[cfg(feature = "macos")]
pub use skill_computer::{
    ComputerResult, ComputerSkill, ComputerSkillError, MacOsComputerSkill, MockComputerSkill,
};
#[cfg(feature = "core-common")]
pub use skill_distance::{
    DistanceResult, DistanceSkill, DistanceSkillError, MockDistanceSkill, OpenMeteoDistanceSkill,
};
#[cfg(feature = "macos")]
pub use skill_media::{MacOsMusicSkill, MediaResult, MediaSkill, MediaSkillError, MockMediaSkill};
#[cfg(feature = "macos")]
pub use skill_memory::{
    MemoryFact, MemoryResult, MemorySkill, MemorySkillError, MockMemorySkill, SqliteMemorySkill,
};
#[cfg(feature = "macos")]
pub use skill_message::{
    MacOsMessagesSkill, MessageResult, MessageSkill, MessageSkillError, MockMessageSkill,
};
#[cfg(feature = "macos")]
pub use skill_record_window::{
    MacOsRecordWindowSkill, MockRecordWindowSkill, RecordWindowResult, RecordWindowSkill,
    RecordWindowSkillError,
};
#[cfg(feature = "macos")]
pub use skill_reminder::{
    MacOsReminderSkill, MockReminderSkill, ReminderResult, ReminderSkill, ReminderSkillError,
};
#[cfg(feature = "macos")]
pub use skill_screenshot::{
    MacOsScreenshotSkill, MockScreenshotSkill, ScreenshotResult, ScreenshotSkill,
    ScreenshotSkillError,
};
#[cfg(feature = "macos")]
pub use skill_shopping_list::{
    MacOsNotesShoppingListSkill, MockShoppingListSkill, ShoppingListResult, ShoppingListSkill,
    ShoppingListSkillError,
};
#[cfg(feature = "macos")]
pub use skill_smart_home::{
    DeviceState, HueSmartHomeSkill, MockSmartHomeSkill, SmartHomeResult, SmartHomeSkill,
    SmartHomeSkillError,
};
#[cfg(feature = "core-common")]
pub use skill_time::{MockTimeSkill, OpenMeteoTimeSkill, TimeResult, TimeSkill, TimeSkillError};
#[cfg(feature = "macos")]
pub use skill_timer::{
    MacOsClockTimerSkill, MockTimerSkill, TimerResult, TimerSkill, TimerSkillError,
};
#[cfg(feature = "macos")]
pub use skill_volume::{
    MacOsVolumeSkill, MockVolumeSkill, VolumeResult, VolumeSkill, VolumeSkillError,
};
#[cfg(feature = "core-common")]
pub use skill_weather::{
    MockWeatherSkill, OpenMeteoWeatherSkill, ResolvedLocation, WeatherResult, WeatherSkill,
    WeatherSkillError,
};
