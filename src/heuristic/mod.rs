mod aggregate;
mod cooccurrence;
mod entities;
pub mod pack;
mod register;
mod relations;
mod segment;

use crate::types::{AggregateTriple, Conflict, Options, Polarity, TripleCandidate};
use crate::Result;
use pack::LanguagePack;
use std::collections::{BTreeMap, BTreeSet};

pub use entities::extract_entities;
pub use relations::normalize_relation;

/// Extract candidate (subject, relation, object) triples from prose.
/// Runs the heuristic pipeline: entity detection, relation labeling, co-occurrence scoring, and confidence calculation.
/// IMPORTANT: asserted candidates only. A denied relation is a different claim
/// and this output has nowhere to say so, which is why it is dropped here and
/// kept by `extract_aggregates`, whose type carries the polarity.
pub fn extract_candidate_triples(text: &str, opts: &Options) -> Result<Vec<TripleCandidate>> {
    let mut candidates: Vec<TripleCandidate> = collect_candidates(text, opts, &pack::ENGLISH)?
        .into_iter()
        .filter(|(_, polarity)| *polarity == Polarity::Asserted)
        .map(|(candidate, _)| candidate)
        .collect();
    // Deduplicate: keep highest confidence for each (subj, rel, obj) triple
    dedup_candidates(&mut candidates);
    Ok(candidates)
}

/// Extract one entry per fact the document states, carrying every span that
/// states it and ordered by where the first of them appears.
///
/// The same pipeline and the same rules as `extract_candidate_triples`; only
/// the collapse at the end differs. Use this to ask what a manuscript claims
/// overall, and that one to ask what each sentence claims.
pub fn extract_aggregates(text: &str, opts: &Options) -> Result<Vec<AggregateTriple>> {
    let candidates = collect_candidates(text, opts, &pack::ENGLISH)?;
    Ok(aggregate::aggregate(candidates))
}

/// Find the claims in a passage that cannot both be true.
///
/// A fact asserted and the same fact denied, or a relation that admits one
/// subject per object asserted of two subjects. Both sides of every conflict
/// carry their spans, because the crate has no way to decide which one the
/// manuscript meant and no business guessing.
///
/// IMPORTANT: a relation that simply changes over the story is not a conflict.
/// `enemy_of` early and `ally_of` late is a character arc, and
/// `extract_aggregates` already puts the two in the order the story states
/// them.
pub fn find_conflicts(text: &str, opts: &Options) -> Result<Vec<Conflict>> {
    let aggregates = extract_aggregates(text, opts)?;
    Ok(aggregate::find_conflicts(&aggregates, &pack::ENGLISH))
}

/// Every candidate the rules fire on, in document order and with repeats
/// intact. IMPORTANT: this is the one place the evidence still exists in full.
/// `dedup_candidates` keeps one candidate per triple and drops the spans of the
/// rest, so anything that needs them has to read this list, not that output.
///
/// Reading the whole vocabulary from one pack; selecting a pack is a
/// caller-facing decision that waits on a second pack existing, so this stays
/// internal and the public entry points supply English.
fn collect_candidates(
    text: &str,
    opts: &Options,
    pack: &LanguagePack,
) -> Result<Vec<(TripleCandidate, Polarity)>> {
    if text.is_empty() {
        return Ok(vec![]);
    }

    let min_confidence = opts.min_confidence.unwrap_or(0.0);
    if !(0.0..=1.0).contains(&min_confidence) {
        return Err(crate::NarrativeGraphError::InvalidConfidenceThreshold(
            min_confidence,
        ));
    }

    // Consulted per candidate, so it is built once rather than scanned linearly
    // per emission. Borrowed: a rejection set is caller-owned and not cloned.
    let rejected: BTreeSet<(&str, &str, &str)> = opts
        .rejections
        .iter()
        .map(|r| (r.subject.as_str(), r.relation.as_str(), r.object.as_str()))
        .collect();

    let mut candidates = Vec::new();
    // The referents the previous sentence named, for a pronoun in this one
    // that has no antecedent of its own. Cleared at a paragraph break: a
    // pronoun chain does not cross one, and a paragraph is the coarsest
    // boundary the pipeline can actually see — it has no notion of a scene
    // break or a POV change, so this is the closest stand-in for both.
    let mut carried: Vec<String> = Vec::new();
    let mut previous_end = 0usize;

    // IMPORTANT: built over the whole text before any pronoun is resolved. A
    // name's evidence — a title on it, a determiner in front of it, a capital
    // no sentence boundary explains — can appear anywhere in the document, and
    // a register built as the pipeline went would miss whatever came later.
    let mut register = register::Register::default();
    for (sent_text, _) in segment::split_sentences(text, pack) {
        let mentions = entities::mentions_for_register(sent_text, pack);
        let pronouns = entities::pronouns_for_register(sent_text, pack);
        register.observe(sent_text, &mentions, &pronouns, pack);
    }

    for (sent_text, sent_start) in segment::split_sentences(text, pack) {
        if text[previous_end..sent_start].matches('\n').count() > 1 {
            carried.clear();
        }
        previous_end = sent_start + sent_text.len();

        let entities =
            entities::extract_entities_with(sent_text, &opts.aliases, pack, &carried, &register);
        carried = entities::carried_referents(sent_text, &entities, pack);

        if entities.is_empty() {
            continue;
        }

        // Look for relations between entity pairs in the same clause
        let relations = relations::extract_relations(sent_text, &entities, &opts.ontology, pack);

        // Score by rule strength and entity proximity within the sentence
        for rel in relations {
            let confidence = cooccurrence::score_confidence(rel.base, rel.gap);

            let key = (
                rel.subject.as_str(),
                rel.relation.as_str(),
                rel.object.as_str(),
            );
            if confidence >= min_confidence && !rejected.contains(&key) {
                let span = [sent_start + rel.span[0], sent_start + rel.span[1]];
                // A rejection names a triple, not a stance: a caller rejecting
                // a wrong claim means the claim, whichever way the text put it.
                let polarity = match rel.stance {
                    relations::Stance::Denied => Polarity::Denied,
                    // `extract_relations` emits nothing for a suspended window.
                    _ => Polarity::Asserted,
                };

                candidates.push((
                    TripleCandidate {
                        subject: rel.subject,
                        relation: rel.relation,
                        object: rel.object,
                        confidence,
                        span,
                        rule: rel.rule,
                    },
                    polarity,
                ));
            }
        }
    }

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
