pub mod error;
pub mod heuristic;
pub mod types;

pub use error::{NarrativeGraphError, Result};
pub use heuristic::{
    extract_aggregates, extract_aggregates_in, extract_candidate_triples,
    extract_candidate_triples_in, find_conflicts, find_conflicts_in, Context,
};
pub use types::{
    AggregateTriple, Conflict, ConflictKind, Options, Polarity, Rejection, TripleCandidate,
};

#[cfg(feature = "node-api")]
pub mod napi_bindings;
