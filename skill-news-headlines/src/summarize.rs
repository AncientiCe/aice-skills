//! Opt-in per-headline LLM summarization with streaming.
//!
//! The backend wires a real `LlmStream` (Ollama) to a [`NewsSummaryLlm`] so
//! callers can either:
//!  - stream tokens as they arrive ([`stream_summaries`]) — useful for live
//!    text-to-speech / UI updates;
//!  - or collect final summaries once each headline finishes
//!    ([`collect_summaries`]) — useful for compact prompt context.
//!
//! Failures on individual headlines do not abort the whole stream: each
//! headline gets its own terminal `HeadlineEnd` event with an optional
//! `error` field set.

use crate::types::{NewsHeadline, NewsSummaryLlm, SummarizedHeadline};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Streamed summary events for one or more headlines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewsSummaryEvent {
    /// Emitted before any chunks for the headline at `index`.
    HeadlineStart {
        index: usize,
        headline: NewsHeadline,
    },
    /// One streamed chunk of summary text.
    Chunk { index: usize, text: String },
    /// Final event for the headline at `index`. `error` is `Some(...)` when
    /// the LLM failed; `summary` always contains the accumulated text so far.
    HeadlineEnd {
        index: usize,
        summary: String,
        error: Option<String>,
    },
    /// Emitted once after the last `HeadlineEnd`.
    Done,
}

/// Stream summary events for `headlines` to the returned channel.
///
/// The channel closes after a single [`NewsSummaryEvent::Done`] event. All
/// LLM calls happen on a spawned task so the caller can start consuming
/// chunks immediately.
pub fn stream_summaries(
    headlines: Vec<NewsHeadline>,
    llm: Arc<dyn NewsSummaryLlm>,
) -> mpsc::Receiver<NewsSummaryEvent> {
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(async move {
        for (index, headline) in headlines.into_iter().enumerate() {
            if tx
                .send(NewsSummaryEvent::HeadlineStart {
                    index,
                    headline: headline.clone(),
                })
                .await
                .is_err()
            {
                return;
            }

            let mut accumulated = String::new();
            let mut error: Option<String> = None;
            match llm.summarize(&headline).await {
                Ok(mut chunks) => {
                    while let Some(chunk) = chunks.recv().await {
                        accumulated.push_str(&chunk);
                        if tx
                            .send(NewsSummaryEvent::Chunk { index, text: chunk })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                Err(err) => {
                    error = Some(err.to_string());
                }
            }

            if tx
                .send(NewsSummaryEvent::HeadlineEnd {
                    index,
                    summary: accumulated,
                    error,
                })
                .await
                .is_err()
            {
                return;
            }
        }
        let _ = tx.send(NewsSummaryEvent::Done).await;
    });
    rx
}

/// Convenience: drain [`stream_summaries`] into one `SummarizedHeadline` per
/// input. Order is preserved.
pub async fn collect_summaries(
    headlines: Vec<NewsHeadline>,
    llm: Arc<dyn NewsSummaryLlm>,
) -> Vec<SummarizedHeadline> {
    let originals: Vec<NewsHeadline> = headlines.clone();
    let mut out: Vec<SummarizedHeadline> = originals
        .into_iter()
        .map(|h| SummarizedHeadline {
            headline: h,
            summary: None,
            summary_error: None,
        })
        .collect();
    let mut rx = stream_summaries(headlines, llm);
    while let Some(event) = rx.recv().await {
        match event {
            NewsSummaryEvent::HeadlineEnd {
                index,
                summary,
                error,
            } => {
                if let Some(slot) = out.get_mut(index) {
                    slot.summary = Some(summary);
                    slot.summary_error = error;
                }
            }
            NewsSummaryEvent::Done => break,
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NewsHeadlinesError;
    use async_trait::async_trait;

    fn h(title: &str) -> NewsHeadline {
        NewsHeadline {
            title: title.to_string(),
            source: None,
            url: None,
            published_at: None,
        }
    }

    struct ChunkLlm {
        chunks: Vec<&'static str>,
    }

    #[async_trait]
    impl NewsSummaryLlm for ChunkLlm {
        async fn summarize(
            &self,
            _headline: &NewsHeadline,
        ) -> Result<mpsc::Receiver<String>, NewsHeadlinesError> {
            let (tx, rx) = mpsc::channel(8);
            let chunks: Vec<String> = self.chunks.iter().map(|s| s.to_string()).collect();
            tokio::spawn(async move {
                for c in chunks {
                    if tx.send(c).await.is_err() {
                        return;
                    }
                }
            });
            Ok(rx)
        }
    }

    struct FailingLlm;

    #[async_trait]
    impl NewsSummaryLlm for FailingLlm {
        async fn summarize(
            &self,
            _headline: &NewsHeadline,
        ) -> Result<mpsc::Receiver<String>, NewsHeadlinesError> {
            Err(NewsHeadlinesError::ProviderUnavailable(
                "offline".to_string(),
            ))
        }
    }

    #[tokio::test]
    async fn stream_emits_start_chunks_end_done_in_order() {
        let llm: Arc<dyn NewsSummaryLlm> = Arc::new(ChunkLlm {
            chunks: vec!["Hello ", "world"],
        });
        let mut rx = stream_summaries(vec![h("topic")], llm);
        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }
        assert!(matches!(
            events[0],
            NewsSummaryEvent::HeadlineStart { index: 0, .. }
        ));
        assert!(matches!(
            events[1],
            NewsSummaryEvent::Chunk { index: 0, .. }
        ));
        assert!(matches!(
            events[2],
            NewsSummaryEvent::Chunk { index: 0, .. }
        ));
        match &events[3] {
            NewsSummaryEvent::HeadlineEnd {
                index: 0,
                summary,
                error,
            } => {
                assert_eq!(summary, "Hello world");
                assert!(error.is_none());
            }
            other => panic!("expected HeadlineEnd, got {:?}", other),
        }
        assert!(matches!(events.last(), Some(NewsSummaryEvent::Done)));
    }

    #[tokio::test]
    async fn collect_returns_summaries_in_input_order() {
        let llm: Arc<dyn NewsSummaryLlm> = Arc::new(ChunkLlm { chunks: vec!["ok"] });
        let out = collect_summaries(vec![h("a"), h("b")], llm).await;
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].headline.title, "a");
        assert_eq!(out[0].summary.as_deref(), Some("ok"));
        assert_eq!(out[1].headline.title, "b");
        assert_eq!(out[1].summary.as_deref(), Some("ok"));
    }

    #[tokio::test]
    async fn failing_llm_records_error_per_headline() {
        let llm: Arc<dyn NewsSummaryLlm> = Arc::new(FailingLlm);
        let out = collect_summaries(vec![h("a")], llm).await;
        assert_eq!(out.len(), 1);
        assert!(out[0].summary_error.is_some());
        assert_eq!(out[0].summary.as_deref(), Some(""));
    }
}
