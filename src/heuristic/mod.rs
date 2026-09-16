mod cooccurrence;
mod entities;
pub mod pack;
mod relations;
mod segment;

use crate::types::{Options, TripleCandidate};
use crate::Result;
use pack::LanguagePack;
use std::collections::BTreeMap;

pub use entities::extract_entities;
pub use relations::normalize_relation;

/// Extract candidate (subject, relation, object) triples from prose.
/// Runs the heuristic pipeline: entity detection, relation labeling, co-occurrence scoring, and confidence calculation.
pub fn extract_candidate_triples(text: &str, opts: &Options) -> Result<Vec<TripleCandidate>> {
    extract_with_pack(text, opts, &pack::ENGLISH)
}

/// The pipeline proper, reading its whole vocabulary from one pack. Selecting a
/// pack is a caller-facing decision that waits on a second pack existing, so
/// this stays internal and `extract_candidate_triples` supplies English.
fn extract_with_pack(
    text: &str,
    opts: &Options,
    pack: &LanguagePack,
) -> Result<Vec<TripleCandidate>> {
    if text.is_empty() {
        return Ok(vec![]);
    }

    let min_confidence = opts.min_confidence.unwrap_or(0.0);
    if !(0.0..=1.0).contains(&min_confidence) {
        return Err(crate::NarrativeGraphError::InvalidConfidenceThreshold(
            min_confidence,
        ));
    }

    let mut candidates = Vec::new();

    for (sent_text, sent_start) in segment::split_sentences(text, pack) {
        let entities = entities::extract_entities_with(sent_text, &opts.aliases, pack);

        if entities.is_empty() {
            continue;
        }

        // Look for relations between entity pairs in the same clause
        let relations = relations::extract_relations(sent_text, &entities, &opts.ontology, pack);

        // Score by rule strength and entity proximity within the sentence
        for rel in relations {
            let confidence = cooccurrence::score_confidence(rel.base, rel.gap);

            if confidence >= min_confidence {
                let span = [sent_start + rel.span[0], sent_start + rel.span[1]];

                candidates.push(TripleCandidate {
                    subject: rel.subject,
                    relation: rel.relation,
                    object: rel.object,
                    confidence,
                    span,
                    rule: rel.rule,
                });
            }
        }
    }

    // Deduplicate: keep highest confidence for each (subj, rel, obj) triple
    dedup_candidates(&mut candidates);

    Ok(candidates)
}

fn dedup_candidates(candidates: &mut Vec<TripleCandidate>) {
    let mut best: BTreeMap<(String, String, String), TripleCandidate> = BTreeMap::new();

    for candidate in candidates.drain(..) {
        let key = (
            candidate.subject.clone(),
            candidate.relation.clone(),
            candidate.object.clone(),
        );
        best.entry(key)
            .and_modify(|best_cand| {
                if candidate.confidence > best_cand.confidence {
                    *best_cand = candidate.clone();
                }
            })
            .or_insert(candidate);
    }

    *candidates = best.into_values().collect();
}
