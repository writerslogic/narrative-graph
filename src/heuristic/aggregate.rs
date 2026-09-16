//! Document-level aggregation: one entry per fact, carrying every span that
//! stated it.
//!
//! The per-sentence pipeline is unchanged and still the default. This is the
//! other reading of the same candidates: `extract_candidate_triples` answers
//! "what does each sentence claim", and `extract_aggregates` answers "what does
//! the document claim, and where does it say so".

use crate::types::{AggregateTriple, TripleCandidate};
use std::collections::BTreeMap;

/// Merge candidates that restate the same fact, keeping all their evidence.
///
/// IMPORTANT: order is by first supporting span, not alphabetical. That is what
/// makes an aggregate orderable against another one over the same pair, which
/// is the cheapest useful answer to "when does this hold": a relation asserted
/// in chapter 2 and its opposite in chapter 20 are a character arc, and the
/// document position is the only "when" the pipeline already knows. Ties break
/// on the triple, so the order is total and reproducible.
///
/// `extract_candidate_triples` keeps its own alphabetical order, which callers
/// and the documented example output depend on; nothing here touches it.
pub fn aggregate(candidates: Vec<TripleCandidate>) -> Vec<AggregateTriple> {
    let mut merged: BTreeMap<(String, String, String), AggregateTriple> = BTreeMap::new();

    for candidate in candidates {
        let key = (
            candidate.subject.clone(),
            candidate.relation.clone(),
            candidate.object.clone(),
        );
        match merged.get_mut(&key) {
            Some(entry) => {
                entry.confidence = entry.confidence.max(candidate.confidence);
                entry.spans.push(candidate.span);
                if !entry.rules.contains(&candidate.rule) {
                    entry.rules.push(candidate.rule);
                }
            }
            None => {
                merged.insert(
                    key,
                    AggregateTriple {
                        subject: candidate.subject,
                        relation: candidate.relation,
                        object: candidate.object,
                        confidence: candidate.confidence,
                        spans: vec![candidate.span],
                        rules: vec![candidate.rule],
                    },
                );
            }
        }
    }

    let mut aggregates: Vec<AggregateTriple> = merged.into_values().collect();
    // Candidates arrive in document order, so each `spans` vector is already
    // ascending; only the aggregates themselves need ordering.
    aggregates.sort_by(|a, b| {
        a.spans[0].cmp(&b.spans[0]).then_with(|| {
            (&a.subject, &a.relation, &a.object).cmp(&(&b.subject, &b.relation, &b.object))
        })
    });
    aggregates
}
