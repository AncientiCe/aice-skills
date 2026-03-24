//! Core skills: umbrella crate that links to skill crates and re-exports their API.
//!
//! Consumers (e.g. desktop-runner) depend only on `core-skills` to use any registered skill.
//! The backend can import `ALL_SKILL_IDS` for classifier prompt construction without executing skills.

/// Canonical skill IDs understood by the classifier and the frontend runtime protocol.
/// This is the single source of truth for which skills exist in the system.
pub const ALL_SKILL_IDS: &[&str] = &[
    "skill_weather",
    "skill_time",
    "skill_distance",
    "skill_smart_home",
    "skill_memory",
    "skill_assistant",
    "skill_media",
    "skill_computer",
    "skill_screenshot",
    "skill_app_switcher",
    "skill_reminder",
    "skill_timer",
    "skill_shopping_list",
    "skill_message",
    "skill_volume",
];

pub use skill_app_switcher::{
    AppSwitcherResult, AppSwitcherSkill, AppSwitcherSkillError, MacOsAppSwitcherSkill,
    MockAppSwitcherSkill,
};
pub use skill_assistant::{
    AssistantItem, AssistantResult, AssistantSkill, AssistantSkillError, MockAssistantSkill,
};
pub use skill_computer::{
    ComputerResult, ComputerSkill, ComputerSkillError, MacOsComputerSkill, MockComputerSkill,
};
pub use skill_distance::{
    DistanceResult, DistanceSkill, DistanceSkillError, MockDistanceSkill, OpenMeteoDistanceSkill,
};
pub use skill_media::{MacOsMusicSkill, MediaResult, MediaSkill, MediaSkillError, MockMediaSkill};
pub use skill_memory::{
    MemoryFact, MemoryResult, MemorySkill, MemorySkillError, MockMemorySkill, SqliteMemorySkill,
};
pub use skill_message::{
    MacOsMessagesSkill, MessageResult, MessageSkill, MessageSkillError, MockMessageSkill,
};
pub use skill_reminder::{
    MacOsReminderSkill, MockReminderSkill, ReminderResult, ReminderSkill, ReminderSkillError,
};
pub use skill_screenshot::{
    MacOsScreenshotSkill, MockScreenshotSkill, ScreenshotResult, ScreenshotSkill,
    ScreenshotSkillError,
};
pub use skill_shopping_list::{
    MacOsNotesShoppingListSkill, MockShoppingListSkill, ShoppingListResult, ShoppingListSkill,
    ShoppingListSkillError,
};
pub use skill_smart_home::{
    DeviceState, HueSmartHomeSkill, MockSmartHomeSkill, SmartHomeResult, SmartHomeSkill,
    SmartHomeSkillError,
};
pub use skill_time::{MockTimeSkill, OpenMeteoTimeSkill, TimeResult, TimeSkill, TimeSkillError};
pub use skill_timer::{
    MacOsClockTimerSkill, MockTimerSkill, TimerResult, TimerSkill, TimerSkillError,
};
pub use skill_volume::{
    MacOsVolumeSkill, MockVolumeSkill, VolumeResult, VolumeSkill, VolumeSkillError,
};
pub use skill_weather::{
    MockWeatherSkill, OpenMeteoWeatherSkill, ResolvedLocation, WeatherResult, WeatherSkill,
    WeatherSkillError,
};
