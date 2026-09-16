//! Document-level aggregation: one entry per fact, carrying every span that
//! stated it.
//!
//! The per-sentence pipeline is unchanged and still the default. This is the
//! other reading of the same candidates: `extract_candidate_triples` answers
//! "what does each sentence claim", and `extract_aggregates` answers "what does
//! the document claim, and where does it say so".

use super::pack::LanguagePack;
use crate::types::{AggregateTriple, Conflict, ConflictKind, Polarity, TripleCandidate};
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
pub fn aggregate(candidates: Vec<(TripleCandidate, Polarity)>) -> Vec<AggregateTriple> {
    let mut merged: BTreeMap<(String, String, String, Polarity), AggregateTriple> = BTreeMap::new();

    for (candidate, polarity) in candidates {
        let key = (
            candidate.subject.clone(),
            candidate.relation.clone(),
            candidate.object.clone(),
            polarity,
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
                        polarity,
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
    sort_by_first_span(&mut aggregates);
    aggregates
}

/// Claims the passage makes that cannot both be true.
///
/// Two checks, and no more, because each has to be answerable from what the
/// pipeline actually extracted:
///
/// - A fact asserted and the same fact denied. This is what the denial machinery
///   exists for: the assertion gate used to drop a negated relation, so "Elena
///   is not Marco's sister" was indistinguishable from a sentence mentioning
///   neither of them.
/// - A relation whose object takes one subject, asserted of two subjects. A
///   person has one mother; two of them is an error in the manuscript, not a
///   character arc.
///
/// IMPORTANT: relations that legitimately change over a story are *not*
/// conflicts. `enemy_of` in chapter 2 and `ally_of` in chapter 20 is the thing
/// a narrative does, and reporting it would bury the real continuity errors
/// under every character arc in the book. Ordering those two is what
/// `extract_aggregates` already gives a caller.
pub fn find_conflicts(aggregates: &[AggregateTriple], pack: &LanguagePack) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    for (i, left) in aggregates.iter().enumerate() {
        for right in &aggregates[i + 1..] {
            let kind = if left.subject == right.subject
                && left.relation == right.relation
                && left.object == right.object
                && left.polarity != right.polarity
            {
                ConflictKind::Denial
            } else if left.polarity == Polarity::Asserted
                && right.polarity == Polarity::Asserted
                && left.relation == right.relation
                && left.object == right.object
                && left.subject != right.subject
                && pack
                    .single_filler_relations
                    .contains(&left.relation.as_str())
            {
                ConflictKind::Cardinality
            } else {
                continue;
            };

            conflicts.push(Conflict {
                kind,
                left: left.clone(),
                right: right.clone(),
            });
        }
    }

    conflicts
}

/// Order by where the document first states each claim, breaking ties on the
/// triple so the order is total and reproducible.
fn sort_by_first_span(aggregates: &mut [AggregateTriple]) {
    aggregates.sort_by(|a, b| {
        a.spans[0].cmp(&b.spans[0]).then_with(|| {
            (&a.subject, &a.relation, &a.object, a.polarity).cmp(&(
                &b.subject,
                &b.relation,
                &b.object,
                b.polarity,
            ))
        })
    });
}
