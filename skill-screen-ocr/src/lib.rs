//! Screen OCR skill: capture -> OCR -> optional Q&A.

mod types;

pub use types::{
    OcrEngine, ScreenOcrError, ScreenOcrLlm, ScreenOcrQuery, ScreenOcrResult, ScreenOcrSkill,
};

use async_trait::async_trait;
use metrics::{counter, histogram};
use skill_screenshot::{ScreenshotSkill, ScreenshotSkillError};
use std::sync::Arc;
use std::time::Instant;

const SKILL_EXECUTE_TOTAL: &str = "backend_skill_execute_total";
const SKILL_EXECUTE_DURATION_SECONDS: &str = "backend_skill_execute_duration_seconds";
const SKILL_LABEL: &str = "screen_ocr";

pub struct ComposedScreenOcrSkill {
    screenshot: Arc<dyn ScreenshotSkill>,
    ocr: Arc<dyn OcrEngine>,
    llm: Option<Arc<dyn ScreenOcrLlm>>,
}

impl ComposedScreenOcrSkill {
    pub fn new(screenshot: Arc<dyn ScreenshotSkill>, ocr: Arc<dyn OcrEngine>) -> Self {
        Self {
            screenshot,
            ocr,
            llm: None,
        }
    }

    pub fn with_llm(mut self, llm: Arc<dyn ScreenOcrLlm>) -> Self {
        self.llm = Some(llm);
        self
    }

    fn map_screenshot_error(e: ScreenshotSkillError) -> ScreenOcrError {
        ScreenOcrError::Screenshot(e.to_string())
    }

    fn error_kind(error: &ScreenOcrError) -> &'static str {
        match error {
            ScreenOcrError::Screenshot(_) => "screenshot",
            ScreenOcrError::OcrUnavailable(_) => "ocr_unavailable",
            ScreenOcrError::EmptyOcr => "empty_ocr",
            ScreenOcrError::LlmUnavailable(_) => "llm_unavailable",
        }
    }
}

#[async_trait]
impl ScreenOcrSkill for ComposedScreenOcrSkill {
    async fn execute(&self, query: &ScreenOcrQuery) -> Result<ScreenOcrResult, ScreenOcrError> {
        let started_at = Instant::now();
        let result = async {
            let shot = self
                .screenshot
                .execute(query.filename.as_deref())
                .await
                .map_err(Self::map_screenshot_error)?;
            let text = self
                .ocr
                .recognize(&shot.path)
                .await
                .map_err(ScreenOcrError::OcrUnavailable)?;
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                return Err(ScreenOcrError::EmptyOcr);
            }
            let answer = match (&query.question, &self.llm) {
                (Some(q), Some(llm)) if !q.trim().is_empty() => Some(
                    llm.answer(q.trim(), &trimmed)
                        .await
                        .map_err(ScreenOcrError::LlmUnavailable)?,
                ),
                _ => None,
            };
            Ok(ScreenOcrResult {
                screenshot_path: shot.path,
                text: trimmed,
                answer,
            })
        }
        .await;

        let mut result_label = "success";
        let mut error_kind = "none";
        if let Err(e) = &result {
            result_label = "error";
            error_kind = Self::error_kind(e);
        }
        counter!(
            SKILL_EXECUTE_TOTAL,
            1,
            "skill" => SKILL_LABEL,
            "result" => result_label,
            "error_kind" => error_kind
        );
        histogram!(
            SKILL_EXECUTE_DURATION_SECONDS,
            started_at.elapsed().as_secs_f64(),
            "skill" => SKILL_LABEL
        );
        result
    }
}

pub struct MockScreenOcrSkill {
    pub result: Result<ScreenOcrResult, ScreenOcrError>,
}

impl MockScreenOcrSkill {
    pub fn ok(result: ScreenOcrResult) -> Self {
        Self { result: Ok(result) }
    }

    pub fn err(error: ScreenOcrError) -> Self {
        Self { result: Err(error) }
    }
}

#[async_trait]
impl ScreenOcrSkill for MockScreenOcrSkill {
    async fn execute(&self, _query: &ScreenOcrQuery) -> Result<ScreenOcrResult, ScreenOcrError> {
        self.result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use skill_screenshot::{MockScreenshotSkill, ScreenshotResult};
    use std::path::{Path, PathBuf};

    struct StubOcr {
        text: String,
    }

    #[async_trait]
    impl OcrEngine for StubOcr {
        async fn recognize(&self, _path: &Path) -> Result<String, String> {
            Ok(self.text.clone())
        }
    }

    struct FailingOcr;

    #[async_trait]
    impl OcrEngine for FailingOcr {
        async fn recognize(&self, _path: &Path) -> Result<String, String> {
            Err("tesseract missing".to_string())
        }
    }

    struct StubLlm;

    #[async_trait]
    impl ScreenOcrLlm for StubLlm {
        async fn answer(&self, question: &str, ocr_text: &str) -> Result<String, String> {
            Ok(format!("Q={question}; saw={ocr_text}"))
        }
    }

    fn screenshot_at(path: PathBuf) -> Arc<dyn ScreenshotSkill> {
        Arc::new(MockScreenshotSkill::ok(ScreenshotResult { path }))
    }

    #[tokio::test]
    async fn returns_text_when_no_question() {
        let path = PathBuf::from("/tmp/shot.png");
        let skill = ComposedScreenOcrSkill::new(
            screenshot_at(path.clone()),
            Arc::new(StubOcr {
                text: "hello world".into(),
            }),
        );
        let res = skill
            .execute(&ScreenOcrQuery {
                question: None,
                filename: None,
            })
            .await
            .expect("ok");
        assert_eq!(res.text, "hello world");
        assert!(res.answer.is_none());
        assert_eq!(res.screenshot_path, path);
    }

    #[tokio::test]
    async fn answers_when_question_and_llm_provided() {
        let skill = ComposedScreenOcrSkill::new(
            screenshot_at(PathBuf::from("/tmp/shot.png")),
            Arc::new(StubOcr {
                text: "the quick brown fox".into(),
            }),
        )
        .with_llm(Arc::new(StubLlm));
        let res = skill
            .execute(&ScreenOcrQuery {
                question: Some("what color is the fox?".into()),
                filename: None,
            })
            .await
            .expect("ok");
        assert!(res.answer.is_some());
        assert!(res.answer.as_deref().unwrap().contains("brown fox"));
    }

    #[tokio::test]
    async fn empty_ocr_propagates_error() {
        let skill = ComposedScreenOcrSkill::new(
            screenshot_at(PathBuf::from("/tmp/shot.png")),
            Arc::new(StubOcr { text: "   ".into() }),
        );
        let err = skill
            .execute(&ScreenOcrQuery {
                question: None,
                filename: None,
            })
            .await
            .expect_err("err");
        assert_eq!(err, ScreenOcrError::EmptyOcr);
    }

    #[tokio::test]
    async fn ocr_failure_propagates() {
        let skill = ComposedScreenOcrSkill::new(
            screenshot_at(PathBuf::from("/tmp/shot.png")),
            Arc::new(FailingOcr),
        );
        let err = skill
            .execute(&ScreenOcrQuery {
                question: None,
                filename: None,
            })
            .await
            .expect_err("err");
        assert!(matches!(err, ScreenOcrError::OcrUnavailable(_)));
    }

    #[tokio::test]
    async fn screenshot_failure_propagates() {
        let bad_screenshot: Arc<dyn ScreenshotSkill> = Arc::new(MockScreenshotSkill::err(
            ScreenshotSkillError::Execution("permission denied".into()),
        ));
        let skill = ComposedScreenOcrSkill::new(
            bad_screenshot,
            Arc::new(StubOcr {
                text: "ignored".into(),
            }),
        );
        let err = skill
            .execute(&ScreenOcrQuery {
                question: None,
                filename: None,
            })
            .await
            .expect_err("err");
        assert!(matches!(err, ScreenOcrError::Screenshot(_)));
    }
}
