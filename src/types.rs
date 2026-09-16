use std::collections::BTreeMap;

/// A candidate relational fact extracted from text.
/// Every candidate carries confidence, provenance span, and the rule that produced it.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct TripleCandidate {
    /// The subject entity (normalized to lowercase with underscores for multi-word entities).
    pub subject: String,
    /// The relation type (normalized from verb phrases to a controlled vocabulary).
    pub relation: String,
    /// The object entity (normalized to lowercase with underscores for multi-word entities).
    pub object: String,
    /// Confidence score from 0.0 to 1.0, derived from rule strength and entity proximity.
    pub confidence: f32,
    /// Byte range [start, end) in the input text where this candidate was extracted.
    pub span: [usize; 2],
    /// The name of the extraction rule that produced this candidate.
    /// Examples: "possessive-sister-pattern", "verb-mentor-pattern".
    pub rule: String,
}

/// Configuration for entity and relation extraction.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct Options {
    /// Optional mapping from surface forms to canonical entity names.
    /// Example: { "Marcus": "marcus_hale", "Mr Marcus Hale": "marcus_hale" }
    /// All mentions matching a key in this map resolve to the value.
    ///
    /// IMPORTANT: a key is matched against the mention's surface form exactly,
    /// which is the capitalized run with `.` dropped as a separator: "Mr Marcus
    /// Hale", never "mr. marcus hale". A lowercase phrase such as "the
    /// detective" is not detected as a mention at all, so it can never be a
    /// key.
    #[cfg_attr(feature = "serde", serde(default))]
    pub aliases: BTreeMap<String, String>,

    /// Minimum confidence threshold for returned candidates.
    /// Candidates below this score are filtered out.
    /// Default: 0.0 (all candidates returned).
    #[cfg_attr(feature = "serde", serde(default))]
    pub min_confidence: Option<f32>,

    /// Optional mapping from recognized relation patterns to a caller-supplied controlled vocabulary.
    /// Example: { "loves": "romantic_interest", "is_married_to": "spouse" }
    /// If a relation type is not in this map, the heuristic default is used.
    #[cfg_attr(feature = "serde", serde(default))]
    pub ontology: BTreeMap<String, String>,
}

/// A reference span into the input text with its corresponding surface text.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct SpannedTriple {
    pub candidate: TripleCandidate,
    /// The actual text from input[span[0]..span[1]].
    pub text: String,
}
