use napi_derive::napi;

use crate::heuristic::{extract_aggregates, extract_candidate_triples, find_conflicts};
use crate::types::{AggregateTriple, ConflictKind, Options, Polarity, Rejection};

#[napi]
pub fn extract_candidate_triples_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<Vec<NapiTripleCandidate>> {
    let options = to_options(opts);

    match extract_candidate_triples(&text, &options) {
        Ok(candidates) => Ok(candidates
            .into_iter()
            .map(|c| NapiTripleCandidate {
                subject: c.subject,
                relation: c.relation,
                object: c.object,
                confidence: c.confidence as f64,
                span: c.span.iter().map(|&s| s as u32).collect(),
                rule: c.rule,
            })
            .collect()),
        Err(e) => Err(napi::Error::new(
            napi::Status::GenericFailure,
            e.to_string(),
        )),
    }
}

/// The one place the hand-maintained Node option shape is mapped onto
/// `Options`. IMPORTANT: it is hand-maintained, so a field added to `Options`
/// and not added here compiles clean and silently never reaches the pipeline.
fn to_options(opts: Option<NapiOptions>) -> Options {
    if let Some(napi_opts) = opts {
        Options {
            aliases: napi_opts.aliases.unwrap_or_default().into_iter().collect(),
            min_confidence: napi_opts.min_confidence.map(|c| c as f32),
            cross_sentence_pronouns: napi_opts.cross_sentence_pronouns.unwrap_or(false),
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
        }
    } else {
        Options::default()
    }
}

/// One fact as the whole passage states it, with every supporting span.
/// Ordered by first span, so two facts about the same pair can be read in the
/// order the story states them.
#[napi]
pub fn extract_aggregates_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<Vec<NapiAggregateTriple>> {
    let options = to_options(opts);

    match extract_aggregates(&text, &options) {
        Ok(aggregates) => Ok(aggregates.into_iter().map(to_napi_aggregate).collect()),
        Err(e) => Err(napi::Error::new(
            napi::Status::GenericFailure,
            e.to_string(),
        )),
    }
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
    let options = to_options(opts);

    match find_conflicts(&text, &options) {
        Ok(conflicts) => Ok(conflicts
            .into_iter()
            .map(|c| NapiConflict {
                kind: match c.kind {
                    ConflictKind::Denial => "denial".to_string(),
                    ConflictKind::Cardinality => "cardinality".to_string(),
                },
                left: to_napi_aggregate(c.left),
                right: to_napi_aggregate(c.right),
            })
            .collect()),
        Err(e) => Err(napi::Error::new(
            napi::Status::GenericFailure,
            e.to_string(),
        )),
    }
}

#[napi(object)]
pub struct NapiConflict {
    /// "denial" or "cardinality".
    pub kind: String,
    pub left: NapiAggregateTriple,
    pub right: NapiAggregateTriple,
}

fn to_napi_aggregate(a: AggregateTriple) -> NapiAggregateTriple {
    NapiAggregateTriple {
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
            .iter()
            .map(|span| span.iter().map(|&s| s as u32).collect())
            .collect(),
        rules: a.rules,
    }
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
    pub cross_sentence_pronouns: Option<bool>,
    pub rejections: Option<Vec<NapiRejection>>,
    pub ontology: Option<std::collections::HashMap<String, String>>,
}

#[napi(object)]
pub struct NapiRejection {
    pub subject: String,
    pub relation: String,
    pub object: String,
}
