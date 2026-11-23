//! Node bindings.
//!
//! IMPORTANT: every span crossing this boundary is in UTF-16 code units, the
//! unit `String.prototype.slice` takes. The Rust API reports UTF-8 byte
//! offsets; the two agree only on ASCII text.

use napi::bindgen_prelude::{AsyncTask, External};
use napi::{Env, Task};
use napi_derive::napi;

use crate::heuristic::{
    analyze, analyze_document, extract_aggregates, extract_candidate_triples, find_conflicts,
};
use crate::types::{
    AggregateTriple, Analysis, CharacterProfile, Conflict, ConflictKind, DocumentAnalysis,
    EventKind, FactOccurrence, Gender, KnowledgeKind, Language, MentionKind, Options, Polarity,
    Rejection, RelationshipState, TripleCandidate,
};
use crate::{ChapterPosition, Series};
use std::collections::BTreeMap;

#[napi]
pub fn extract_candidate_triples_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<Vec<NapiTripleCandidate>> {
    let options = to_options(opts)?;
    let offsets = Utf16Offsets::new(&text)?;
    Ok(extract_candidate_triples(&text, &options)
        .map_err(failure)?
        .into_iter()
        .map(|c| to_napi_candidate(c, &offsets))
        .collect())
}

/// One fact as the whole passage states it, with every supporting span.
/// Ordered by first span, so two facts about the same pair can be read in the
/// order the story states them.
#[napi]
pub fn extract_aggregates_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<Vec<NapiAggregateTriple>> {
    let options = to_options(opts)?;
    let offsets = Utf16Offsets::new(&text)?;
    Ok(extract_aggregates(&text, &options)
        .map_err(failure)?
        .into_iter()
        .map(|a| to_napi_aggregate(a, &offsets))
        .collect())
}

/// Claims the passage makes that cannot both be true: a fact and its denial,
/// or two subjects where the relation admits one. Both sides carry their spans.
#[napi]
pub fn find_conflicts_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<Vec<NapiConflict>> {
    let options = to_options(opts)?;
    let offsets = Utf16Offsets::new(&text)?;
    Ok(find_conflicts(&text, &options)
        .map_err(failure)?
        .into_iter()
        .map(|c| to_napi_conflict(c, &offsets))
        .collect())
}

/// Candidates, aggregates and conflicts from one pass over the text.
#[napi]
pub fn analyze_napi(text: String, opts: Option<NapiOptions>) -> napi::Result<NapiAnalysis> {
    let options = to_options(opts)?;
    let analysis = analyze(&text, &options).map_err(failure)?;
    to_napi_analysis(analysis, &text)
}

#[napi]
pub fn analyze_document_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<NapiDocumentAnalysis> {
    let options = to_options(opts)?;
    let offsets = Utf16Offsets::new(&text)?;
    let document = analyze_document(&text, &options).map_err(failure)?;
    Ok(to_napi_document(document, &offsets))
}

/// `analyzeNapi` on the libuv worker pool, so a long manuscript does not block
/// the calling JavaScript thread. Options are validated before the task is
/// queued.
#[napi(ts_return_type = "Promise<NapiAnalysis>")]
pub fn analyze_async_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<AsyncTask<AnalyzeTask>> {
    let options = to_options(opts)?;
    Ok(AsyncTask::new(AnalyzeTask { text, options }))
}

pub struct AnalyzeTask {
    text: String,
    options: Options,
}

impl Task for AnalyzeTask {
    type Output = NapiAnalysis;
    type JsValue = NapiAnalysis;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let analysis = analyze(&self.text, &self.options).map_err(failure)?;
        to_napi_analysis(analysis, &self.text)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

fn failure(e: impl std::fmt::Display) -> napi::Error {
    napi::Error::new(napi::Status::GenericFailure, e.to_string())
}

/// Maps UTF-8 byte offsets of one text to UTF-16 code unit offsets.
enum Utf16Offsets {
    /// ASCII text: both units coincide.
    Identity,
    /// Indexed by byte offset; entries off a char boundary are unused.
    Table(Vec<u32>),
    Sparse(BTreeMap<usize, u32>),
}

impl Utf16Offsets {
    fn new(text: &str) -> napi::Result<Self> {
        if u32::try_from(text.len()).is_err() {
            return Err(napi::Error::new(
                napi::Status::InvalidArg,
                "text exceeds 4 GiB",
            ));
        }
        if text.is_ascii() {
            return Ok(Self::Identity);
        }
        let mut table = vec![0u32; text.len() + 1];
        let mut units = 0u32;
        for (byte, c) in text.char_indices() {
            table[byte] = units;
            units += c.len_utf16() as u32;
        }
        table[text.len()] = units;
        Ok(Self::Table(table))
    }

    fn span(&self, span: [usize; 2]) -> Vec<u32> {
        span.iter()
            .map(|&offset| match self {
                // `new` bounded the text, and every span lies inside it.
                Self::Identity => offset as u32,
                Self::Table(table) => table[offset],
                Self::Sparse(table) => table[&offset],
            })
            .collect()
    }

    fn for_document(self, document: &DocumentAnalysis) -> Self {
        let Self::Table(table) = self else {
            return self;
        };
        let offsets = document
            .timeline
            .iter()
            .flat_map(|f| f.candidate.span)
            .chain(
                document
                    .events
                    .iter()
                    .flat_map(|e| e.span.into_iter().chain(e.trigger)),
            )
            .chain(document.mentions.iter().flat_map(|m| m.span))
            .chain(
                document
                    .dialogue
                    .lines
                    .iter()
                    .flat_map(|line| line.spoken_spans.iter().copied().flatten()),
            )
            .chain(
                document
                    .knowledge
                    .iter()
                    .flat_map(|k| k.span.into_iter().chain(k.content)),
            );
        Self::Sparse(offsets.map(|byte| (byte, table[byte])).collect())
    }
}

/// Validated on the caller's `f64`: narrowing first can round an out-of-range
/// value such as `1.00000001` into range.
fn threshold_from_js(value: f64) -> napi::Result<f32> {
    if !(0.0..=1.0).contains(&value) {
        return Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("Confidence threshold must be between 0.0 and 1.0, got {value}"),
        ));
    }
    Ok(value as f32)
}

/// The one place the Node option shape is mapped onto `Options`. The struct
/// literal is exhaustive, so a new `Options` field fails to compile here until
/// it is mapped.
fn to_options(opts: Option<NapiOptions>) -> napi::Result<Options> {
    let Some(napi_opts) = opts else {
        return Ok(Options::default());
    };
    Ok(Options {
        aliases: napi_opts.aliases.unwrap_or_default().into_iter().collect(),
        min_confidence: napi_opts
            .min_confidence
            .map(threshold_from_js)
            .transpose()?,
        rejections: napi_opts
            .rejections
            .unwrap_or_default()
            .into_iter()
            .map(|r| Rejection {
                subject: r.subject,
                relation: r.relation,
                object: r.object,
            })
            .collect(),
        ontology: napi_opts.ontology.unwrap_or_default().into_iter().collect(),
        language: match napi_opts.language.as_deref() {
            None | Some("english") => Language::English,
            Some("chinese") => Language::Chinese,
            Some(other) => {
                return Err(napi::Error::new(
                    napi::Status::InvalidArg,
                    format!("Unknown language {other:?}; expected \"english\" or \"chinese\""),
                ))
            }
        },
    })
}

fn to_napi_analysis(analysis: Analysis, text: &str) -> napi::Result<NapiAnalysis> {
    let offsets = Utf16Offsets::new(text)?;
    Ok(convert_analysis(analysis, &offsets))
}

fn convert_analysis(analysis: Analysis, offsets: &Utf16Offsets) -> NapiAnalysis {
    NapiAnalysis {
        candidates: analysis
            .candidates
            .into_iter()
            .map(|c| to_napi_candidate(c, offsets))
            .collect(),
        aggregates: analysis
            .aggregates
            .into_iter()
            .map(|a| to_napi_aggregate(a, offsets))
            .collect(),
        conflicts: analysis
            .conflicts
            .into_iter()
            .map(|c| to_napi_conflict(c, offsets))
            .collect(),
    }
}

fn polarity(polarity: Polarity) -> String {
    match polarity {
        Polarity::Asserted => "asserted",
        Polarity::Denied => "denied",
    }
    .to_string()
}

fn to_napi_candidate(c: TripleCandidate, offsets: &Utf16Offsets) -> NapiTripleCandidate {
    NapiTripleCandidate {
        span: offsets.span(c.span),
        subject: c.subject,
        relation: c.relation,
        object: c.object,
        confidence: c.confidence as f64,
        rule: c.rule,
    }
}

fn to_napi_aggregate(a: AggregateTriple, offsets: &Utf16Offsets) -> NapiAggregateTriple {
    NapiAggregateTriple {
        spans: a.spans.into_iter().map(|s| offsets.span(s)).collect(),
        subject: a.subject,
        relation: a.relation,
        object: a.object,
        polarity: polarity(a.polarity),
        confidence: a.confidence as f64,
        rules: a.rules,
    }
}

fn to_napi_conflict(c: Conflict, offsets: &Utf16Offsets) -> NapiConflict {
    NapiConflict {
        kind: match c.kind {
            ConflictKind::Denial => "denial".to_string(),
            ConflictKind::Cardinality => "cardinality".to_string(),
        },
        left: to_napi_aggregate(c.left, offsets),
        right: to_napi_aggregate(c.right, offsets),
    }
}

#[napi(object)]
pub struct NapiAnalysis {
    pub candidates: Vec<NapiTripleCandidate>,
    pub aggregates: Vec<NapiAggregateTriple>,
    pub conflicts: Vec<NapiConflict>,
}

#[napi(object)]
pub struct NapiAggregateTriple {
    pub subject: String,
    pub relation: String,
    pub object: String,
    /// "asserted" or "denied".
    pub polarity: String,
    pub confidence: f64,
    /// `[start, end)` in UTF-16 code units.
    pub spans: Vec<Vec<u32>>,
    pub rules: Vec<String>,
}

#[napi(object)]
pub struct NapiConflict {
    /// "denial" or "cardinality".
    pub kind: String,
    pub left: NapiAggregateTriple,
    pub right: NapiAggregateTriple,
}

#[napi(object)]
pub struct NapiTripleCandidate {
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub confidence: f64,
    /// `[start, end)` in UTF-16 code units.
    pub span: Vec<u32>,
    pub rule: String,
}

#[napi(object)]
pub struct NapiOptions {
    pub aliases: Option<std::collections::HashMap<String, String>>,
    pub min_confidence: Option<f64>,
    pub rejections: Option<Vec<NapiRejection>>,
    pub ontology: Option<std::collections::HashMap<String, String>>,
    /// `"english"` (the default) or `"chinese"`.
    pub language: Option<String>,
}

#[napi(object)]
pub struct NapiRejection {
    pub subject: String,
    pub relation: String,
    pub object: String,
}

#[napi(object)]
pub struct NapiDocumentAnalysis {
    pub analysis: NapiAnalysis,
    pub characters: Vec<NapiCharacterProfile>,
    pub mentions: Vec<NapiCharacterMention>,
    pub timeline: Vec<NapiFactOccurrence>,
    pub events: Vec<NapiEventCandidate>,
    pub knowledge: Vec<NapiKnowledgeFact>,
    pub dialogue: NapiDialogue,
}

#[napi(object)]
pub struct NapiDialogue {
    pub lines: Vec<NapiDialogueLine>,
    /// Lines with an unknown speaker or listener.
    pub unattributed: u32,
}

#[napi(object)]
pub struct NapiDialogueLine {
    pub speaker: Option<String>,
    pub listener: Option<String>,
    /// The utterance's bounding span in UTF-16 code units.
    pub span: Vec<u32>,
    /// Quoted spans in UTF-16 code units.
    pub spoken_spans: Vec<Vec<u32>>,
}

#[napi(object)]
pub struct NapiKnowledgeFact {
    pub kind: String,
    pub actor: String,
    pub recipient: Option<String>,
    pub topic: Option<String>,
    /// Full statement in UTF-16 code units.
    pub span: Vec<u32>,
    /// Topic or proposition in UTF-16 code units.
    pub content: Vec<u32>,
}

#[napi(object)]
pub struct NapiCharacterMention {
    pub id: String,
    pub kind: String,
    /// `[start, end)` in UTF-16 code units.
    pub span: Vec<u32>,
}

#[napi(object)]
pub struct NapiCharacterProfile {
    pub id: String,
    pub aliases: Vec<String>,
    pub gender: Option<String>,
    pub family: Vec<NapiFamilyLink>,
}

#[napi(object)]
pub struct NapiFamilyLink {
    pub role: String,
    pub relative: String,
}

#[napi(object)]
pub struct NapiFactOccurrence {
    pub candidate: NapiTripleCandidate,
    pub polarity: String,
    pub state: Option<String>,
    pub ends_state: bool,
    pub calibrated_confidence: Option<f64>,
}

#[napi(object)]
pub struct NapiEventCandidate {
    pub kind: String,
    pub actor: String,
    pub target: Option<String>,
    pub location: Option<String>,
    /// `[start, end)` in UTF-16 code units.
    pub span: Vec<u32>,
    /// Trigger words in UTF-16 code units.
    pub trigger: Vec<u32>,
}

fn to_napi_profile(person: CharacterProfile) -> NapiCharacterProfile {
    NapiCharacterProfile {
        id: person.id,
        aliases: person.aliases,
        gender: person.gender.map(|g| {
            match g {
                Gender::Feminine => "feminine",
                Gender::Masculine => "masculine",
            }
            .to_string()
        }),
        family: person
            .family
            .into_iter()
            .map(|link| NapiFamilyLink {
                role: link.role,
                relative: link.relative,
            })
            .collect(),
    }
}

fn to_napi_document(document: DocumentAnalysis, offsets: &Utf16Offsets) -> NapiDocumentAnalysis {
    NapiDocumentAnalysis {
        analysis: convert_analysis(document.analysis, offsets),
        characters: document
            .characters
            .into_iter()
            .map(to_napi_profile)
            .collect(),
        mentions: document
            .mentions
            .into_iter()
            .map(|m| NapiCharacterMention {
                id: m.id,
                kind: match m.kind {
                    MentionKind::Name => "name",
                    MentionKind::Pronoun => "pronoun",
                    MentionKind::Description => "description",
                }
                .to_string(),
                span: offsets.span(m.span),
            })
            .collect(),
        timeline: document
            .timeline
            .into_iter()
            .map(|f| to_napi_fact(f, offsets))
            .collect(),
        events: document
            .events
            .into_iter()
            .map(|e| NapiEventCandidate {
                kind: match e.kind {
                    EventKind::Duel => "duel",
                    EventKind::Proposal => "proposal",
                    EventKind::Journey => "journey",
                    EventKind::Death => "death",
                }
                .to_string(),
                actor: e.actor,
                target: e.target,
                location: e.location,
                span: offsets.span(e.span),
                trigger: offsets.span(e.trigger),
            })
            .collect(),
        knowledge: document
            .knowledge
            .into_iter()
            .map(|k| NapiKnowledgeFact {
                kind: match k.kind {
                    KnowledgeKind::Learned => "learned",
                    KnowledgeKind::Known => "known",
                    KnowledgeKind::Told => "told",
                }
                .to_string(),
                actor: k.actor,
                recipient: k.recipient,
                topic: k.topic,
                span: offsets.span(k.span),
                content: offsets.span(k.content),
            })
            .collect(),
        dialogue: NapiDialogue {
            unattributed: document.dialogue.unattributed as u32,
            lines: document
                .dialogue
                .lines
                .into_iter()
                .map(|line| NapiDialogueLine {
                    span: offsets.span(line.span),
                    spoken_spans: line
                        .spoken_spans
                        .into_iter()
                        .map(|span| offsets.span(span))
                        .collect(),
                    speaker: line.speaker,
                    listener: line.listener,
                })
                .collect(),
        },
    }
}

fn to_napi_fact(f: FactOccurrence, offsets: &Utf16Offsets) -> NapiFactOccurrence {
    NapiFactOccurrence {
        candidate: to_napi_candidate(f.candidate, offsets),
        polarity: polarity(f.polarity),
        state: f.state.map(|state| {
            match state {
                RelationshipState::Engaged => "engaged",
                RelationshipState::Married => "married",
                RelationshipState::Widowed => "widowed",
                RelationshipState::Divorced => "divorced",
                RelationshipState::Friends => "friends",
                RelationshipState::Enemies => "enemies",
                RelationshipState::Allies => "allies",
            }
            .to_string()
        }),
        ends_state: f.ends_state,
        calibrated_confidence: f.calibrated_confidence.map(f64::from),
    }
}

pub struct SeriesBinding {
    series: Series,
    offsets: BTreeMap<ChapterPosition, Utf16Offsets>,
}

#[napi(ts_return_type = "unknown")]
pub fn create_series(opts: Option<NapiOptions>) -> napi::Result<External<SeriesBinding>> {
    Ok(External::new(SeriesBinding {
        series: Series::new(to_options(opts)?),
        offsets: BTreeMap::new(),
    }))
}

#[napi]
pub fn update_series_chapter(
    #[napi(ts_arg_type = "unknown")] binding: &mut External<SeriesBinding>,
    book: f64,
    chapter: f64,
    revision: f64,
    text: String,
) -> napi::Result<bool> {
    let position = chapter_position(book, chapter)?;
    let revision = bounded_integer(revision, (1u64 << 53) - 1, "revision")?;
    if binding.series.chapter_revision(position) == Some(revision) {
        return Ok(false);
    }
    let offsets = Utf16Offsets::new(&text)?;
    let updated = binding
        .series
        .update_chapter(position, revision, &text)
        .map_err(failure)?;
    if updated {
        let document = binding
            .series
            .chapter(position)
            .expect("updated chapter is indexed");
        let offsets = offsets.for_document(document);
        binding.offsets.insert(position, offsets);
    }
    Ok(updated)
}

#[napi]
pub fn remove_series_chapter(
    #[napi(ts_arg_type = "unknown")] binding: &mut External<SeriesBinding>,
    book: f64,
    chapter: f64,
) -> napi::Result<bool> {
    let position = chapter_position(book, chapter)?;
    binding.offsets.remove(&position);
    Ok(binding.series.remove_chapter(position))
}

#[napi]
pub fn series_chapter(
    #[napi(ts_arg_type = "unknown")] binding: &External<SeriesBinding>,
    book: f64,
    chapter: f64,
) -> napi::Result<Option<NapiDocumentAnalysis>> {
    let position = chapter_position(book, chapter)?;
    Ok(binding
        .series
        .chapter(position)
        .map(|document| to_napi_document(document.clone(), &binding.offsets[&position])))
}

#[napi]
pub fn series_aggregates(
    #[napi(ts_arg_type = "unknown")] binding: &External<SeriesBinding>,
) -> Vec<NapiSeriesAggregate> {
    binding
        .series
        .aggregates()
        .into_iter()
        .map(|a| NapiSeriesAggregate {
            subject: a.subject,
            relation: a.relation,
            object: a.object,
            polarity: polarity(a.polarity),
            confidence: f64::from(a.confidence),
            rules: a.rules,
            spans: a
                .spans
                .into_iter()
                .map(|s| NapiChapterSpan {
                    book: s.position.book,
                    chapter: s.position.chapter,
                    span: binding.offsets[&s.position].span(s.span),
                })
                .collect(),
        })
        .collect()
}

#[napi]
pub fn series_relationship_history(
    #[napi(ts_arg_type = "unknown")] binding: &External<SeriesBinding>,
    person: String,
    other: String,
) -> Vec<NapiChapterFactOccurrence> {
    binding
        .series
        .relationship_history(&person, &other)
        .map(|(position, fact)| NapiChapterFactOccurrence {
            book: position.book,
            chapter: position.chapter,
            fact: to_napi_fact(fact.clone(), &binding.offsets[&position]),
        })
        .collect()
}

#[napi]
pub fn series_characters(
    #[napi(ts_arg_type = "unknown")] binding: &External<SeriesBinding>,
) -> Vec<NapiCharacterProfile> {
    binding
        .series
        .characters()
        .into_iter()
        .map(to_napi_profile)
        .collect()
}

fn chapter_position(book: f64, chapter: f64) -> napi::Result<ChapterPosition> {
    Ok(ChapterPosition {
        book: bounded_integer(book, u32::MAX.into(), "book")? as u32,
        chapter: bounded_integer(chapter, u32::MAX.into(), "chapter")? as u32,
    })
}

fn bounded_integer(value: f64, max: u64, name: &str) -> napi::Result<u64> {
    if (0.0..=max as f64).contains(&value) && value.fract() == 0.0 {
        Ok(value as u64)
    } else {
        Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("{name} must be an integer from 0 through {max}"),
        ))
    }
}

#[napi(object)]
pub struct NapiSeriesAggregate {
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub polarity: String,
    pub confidence: f64,
    pub spans: Vec<NapiChapterSpan>,
    pub rules: Vec<String>,
}

#[napi(object)]
pub struct NapiChapterSpan {
    pub book: u32,
    pub chapter: u32,
    /// `[start, end)` in the chapter's UTF-16 code units.
    pub span: Vec<u32>,
}

#[napi(object)]
pub struct NapiChapterFactOccurrence {
    pub book: u32,
    pub chapter: u32,
    pub fact: NapiFactOccurrence,
}
