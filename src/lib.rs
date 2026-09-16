pub mod error;
pub mod heuristic;
pub mod types;

pub use error::{NarrativeGraphError, Result};
pub use heuristic::extract_candidate_triples;
pub use types::{Options, Rejection, TripleCandidate};

#[cfg(feature = "node-api")]
pub mod napi_bindings;
