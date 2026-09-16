use napi_derive::napi;

use crate::heuristic::{extract_aggregates, extract_candidate_triples};
use crate::types::{Options, Rejection};

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
        Ok(aggregates) => Ok(aggregates
            .into_iter()
            .map(|a| NapiAggregateTriple {
                subject: a.subject,
                relation: a.relation,
                object: a.object,
                confidence: a.confidence as f64,
                spans: a
                    .spans
                    .iter()
                    .map(|span| span.iter().map(|&s| s as u32).collect())
                    .collect(),
                rules: a.rules,
            })
            .collect()),
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
    pub confidence: f64,
    pub spans: Vec<Vec<u32>>,
    pub rules: Vec<String>,
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
