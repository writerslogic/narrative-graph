use napi_derive::napi;

use crate::heuristic::extract_candidate_triples;
use crate::types::{Options, Rejection};

#[napi]
pub fn extract_candidate_triples_napi(
    text: String,
    opts: Option<NapiOptions>,
) -> napi::Result<Vec<NapiTripleCandidate>> {
    let options = if let Some(napi_opts) = opts {
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
    };

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
