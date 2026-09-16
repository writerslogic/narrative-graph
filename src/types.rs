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

/// A candidate the caller has judged wrong, named by the triple it emitted.
///
/// IMPORTANT: the three fields are the *normalized* values as they appeared on
/// the `TripleCandidate`, which for `relation` means after any `ontology`
/// mapping. What the caller saw is what the caller rejects; nothing here is
/// re-derived from the source text.
///
/// Keying on the triple rather than the span is deliberate. A span does not
/// survive an edit to the manuscript, so a span-keyed rejection would come back
/// on the next save; a triple survives every edit that does not restate the
/// fact. The cost is that the rejection is document-wide: a caller who meant
/// "not in this passage" suppresses the claim everywhere, which is the blunter
/// and more inspectable of the two errors.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct Rejection {
    pub subject: String,
    pub relation: String,
    pub object: String,
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
    /// IMPORTANT: the key's own shape selects how it matches. A key carrying an
    /// uppercase character is matched against the mention's surface form
    /// exactly, which is the capitalized run with `.` dropped as a separator:
    /// "Mr Marcus Hale", never "Mr. Marcus Hale". A wholly lowercase key is a
    /// lexicon entry instead: the phrase is searched for in the text as a
    /// literal, word-bounded, case-insensitive match and becomes a mention of
    /// the canonical name there, which is how "the detective" reaches a
    /// referent no capitalized-run detector can see. A lexicon match that
    /// overlaps a mention already detected is dropped, so a key can add a
    /// mention but never replace one. A pronoun can take a lexicon mention as
    /// its referent, and a pronoun the mention covers is replaced by it.
    #[cfg_attr(feature = "serde", serde(default))]
    pub aliases: BTreeMap<String, String>,

    /// Minimum confidence threshold for returned candidates.
    /// Candidates below this score are filtered out.
    /// Default: 0.0 (all candidates returned).
    #[cfg_attr(feature = "serde", serde(default))]
    pub min_confidence: Option<f32>,

    /// Candidates the caller has already judged wrong. A candidate whose
    /// (subject, relation, object) matches one of these is not emitted.
    ///
    /// IMPORTANT: the crate persists nothing. This set is caller-owned state
    /// handed in per call, so where it is stored between runs, and for how
    /// long, stays the caller's decision.
    ///
    /// A rejection suppresses and does nothing else: it does not lower the
    /// confidence of a sibling candidate, and it does not weaken the rule that
    /// produced it. Feeding rejections back into scoring would make the output
    /// depend on a history the caller cannot see in the result, and the span
    /// and rule name are on every candidate precisely so a caller can audit
    /// why it fired.
    #[cfg_attr(feature = "serde", serde(default))]
    pub rejections: Vec<Rejection>,

    /// Let a pronoun with no antecedent in its own sentence take one from the
    /// sentence before it, within the same paragraph. Default: off.
    ///
    /// It buys the relation in "Elena grew up in the Archive. She runs it now.",
    /// which is invisible without it because the second sentence resolves
    /// nothing and so contributes no candidate at all.
    ///
    /// IMPORTANT: off by default because it is measurably lossy today. The
    /// pipeline has no gender or number agreement, so the fallback picks by
    /// position alone, and over three novels (5014 paragraphs) turning it on
    /// adds exactly one triple, which is wrong: "He" there refers to a man
    /// named three sentences earlier and resolves to the woman named in the
    /// sentence before. Agreement is the prerequisite, not a wider window —
    /// the right antecedent was out of reach of any one-sentence lookback.
    ///
    /// Turn it on for prose with few characters per scene, or where recall
    /// matters more than precision and a caller filters afterwards.
    #[cfg_attr(feature = "serde", serde(default))]
    pub cross_sentence_pronouns: bool,

    /// Optional mapping from recognized relation patterns to a caller-supplied controlled vocabulary.
    /// Example: { "loves": "romantic_interest", "is_married_to": "spouse" }
    /// If a relation type is not in this map, the heuristic default is used.
    #[cfg_attr(feature = "serde", serde(default))]
    pub ontology: BTreeMap<String, String>,
}

/// Whether a document states a fact or states that it does not hold.
///
/// IMPORTANT: a denial is a claim, not a missing assertion. Text that neither
/// asserts nor denies — a conditional, a modal, a question, a complement held
/// open by its main verb — produces no aggregate at all and so has no polarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub enum Polarity {
    Asserted,
    Denied,
}

/// One fact as a whole document states it, with every piece of evidence for it.
///
/// `TripleCandidate` is per-sentence and `extract_candidate_triples` keeps the
/// highest-confidence one per triple, discarding the spans of the rest. An
/// aggregate keeps them: a passage stating one fact three ways yields one
/// aggregate carrying three spans.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct AggregateTriple {
    pub subject: String,
    pub relation: String,
    pub object: String,
    /// Whether the document states this fact or denies it. A denial and the
    /// assertion of the same triple are two aggregates, not one, because they
    /// are two claims — and a document carrying both is the contradiction
    /// `find_conflicts` reports.
    pub polarity: Polarity,
    /// The highest confidence of any supporting candidate.
    ///
    /// IMPORTANT: repetition does not raise it, and `spans.len()` is the only
    /// thing that reports corroboration. In fiction a restatement is not
    /// independent evidence — an unreliable narrator restates a falsehood as
    /// readily as a reliable one states a fact — so summing would let a
    /// thrice-repeated lie outrank a once-stated fact, and averaging would
    /// punish a strong statement for being echoed by a weaker phrasing. The
    /// score answers "how good is the best evidence", not "how much is there".
    pub confidence: f32,
    /// Every span the fact was found at, in document order. Never empty.
    pub spans: Vec<[usize; 2]>,
    /// The rules that produced those spans, first occurrence first, without
    /// repeats. Two spans from one rule name it once.
    pub rules: Vec<String>,
}

/// Why two aggregates cannot both be true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub enum ConflictKind {
    /// The document asserts a fact and denies the same fact.
    Denial,
    /// The relation admits one subject per object, and two subjects are
    /// asserted over the same object.
    Cardinality,
}

/// Two claims a single passage cannot both support, with the evidence for each.
///
/// IMPORTANT: a conflict is not a ranking. Neither side is the true one, and
/// the crate has no way to decide which is: both carry their spans so the
/// decision stays with whoever can read the manuscript.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct Conflict {
    pub kind: ConflictKind,
    /// The claim appearing first in the document.
    pub left: AggregateTriple,
    /// The claim appearing second.
    pub right: AggregateTriple,
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
