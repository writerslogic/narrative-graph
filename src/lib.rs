pub mod error;
pub mod heuristic;
pub mod types;

pub use error::{NarrativeGraphError, Result};
pub use heuristic::{extract_aggregates, extract_candidate_triples, find_conflicts};
pub use types::{
    AggregateTriple, Conflict, ConflictKind, Options, Polarity, Rejection, TripleCandidate,
};

#[cfg(feature = "node-api")]
pub mod napi_bindings;
