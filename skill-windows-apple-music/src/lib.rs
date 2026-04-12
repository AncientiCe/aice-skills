//! Windows Apple Music frontend skill using embedded Apple Music internals.

mod api;
mod session;
mod types;
mod webview_player;
mod windows_apple_music;

pub use types::{WindowsAppleMusic, WindowsAppleMusicResult, WindowsAppleMusicSkillError};
pub use windows_apple_music::{MockWindowsAppleMusicSkill, WindowsAppleMusicSkill};
