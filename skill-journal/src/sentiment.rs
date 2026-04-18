//! Tiny lexicon-based sentiment classifier used as the offline default.
//!
//! Intentionally simple — when accuracy matters the runtime can pre-compute
//! sentiment with a local LLM and pass `Sentiment` explicitly to `Add`.

use crate::types::Sentiment;

const POSITIVE_WORDS: &[&str] = &[
    "good",
    "great",
    "happy",
    "love",
    "excited",
    "awesome",
    "wonderful",
    "calm",
    "grateful",
    "joy",
    "excellent",
    "amazing",
    "proud",
    "energized",
    "win",
    "wins",
    "won",
];

const NEGATIVE_WORDS: &[&str] = &[
    "bad",
    "sad",
    "angry",
    "tired",
    "stressed",
    "anxious",
    "frustrated",
    "exhausted",
    "annoyed",
    "hate",
    "terrible",
    "awful",
    "lonely",
    "afraid",
    "lose",
    "lost",
    "loss",
    "depressed",
    "down",
];

/// Classify a journal entry's sentiment from word counts.
pub fn classify(text: &str) -> Sentiment {
    let lowered = text.to_lowercase();
    let mut pos = 0i32;
    let mut neg = 0i32;
    for token in lowered.split(|c: char| !c.is_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        if POSITIVE_WORDS.contains(&token) {
            pos += 1;
        } else if NEGATIVE_WORDS.contains(&token) {
            neg += 1;
        }
    }
    match pos - neg {
        d if d > 0 => Sentiment::Positive,
        d if d < 0 => Sentiment::Negative,
        _ => Sentiment::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_positive() {
        assert_eq!(
            classify("Today was a great day, I felt happy and grateful."),
            Sentiment::Positive
        );
    }

    #[test]
    fn classifies_negative() {
        assert_eq!(
            classify("I'm tired and frustrated, everything feels awful."),
            Sentiment::Negative
        );
    }

    #[test]
    fn classifies_neutral_when_balanced() {
        assert_eq!(
            classify("Good morning, but tired by night."),
            Sentiment::Neutral
        );
    }

    #[test]
    fn classifies_neutral_when_no_words() {
        assert_eq!(
            classify("Walked to the store and bought milk."),
            Sentiment::Neutral
        );
    }
}
