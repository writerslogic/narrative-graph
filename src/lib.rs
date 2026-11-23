pub mod error;
pub mod heuristic;
pub mod types;

pub use error::{NarrativeGraphError, Result};
#[cfg(feature = "model")]
pub use heuristic::model::StudentModel;
pub use heuristic::{
    analyze, analyze_in, extract_aggregates, extract_aggregates_in, extract_candidate_triples,
    extract_candidate_triples_in, find_conflicts, find_conflicts_in, Context,
};
pub use types::{
    AggregateTriple, Analysis, Conflict, ConflictKind, Language, Options, Polarity, Rejection,
    TripleCandidate,
};

#[cfg(feature = "node-api")]
pub mod napi_bindings;
