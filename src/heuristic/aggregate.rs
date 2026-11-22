//! Document-level aggregation: one entry per fact, carrying every span that
//! stated it.
//!
//! The per-sentence pipeline is unchanged and still the default. This is the
//! other reading of the same candidates: `extract_candidate_triples` answers
//! "what does each sentence claim", and `extract_aggregates` answers "what does
//! the document claim, and where does it say so".

use super::Emitted;
use crate::types::{AggregateTriple, Conflict, ConflictKind, Polarity, TripleCandidate};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap};

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
///
/// The second vector says, per aggregate, whether every candidate behind it came
/// from a single-filler relation, which `find_conflicts` needs and the public
/// type does not carry.
pub fn aggregate(candidates: Vec<Emitted>) -> (Vec<AggregateTriple>, Vec<bool>) {
    struct Evidence {
        confidence: f32,
        spans: Vec<[usize; 2]>,
        rules: Vec<String>,
        single_filler: bool,
    }

    let mut merged: BTreeMap<(String, String, String, Polarity), Evidence> = BTreeMap::new();

    for Emitted {
        candidate,
        polarity,
        single_filler,
    } in candidates
    {
        let TripleCandidate {
            subject,
            relation,
            object,
            confidence,
            span,
            rule,
        } = candidate;
        match merged.entry((subject, relation, object, polarity)) {
            Entry::Occupied(mut entry) => {
                let entry = entry.get_mut();
                entry.confidence = entry.confidence.max(confidence);
                entry.spans.push(span);
                entry.single_filler &= single_filler;
                if !entry.rules.contains(&rule) {
                    entry.rules.push(rule);
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(Evidence {
                    confidence,
                    spans: vec![span],
                    rules: vec![rule],
                    single_filler,
                });
            }
        }
    }

    let mut aggregates: Vec<(AggregateTriple, bool)> = merged
        .into_iter()
        .map(|((subject, relation, object, polarity), evidence)| {
            (
                AggregateTriple {
                    subject,
                    relation,
                    object,
                    polarity,
                    confidence: evidence.confidence,
                    spans: evidence.spans,
                    rules: evidence.rules,
                },
                evidence.single_filler,
            )
        })
        .collect();
    // Candidates arrive in document order, so each `spans` vector is already
    // ascending; only the aggregates themselves need ordering.
    sort_by_first_span(&mut aggregates);
    aggregates.into_iter().unzip()
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
///
/// IMPORTANT: the cardinality check reads `single_filler`, set from the rule's
/// relation before the ontology renamed it. Renaming `mother_of` keeps the
/// constraint, and mapping another relation onto the same label does not
/// acquire it; an aggregate merged from both is unconstrained.
pub fn find_conflicts(aggregates: &[AggregateTriple], single_filler: &[bool]) -> Vec<Conflict> {
    debug_assert_eq!(aggregates.len(), single_filler.len());
    // Only aggregates sharing these keys can conflict, so grouping replaces a
    // scan of every pair. Indices are pushed in ascending order.
    let mut by_fact: HashMap<(&str, &str, &str), Vec<usize>> = HashMap::new();
    let mut by_slot: HashMap<(&str, &str), Vec<usize>> = HashMap::new();
    for (i, a) in aggregates.iter().enumerate() {
        by_fact
            .entry((&a.subject, &a.relation, &a.object))
            .or_default()
            .push(i);
        if a.polarity == Polarity::Asserted && single_filler[i] {
            by_slot.entry((&a.relation, &a.object)).or_default().push(i);
        }
    }

    let mut pairs: Vec<(usize, usize, ConflictKind)> = Vec::new();
    for group in by_fact.values() {
        for (n, &i) in group.iter().enumerate() {
            for &j in &group[n + 1..] {
                if aggregates[i].polarity != aggregates[j].polarity {
                    pairs.push((i, j, ConflictKind::Denial));
                }
            }
        }
    }
    // Aggregates are unique per (triple, polarity), so every asserted pair in a
    // slot names two different subjects and never also forms a denial.
    for group in by_slot.values() {
        for (n, &i) in group.iter().enumerate() {
            for &j in &group[n + 1..] {
                pairs.push((i, j, ConflictKind::Cardinality));
            }
        }
    }
    pairs.sort_unstable_by_key(|&(i, j, _)| (i, j));

    pairs
        .into_iter()
        .map(|(i, j, kind)| Conflict {
            kind,
            left: aggregates[i].clone(),
            right: aggregates[j].clone(),
        })
        .collect()
}

/// Order by where the document first states each claim, breaking ties on the
/// triple so the order is total and reproducible.
fn sort_by_first_span(aggregates: &mut [(AggregateTriple, bool)]) {
    aggregates.sort_by(|(a, _), (b, _)| {
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
