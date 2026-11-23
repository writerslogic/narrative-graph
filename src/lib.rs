mod calibration;
pub mod error;
pub mod heuristic;
mod series;
pub mod types;

pub use calibration::{rule_calibration, CalibrationGroup, RuleCalibration};
pub use error::{NarrativeGraphError, Result};
pub use heuristic::{
    analyze, analyze_document, analyze_document_in, analyze_in, extract_aggregates,
    extract_aggregates_in, extract_candidate_triples, extract_candidate_triples_in, find_conflicts,
    find_conflicts_in, Context,
};
pub use series::{ChapterPosition, ChapterSpan, Series, SeriesAggregate};
pub use types::{
    AggregateTriple, Analysis, CharacterMention, CharacterProfile, Conflict, ConflictKind,
    Dialogue, DialogueLine, DocumentAnalysis, EventCandidate, EventKind, FactOccurrence,
    FamilyLink, Gender, KnowledgeFact, KnowledgeKind, Language, MentionKind, Options, Polarity,
    Rejection, RelationshipState, TripleCandidate,
};

#[cfg(feature = "node-api")]
pub mod napi_bindings;
