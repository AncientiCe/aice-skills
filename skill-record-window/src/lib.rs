//! Record window skill: capture a specific macOS app window with audio.

mod macos_record_window;
mod types;

pub use macos_record_window::MacOsRecordWindowSkill;
pub use types::{RecordWindowResult, RecordWindowSkill, RecordWindowSkillError};

/// Mock implementation for tests.
pub struct MockRecordWindowSkill {
    pub result: Result<RecordWindowResult, RecordWindowSkillError>,
}

impl MockRecordWindowSkill {
    pub fn ok(result: RecordWindowResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(e: RecordWindowSkillError) -> Self {
        Self { result: Err(e) }
    }
}

#[async_trait::async_trait]
impl RecordWindowSkill for MockRecordWindowSkill {
    async fn execute(
        &self,
        _app_name: Option<&str>,
        _filename: Option<&str>,
        _duration_secs: Option<u64>,
    ) -> Result<RecordWindowResult, RecordWindowSkillError> {
        self.result.clone()
    }
}
