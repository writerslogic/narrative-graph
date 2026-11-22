//! Node bindings.
//!
//! IMPORTANT: every span crossing this boundary is in UTF-16 code units, the
//! unit `String.prototype.slice` takes. The Rust API reports UTF-8 byte
//! offsets; the two agree only on ASCII text.

use napi::bindgen_prelude::AsyncTask;
use napi::{Env, Task};
use napi_derive::napi;

use crate::heuristic::{analyze, extract_aggregates, extract_candidate_triples, find_conflicts};
use crate::types::{
    AggregateTriple, Analysis, Conflict, ConflictKind, Options, Polarity, Rejection,
    TripleCandidate,
};

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
            })
            .collect()
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
    })
}

fn to_napi_analysis(analysis: Analysis, text: &str) -> napi::Result<NapiAnalysis> {
    let offsets = Utf16Offsets::new(text)?;
    Ok(NapiAnalysis {
        candidates: analysis
            .candidates
            .into_iter()
            .map(|c| to_napi_candidate(c, &offsets))
            .collect(),
        aggregates: analysis
            .aggregates
            .into_iter()
            .map(|a| to_napi_aggregate(a, &offsets))
            .collect(),
        conflicts: analysis
            .conflicts
            .into_iter()
            .map(|c| to_napi_conflict(c, &offsets))
            .collect(),
    })
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
        polarity: match a.polarity {
            Polarity::Asserted => "asserted".to_string(),
            Polarity::Denied => "denied".to_string(),
        },
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
}

#[napi(object)]
pub struct NapiRejection {
    pub subject: String,
    pub relation: String,
    pub object: String,
}
