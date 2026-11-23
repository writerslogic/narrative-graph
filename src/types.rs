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

/// The language a text is written in, which selects the reader.
///
/// English is read by capitalization and word-bounded verb rules. Chinese has
/// neither capitals nor spaces, so it has a reader of its own, which finds
/// names from the caller's `aliases` and from a surname followed by a title
/// (王夫人, 林姑娘). See `heuristic/chinese.rs` for what it reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
#[non_exhaustive]
pub enum Language {
    #[default]
    English,
    /// Traditional or simplified Chinese.
    Chinese,
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

    /// Optional mapping from recognized relation patterns to a caller-supplied controlled vocabulary.
    /// Example: { "loves": "romantic_interest", "is_married_to": "spouse" }
    /// If a relation type is not in this map, the heuristic default is used.
    #[cfg_attr(feature = "serde", serde(default))]
    pub ontology: BTreeMap<String, String>,

    /// The language of the text. Default: English.
    ///
    /// IMPORTANT: for Chinese, every key of `aliases` is matched as a literal
    /// run of characters with no word boundary, since the script has none.
    #[cfg_attr(feature = "serde", serde(default))]
    pub language: Language,
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

/// Everything one pass over a passage yields: what `extract_candidate_triples`,
/// `extract_aggregates` and `find_conflicts` return separately.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct Analysis {
    pub candidates: Vec<TripleCandidate>,
    pub aggregates: Vec<AggregateTriple>,
    pub conflicts: Vec<Conflict>,
}

/// The document's agreeing gender cues for a named character, when available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub enum Gender {
    Feminine,
    Masculine,
}

/// A family role held by this character with respect to another character.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct FamilyLink {
    pub role: String,
    pub relative: String,
}

/// Names, descriptions and family evidence used to resolve a character's mentions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct CharacterProfile {
    pub id: String,
    pub aliases: Vec<String>,
    pub gender: Option<Gender>,
    pub family: Vec<FamilyLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub enum MentionKind {
    Name,
    Pronoun,
    Description,
}

/// A source span linked to a character ID.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct CharacterMention {
    pub id: String,
    pub kind: MentionKind,
    pub span: [usize; 2],
}

/// Relation analysis and character evidence for one document.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct DocumentAnalysis {
    pub analysis: Analysis,
    pub characters: Vec<CharacterProfile>,
    pub mentions: Vec<CharacterMention>,
    /// Individual relation observations, in source order, including restatements.
    pub timeline: Vec<FactOccurrence>,
    /// Event occurrences, in source order.
    pub events: Vec<EventCandidate>,
    /// Knowledge and communication statements with their source content.
    pub knowledge: Vec<KnowledgeFact>,
    pub dialogue: Dialogue,
}

/// One quoted utterance and any supported attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct DialogueLine {
    pub speaker: Option<String>,
    pub listener: Option<String>,
    /// Byte range covering the utterance in the input.
    pub span: [usize; 2],
    /// Quoted byte ranges, excluding any interleaved attribution.
    pub spoken_spans: Vec<[usize; 2]>,
}

/// Dialogue lines in source order, including lines with incomplete attribution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct Dialogue {
    pub lines: Vec<DialogueLine>,
    /// Number of lines with an unknown speaker or listener.
    pub unattributed: usize,
}

impl Dialogue {
    pub(crate) fn from_lines(mut lines: Vec<DialogueLine>) -> Self {
        lines.sort_by_key(|line| line.span);
        let unattributed = lines
            .iter()
            .filter(|line| line.speaker.is_none() || line.listener.is_none())
            .count();
        Self {
            lines,
            unattributed,
        }
    }
}

/// A time-varying relationship described by a fact in the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub enum RelationshipState {
    Engaged,
    Married,
    Widowed,
    Divorced,
    Friends,
    Enemies,
    Allies,
}

impl RelationshipState {
    pub(crate) fn from_relation(relation: &str) -> Option<Self> {
        Some(match relation {
            "engaged_to" | "fiance_of" => Self::Engaged,
            "spouse_of" | "wife_of" | "husband_of" => Self::Married,
            "widow_of" => Self::Widowed,
            "divorced" | "ex_wife_of" | "ex_husband_of" => Self::Divorced,
            "friend_of" => Self::Friends,
            "enemy_of" => Self::Enemies,
            "ally_of" => Self::Allies,
            _ => return None,
        })
    }
}

/// One statement about a relationship at a particular position in the text.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct FactOccurrence {
    pub candidate: TripleCandidate,
    pub polarity: Polarity,
    pub state: Option<RelationshipState>,
    /// An explicit end to a state, such as "no longer friends".
    pub ends_state: bool,
    /// Observed correctness of comparable predictions in the public calibration set.
    pub calibrated_confidence: Option<f32>,
}

impl DocumentAnalysis {
    /// All state observations about this pair, in source order.
    pub fn relationship_history<'a>(
        &'a self,
        person: &'a str,
        other: &'a str,
    ) -> impl Iterator<Item = &'a FactOccurrence> {
        self.timeline.iter().filter(move |entry| {
            let c = &entry.candidate;
            entry.state.is_some()
                && ((c.subject == person && c.object == other)
                    || (c.subject == other && c.object == person))
        })
    }

    /// Observations with a supported estimate at or above the requested probability.
    pub fn facts_at_least(
        &self,
        probability: f32,
    ) -> crate::Result<impl Iterator<Item = &FactOccurrence>> {
        if !(0.0..=1.0).contains(&probability) {
            return Err(crate::NarrativeGraphError::InvalidConfidenceThreshold(
                probability,
            ));
        }
        Ok(self.timeline.iter().filter(move |entry| {
            entry
                .calibrated_confidence
                .is_some_and(|estimate| estimate >= probability)
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub enum EventKind {
    Duel,
    Proposal,
    Journey,
    Death,
}

/// One event stated in the text, with canonical participants and a byte span.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct EventCandidate {
    pub kind: EventKind,
    pub actor: String,
    pub target: Option<String>,
    pub location: Option<String>,
    pub span: [usize; 2],
    /// The words that name the event in the source text.
    pub trigger: [usize; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub enum KnowledgeKind {
    Learned,
    Known,
    Told,
}

/// A statement made about what a character knows, learns or tells someone.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct KnowledgeFact {
    pub kind: KnowledgeKind,
    pub actor: String,
    pub recipient: Option<String>,
    pub topic: Option<String>,
    pub span: [usize; 2],
    /// The proposition or topic phrase in the original text.
    pub content: [usize; 2],
}
