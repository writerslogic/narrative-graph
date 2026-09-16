pub mod error;
pub mod heuristic;
pub mod types;

pub use error::{NarrativeGraphError, Result};
pub use heuristic::{extract_aggregates, extract_candidate_triples};
pub use types::{AggregateTriple, Options, Rejection, TripleCandidate};

#[cfg(feature = "node-api")]
pub mod napi_bindings;
