//! Core skills for the macOS frontend runtime.
//!
//! Consumers (e.g. `apps/aice-macos`) depend only on `core-skills`
//! for the locally executed macOS skill set.

pub use skill_app_switcher::{
    AppSwitcherResult, AppSwitcherSkill, AppSwitcherSkillError, MacOsAppSwitcherSkill,
    MockAppSwitcherSkill,
};
pub use skill_computer::{
    ComputerResult, ComputerSkill, ComputerSkillError, MacOsComputerSkill, MockComputerSkill,
};
pub use skill_media::{MacOsMusicSkill, MediaResult, MediaSkill, MediaSkillError, MockMediaSkill};
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
pub use skill_timer::{
    MacOsClockTimerSkill, MockTimerSkill, TimerResult, TimerSkill, TimerSkillError,
};
pub use skill_volume::{
    MacOsVolumeSkill, MockVolumeSkill, VolumeResult, VolumeSkill, VolumeSkillError,
};
