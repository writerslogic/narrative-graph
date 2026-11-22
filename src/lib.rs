pub mod error;
pub mod heuristic;
pub mod types;

pub use error::{NarrativeGraphError, Result};
pub use heuristic::{
    analyze, analyze_in, extract_aggregates, extract_aggregates_in, extract_candidate_triples,
    extract_candidate_triples_in, find_conflicts, find_conflicts_in, Context,
};
pub use types::{
    AggregateTriple, Analysis, Conflict, ConflictKind, Options, Polarity, Rejection,
    TripleCandidate,
};

#[cfg(feature = "node-api")]
pub mod napi_bindings;
