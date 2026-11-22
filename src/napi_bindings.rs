use napi_derive::napi;

use crate::heuristic::{extract_aggregates, extract_candidate_triples, find_conflicts};
use crate::types::{AggregateTriple, ConflictKind, Options, Polarity, Rejection};

#[napi]
pub fn extract_candidate_triples_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<Vec<NapiTripleCandidate>> {
    let options = to_options(opts)?;

    extract_candidate_triples(&text, &options)
        .map_err(failure)?
        .into_iter()
        .map(|c| {
            Ok(NapiTripleCandidate {
                span: to_napi_span(c.span)?,
                subject: c.subject,
                relation: c.relation,
                object: c.object,
                confidence: c.confidence as f64,
                rule: c.rule,
            })
        })
        .collect()
}

fn failure(e: impl std::fmt::Display) -> napi::Error {
    napi::Error::new(napi::Status::GenericFailure, e.to_string())
}

/// Byte offsets into the UTF-8 text, narrowed to the `u32` the Node shape uses.
fn to_napi_span(span: [usize; 2]) -> napi::Result<Vec<u32>> {
    span.iter()
        .map(|&offset| {
            u32::try_from(offset).map_err(|_| {
                napi::Error::new(
                    napi::Status::GenericFailure,
                    format!("span offset {offset} exceeds u32"),
                )
            })
        })
        .collect()
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

/// One fact as the whole passage states it, with every supporting span.
/// Ordered by first span, so two facts about the same pair can be read in the
/// order the story states them.
#[napi]
pub fn extract_aggregates_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<Vec<NapiAggregateTriple>> {
    let options = to_options(opts)?;

    extract_aggregates(&text, &options)
        .map_err(failure)?
        .into_iter()
        .map(to_napi_aggregate)
        .collect()
}

#[napi(object)]
pub struct NapiAggregateTriple {
    pub subject: String,
    pub relation: String,
    pub object: String,
    /// "asserted" or "denied".
    pub polarity: String,
    pub confidence: f64,
    pub spans: Vec<Vec<u32>>,
    pub rules: Vec<String>,
}

/// Claims the passage makes that cannot both be true: a fact and its denial,
/// or two subjects where the relation admits one. Both sides carry their spans.
#[napi]
pub fn find_conflicts_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<Vec<NapiConflict>> {
    let options = to_options(opts)?;

    find_conflicts(&text, &options)
        .map_err(failure)?
        .into_iter()
        .map(|c| {
            Ok(NapiConflict {
                kind: match c.kind {
                    ConflictKind::Denial => "denial".to_string(),
                    ConflictKind::Cardinality => "cardinality".to_string(),
                },
                left: to_napi_aggregate(c.left)?,
                right: to_napi_aggregate(c.right)?,
            })
        })
        .collect()
}

#[napi(object)]
pub struct NapiConflict {
    /// "denial" or "cardinality".
    pub kind: String,
    pub left: NapiAggregateTriple,
    pub right: NapiAggregateTriple,
}

fn to_napi_aggregate(a: AggregateTriple) -> napi::Result<NapiAggregateTriple> {
    Ok(NapiAggregateTriple {
        subject: a.subject,
        relation: a.relation,
        object: a.object,
        polarity: match a.polarity {
            Polarity::Asserted => "asserted".to_string(),
            Polarity::Denied => "denied".to_string(),
        },
        confidence: a.confidence as f64,
        spans: a
            .spans
            .into_iter()
            .map(to_napi_span)
            .collect::<napi::Result<_>>()?,
        rules: a.rules,
    })
}

#[napi(object)]
pub struct NapiTripleCandidate {
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub confidence: f64,
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
