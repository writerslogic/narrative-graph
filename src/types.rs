use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// A candidate relational fact extracted from text.
/// Every candidate carries confidence, provenance span, and the rule that produced it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TripleCandidate {
    /// The subject entity (normalized to lowercase with underscores for multi-word entities).
    pub subject: String,
    /// The relation type (normalized from verb phrases to a controlled vocabulary).
    pub relation: String,
    /// The object entity (normalized to lowercase with underscores for multi-word entities).
    pub object: String,
    /// Confidence score from 0.0 to 1.0, derived from co-occurrence strength and pattern specificity.
    pub confidence: f32,
    /// Byte range [start, end) in the input text where this candidate was extracted.
    pub span: [usize; 2],
    /// The name of the extraction rule that produced this candidate.
    /// Examples: "possessive-sister-pattern", "verb-mentor-pattern", "cooccurrence-fallback".
    pub rule: String,
}

/// Configuration for entity and relation extraction.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Options {
    /// Optional mapping from surface forms to canonical entity names.
    /// Example: { "the detective": "marcus", "ms. chen": "chen" }
    /// All mentions matching a key in this map resolve to the value.
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,

    /// Minimum confidence threshold for returned candidates.
    /// Candidates below this score are filtered out.
    /// Default: 0.0 (all candidates returned).
    #[serde(default)]
    pub min_confidence: Option<f32>,

    /// Optional mapping from recognized relation patterns to a caller-supplied controlled vocabulary.
    /// Example: { "loves": "romantic_interest", "is_married_to": "spouse" }
    /// If a relation type is not in this map, the heuristic default is used.
    #[serde(default)]
    pub ontology: BTreeMap<String, String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            aliases: BTreeMap::new(),
            min_confidence: None,
            ontology: BTreeMap::new(),
        }
    }
}

/// A reference span into the input text with its corresponding surface text.
/// Returned from `extract_candidate_triples_with_text`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpannedTriple {
    pub candidate: TripleCandidate,
    /// The actual text from input[span[0]..span[1]].
    pub text: String,
}
