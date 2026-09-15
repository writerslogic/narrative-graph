mod cooccurrence;
mod entities;
mod relations;

use crate::types::{Options, TripleCandidate};
use crate::Result;
use std::collections::BTreeMap;

pub use entities::extract_entities;
pub use relations::normalize_relation;

/// Extract candidate (subject, relation, object) triples from prose.
/// Runs the heuristic pipeline: entity detection, relation labeling, co-occurrence scoring, and confidence calculation.
pub fn extract_candidate_triples(text: &str, opts: &Options) -> Result<Vec<TripleCandidate>> {
    if text.is_empty() {
        return Ok(vec![]);
    }

    let min_confidence = opts.min_confidence.unwrap_or(0.0);
    if min_confidence < 0.0 || min_confidence > 1.0 {
        return Err(crate::NarrativeGraphError::InvalidConfidenceThreshold(min_confidence));
    }

    // Split into sentences for processing
    let sentences = split_sentences(text);
    let mut candidates = Vec::new();

    for (_sentence_idx, (sent_text, sent_start)) in sentences.iter().enumerate() {
        // Extract entities in this sentence
        let entities = extract_entities(sent_text, &opts.aliases);

        if entities.is_empty() {
            continue;
        }

        // Look for relations between entity pairs in the same clause
        let relations = relations::extract_relations(sent_text, &entities, &opts.ontology);

        // Score by co-occurrence within sentence window and pattern strength
        for (subj_ent, rel, obj_ent, rule) in relations {
            let confidence = cooccurrence::score_confidence(&subj_ent, &rel, &obj_ent, sent_text.as_str());

            if confidence >= min_confidence {
                // Find the span of the triple in the original text
                let span_in_sentence = find_span_in_text(sent_text, &subj_ent, &obj_ent);
                let span = [
                    sent_start + span_in_sentence[0],
                    sent_start + span_in_sentence[1],
                ];

                candidates.push(TripleCandidate {
                    subject: subj_ent,
                    relation: rel,
                    object: obj_ent,
                    confidence,
                    span,
                    rule,
                });
            }
        }
    }

    // Deduplicate: keep highest confidence for each (subj, rel, obj) triple
    dedup_candidates(&mut candidates);

    Ok(candidates)
}

fn split_sentences(text: &str) -> Vec<(String, usize)> {
    let mut sentences = Vec::new();
    let mut current_sent = String::new();
    let mut start_idx = 0;
    let mut byte_pos = 0;

    for c in text.chars() {
        current_sent.push(c);

        // Sentence boundaries: . ! ? followed by whitespace or end of text
        if matches!(c, '.' | '!' | '?') {
            let rest = &text[byte_pos + c.len_utf8()..];
            if rest.is_empty() || rest.chars().next().map(|ch| ch.is_whitespace()).unwrap_or(false) {
                let sent = current_sent.trim().to_string();
                if !sent.is_empty() {
                    sentences.push((sent, start_idx));
                }
                current_sent.clear();
                start_idx = byte_pos + c.len_utf8() + rest.chars().next().map(|ch| ch.len_utf8()).unwrap_or(0);
            }
        }

        byte_pos += c.len_utf8();
    }

    if !current_sent.trim().is_empty() {
        sentences.push((current_sent.trim().to_string(), start_idx));
    }

    sentences
}

fn find_span_in_text(text: &str, subj: &str, obj: &str) -> [usize; 2] {
    let text_lower = text.to_lowercase();
    let subj_lower = subj.to_lowercase();
    let obj_lower = obj.to_lowercase();

    if let Some(subj_pos) = text_lower.find(&subj_lower) {
        if let Some(obj_pos) = text_lower.find(&obj_lower) {
            let start = subj_pos.min(obj_pos);
            let end = (subj_pos + subj_lower.len()).max(obj_pos + obj_lower.len());
            return [start, end];
        }
    }

    [0, text.len()]
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
