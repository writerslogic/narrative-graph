mod aggregate;
mod context;
mod cooccurrence;
mod dialogue;
mod entities;
mod kinds;
pub mod pack;
mod quotes;
mod register;
mod relations;
mod segment;

use crate::types::{AggregateTriple, Analysis, Conflict, Options, Polarity, TripleCandidate};
use crate::Result;
use pack::LanguagePack;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

pub use context::Context;
pub use entities::extract_entities;
pub use relations::normalize_relation;

/// Extract candidate (subject, relation, object) triples from prose.
/// Runs the heuristic pipeline: entity detection, relation labeling, co-occurrence scoring, and confidence calculation.
/// IMPORTANT: asserted candidates only. A denied relation is a different claim
/// and this output has nowhere to say so, which is why it is dropped here and
/// kept by `extract_aggregates`, whose type carries the polarity.
pub fn extract_candidate_triples(text: &str, opts: &Options) -> Result<Vec<TripleCandidate>> {
    extract_candidate_triples_in(text, opts, &Context::default())
}

/// `extract_candidate_triples` with what a [`Context`] has learned about the
/// names in the rest of the manuscript.
pub fn extract_candidate_triples_in(
    text: &str,
    opts: &Options,
    context: &Context,
) -> Result<Vec<TripleCandidate>> {
    let mut candidates: Vec<TripleCandidate> =
        collect_candidates(text, opts, &pack::ENGLISH, context)?
            .into_iter()
            .filter(|e| e.polarity == Polarity::Asserted)
            .map(|e| e.candidate)
            .collect();
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
    extract_aggregates_in(text, opts, &Context::default())
}

/// `extract_aggregates` with what a [`Context`] has learned about the names in
/// the rest of the manuscript, and who the first person is.
///
/// Call it once per paragraph with one shared context to get the effect of
/// reading the whole book: a name's gender, and whether it is a person or a
/// place, are settled by every paragraph that mentions it, not by the one
/// being extracted.
pub fn extract_aggregates_in(
    text: &str,
    opts: &Options,
    context: &Context,
) -> Result<Vec<AggregateTriple>> {
    let candidates = collect_candidates(text, opts, &pack::ENGLISH, context)?;
    Ok(aggregate::aggregate(candidates).0)
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
    find_conflicts_in(text, opts, &Context::default())
}

/// `find_conflicts` with a shared [`Context`].
pub fn find_conflicts_in(text: &str, opts: &Options, context: &Context) -> Result<Vec<Conflict>> {
    let candidates = collect_candidates(text, opts, &pack::ENGLISH, context)?;
    let (aggregates, single_filler) = aggregate::aggregate(candidates);
    Ok(aggregate::find_conflicts(&aggregates, &single_filler))
}

/// Candidates, aggregates and conflicts from one run of the pipeline: the same
/// three results the separate functions return, for a third of the work.
pub fn analyze(text: &str, opts: &Options) -> Result<Analysis> {
    analyze_in(text, opts, &Context::default())
}

/// `analyze` with a shared [`Context`].
pub fn analyze_in(text: &str, opts: &Options, context: &Context) -> Result<Analysis> {
    let emitted = collect_candidates(text, opts, &pack::ENGLISH, context)?;
    let mut candidates: Vec<TripleCandidate> = emitted
        .iter()
        .filter(|e| e.polarity == Polarity::Asserted)
        .map(|e| e.candidate.clone())
        .collect();
    dedup_candidates(&mut candidates);
    let (aggregates, single_filler) = aggregate::aggregate(emitted);
    let conflicts = aggregate::find_conflicts(&aggregates, &single_filler);
    Ok(Analysis {
        candidates,
        aggregates,
        conflicts,
    })
}

/// One rule firing, before any collapse.
pub(crate) struct Emitted {
    candidate: TripleCandidate,
    polarity: Polarity,
    single_filler: bool,
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
    context: &Context,
) -> Result<Vec<Emitted>> {
    let min_confidence = opts.min_confidence.unwrap_or(0.0);
    if !(0.0..=1.0).contains(&min_confidence) {
        return Err(crate::NarrativeGraphError::InvalidConfidenceThreshold(
            min_confidence,
        ));
    }

    if text.is_empty() {
        return Ok(vec![]);
    }

    // Consulted per candidate, so it is built once rather than scanned linearly
    // per emission. Borrowed: a rejection set is caller-owned and not cloned.
    let rejected: BTreeSet<(&str, &str, &str)> = opts
        .rejections
        .iter()
        .map(|r| (r.subject.as_str(), r.relation.as_str(), r.object.as_str()))
        .collect();

    let mut candidates = Vec::new();
    // Referents named by the previous sentence, for pronouns with no antecedent
    // of their own. Cleared at paragraph breaks.
    let mut carried: Vec<String> = Vec::new();
    let mut previous_end = 0usize;

    // IMPORTANT: built over the whole text before any pronoun is resolved. A
    // name's evidence — a title on it, a determiner in front of it, a capital
    // no sentence boundary explains — can appear anywhere in the document, and
    // a register built as the pipeline went would miss whatever came later.
    // The caller's context seeds it, so evidence from other paragraphs counts.
    let mut register = context.register().clone();
    context::observe_text(&mut register, text, pack);

    let narrator = context.narrator();
    let mut quote_open = false;
    let mut turn = dialogue::Turn::default();
    // Hard-wrapped text breaks lines inside a paragraph and marks paragraphs
    // with a blank line; otherwise each line is a paragraph.
    let paragraph_break = if text.contains("\n\n") { 2 } else { 1 };
    let mut found = Vec::new();

    for (sent_text, sent_start) in segment::split_sentences(text, pack) {
        let breaks = text[previous_end..sent_start].matches('\n').count();
        // Hard-wrapped speech runs on across line breaks; only a paragraph
        // break ends it.
        if breaks >= paragraph_break {
            quote_open = false;
        }
        if breaks >= paragraph_break {
            found.extend(
                turn.finish(&opts.ontology, &register, pack, false)
                    .into_iter()
                    .map(|r| (r, 0)),
            );
        }
        if text[previous_end..sent_start]
            .matches('\n')
            .nth(1)
            .is_some()
        {
            carried.clear();
        }
        previous_end = sent_start + sent_text.len();

        let opened = quote_open;
        let quotes = quotes::QuoteMap::scan(sent_text, quote_open);
        quote_open = quotes.open_at_end;

        let entities = entities::extract_entities_in(
            sent_text,
            &opts.aliases,
            pack,
            &carried,
            &register,
            &quotes,
            narrator,
        );
        carried = entities::carried_referents(sent_text, &entities, pack);
        turn.observe(sent_text, sent_start, &entities, &quotes, opened, pack);

        if entities.is_empty() {
            continue;
        }

        // Look for relations between entity pairs in the same clause
        let scope = relations::Scope {
            register: &register,
            quotes: &quotes,
            mentions: &entities,
        };
        found.extend(
            relations::extract_relations(sent_text, &entities, &opts.ontology, pack, &scope)
                .into_iter()
                .map(|r| (r, sent_start)),
        );
    }
    found.extend(
        turn.finish(&opts.ontology, &register, pack, true)
            .into_iter()
            .map(|r| (r, 0)),
    );
    // A first-person document is read again with its narrator named, so that
    // "I" is a mention of someone. The caller's narrator always wins.
    if narrator.is_none() {
        if let Some(name) = turn.narrator() {
            return collect_candidates(text, opts, pack, &context.clone().with_narrator(&name));
        }
    }
    let common = dialogue::common_words(
        text,
        found
            .iter()
            .filter(|(r, _)| dialogue::is_rule(&r.rule))
            .flat_map(|(r, _)| [r.subject.as_str(), r.object.as_str()]),
    );
    found.retain(|(r, _)| {
        !dialogue::is_rule(&r.rule)
            || (dialogue::is_voice(&r.subject, 2, &turn, &register)
                && dialogue::is_voice(&r.object, 1, &turn, &register)
                && ![&r.subject, &r.object]
                    .iter()
                    .any(|n| common.contains(*n) || pack.sentence_openers.contains(&n.as_str())))
    });

    // Each span is relative to its offset: the sentence's start, or 0 for a
    // paragraph-level candidate whose span is already absolute.
    for (rel, offset) in found {
        let confidence = cooccurrence::score_confidence(rel.base, rel.gap);

        let key = (
            rel.subject.as_str(),
            rel.relation.as_str(),
            rel.object.as_str(),
        );
        if confidence >= min_confidence && !rejected.contains(&key) {
            let span = [offset + rel.span[0], offset + rel.span[1]];
            // A rejection names a triple, not a stance: a caller rejecting
            // a wrong claim means the claim, whichever way the text put it.
            let polarity = match rel.stance {
                relations::Stance::Asserted => Polarity::Asserted,
                relations::Stance::Denied => Polarity::Denied,
                relations::Stance::Suspended => continue,
            };

            candidates.push(Emitted {
                candidate: TripleCandidate {
                    subject: rel.subject,
                    relation: rel.relation,
                    object: rel.object,
                    confidence,
                    span,
                    rule: rel.rule,
                },
                polarity,
                single_filler: rel.single_filler,
            });
        }
    }

    Ok(candidates)
}

fn dedup_candidates(candidates: &mut Vec<TripleCandidate>) {
    struct Best {
        confidence: f32,
        span: [usize; 2],
        rule: String,
    }

    let mut best: BTreeMap<(String, String, String), Best> = BTreeMap::new();

    for candidate in candidates.drain(..) {
        let TripleCandidate {
            subject,
            relation,
            object,
            confidence,
            span,
            rule,
        } = candidate;
        let evidence = Best {
            confidence,
            span,
            rule,
        };
        // Strict `>`: the first candidate wins a tie.
        match best.entry((subject, relation, object)) {
            Entry::Vacant(entry) => {
                entry.insert(evidence);
            }
            Entry::Occupied(mut entry) => {
                if evidence.confidence > entry.get().confidence {
                    entry.insert(evidence);
                }
            }
        }
    }

    candidates.extend(
        best.into_iter()
            .map(|((subject, relation, object), b)| TripleCandidate {
                subject,
                relation,
                object,
                confidence: b.confidence,
                span: b.span,
                rule: b.rule,
            }),
    );
}
