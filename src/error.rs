use thiserror::Error;

#[derive(Error, Debug)]
pub enum NarrativeGraphError {
    #[error("Invalid span range: [{start}..{end}] exceeds text length {len}")]
    InvalidSpan {
        start: usize,
        end: usize,
        len: usize,
    },

    #[error("Invalid UTF-8 in text")]
    Utf8Error(#[from] std::string::FromUtf8Error),

    #[error("JSON serialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Confidence threshold must be between 0.0 and 1.0, got {0}")]
    InvalidConfidenceThreshold(f32),
}

pub type Result<T> = std::result::Result<T, NarrativeGraphError>;
