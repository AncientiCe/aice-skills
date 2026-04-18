//! Screen OCR skill: capture a screenshot, OCR it, optionally answer a
//! question about the visible text using a local LLM.

use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct ScreenOcrQuery {
    pub question: Option<String>,
    pub filename: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScreenOcrResult {
    pub screenshot_path: PathBuf,
    pub text: String,
    pub answer: Option<String>,
}

impl ScreenOcrResult {
    pub fn to_prompt_context(&self) -> String {
        match &self.answer {
            Some(a) => format!(
                "Screen OCR ({}): answer = {}",
                self.screenshot_path.display(),
                a
            ),
            None => {
                let preview = preview(&self.text, 400);
                format!(
                    "Screen OCR ({}): {}",
                    self.screenshot_path.display(),
                    preview
                )
            }
        }
    }
}

fn preview(text: &str, max_chars: usize) -> String {
    let cleaned = text.trim();
    if cleaned.chars().count() <= max_chars {
        cleaned.to_string()
    } else {
        let truncated: String = cleaned.chars().take(max_chars - 3).collect();
        format!("{truncated}...")
    }
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum ScreenOcrError {
    #[error("screenshot failed: {0}")]
    Screenshot(String),
    #[error("ocr engine unavailable: {0}")]
    OcrUnavailable(String),
    #[error("ocr produced no text")]
    EmptyOcr,
    #[error("llm unavailable: {0}")]
    LlmUnavailable(String),
}

#[async_trait]
pub trait ScreenOcrSkill: Send + Sync {
    async fn execute(&self, query: &ScreenOcrQuery) -> Result<ScreenOcrResult, ScreenOcrError>;
}

/// Pluggable OCR engine. Real implementations (Tesseract, Apple Vision, etc.)
/// live outside this crate so the runtime can pick what suits the platform.
#[async_trait]
pub trait OcrEngine: Send + Sync {
    async fn recognize(&self, image_path: &std::path::Path) -> Result<String, String>;
}

/// Pluggable LLM answer composer over OCR text.
#[async_trait]
pub trait ScreenOcrLlm: Send + Sync {
    async fn answer(&self, question: &str, ocr_text: &str) -> Result<String, String>;
}
