use std::fmt;

#[derive(Debug)]
pub enum NarrativeGraphError {
    InvalidConfidenceThreshold(f32),
}

impl fmt::Display for NarrativeGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NarrativeGraphError::InvalidConfidenceThreshold(value) => write!(
                f,
                "Confidence threshold must be between 0.0 and 1.0, got {value}"
            ),
        }
    }
}

impl std::error::Error for NarrativeGraphError {}

pub type Result<T> = std::result::Result<T, NarrativeGraphError>;
