mod aggregate;
mod chinese;
mod context;
mod cooccurrence;
mod coreference;
mod dialogue;
mod entities;
mod events;
mod kinds;
mod knowledge;
#[cfg(feature = "model")]
pub mod model;
pub mod pack;
mod quotes;
mod register;
mod relations;
mod segment;

use crate::types::{
    AggregateTriple, Analysis, CharacterMention, CharacterProfile, Conflict, Dialogue,
    DocumentAnalysis, EventCandidate, FactOccurrence, KnowledgeFact, Language, MentionKind,
    Options, Polarity, RelationshipState, TripleCandidate,
};
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
    let mut candidates: Vec<TripleCandidate> = collect_candidates(text, opts, context, None)?
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
    let candidates = collect_candidates(text, opts, context, None)?;
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
    let candidates = collect_candidates(text, opts, context, None)?;
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
    Ok(analysis_from_candidates(collect_candidates(
        text, opts, context, None,
    )?))
}

/// Analyze relations and return the character profiles used to resolve them.
pub fn analyze_document(text: &str, opts: &Options) -> Result<DocumentAnalysis> {
    analyze_document_in(text, opts, &Context::default())
}

/// `analyze_document` with a shared [`Context`].
pub fn analyze_document_in(
    text: &str,
    opts: &Options,
    context: &Context,
) -> Result<DocumentAnalysis> {
    let mut evidence = DocumentEvidence::default();
    let emitted = collect_candidates(text, opts, context, Some(&mut evidence))?;
    let mut timeline: Vec<_> = emitted
        .iter()
        .map(|e| FactOccurrence {
            candidate: e.candidate.clone(),
            polarity: e.polarity,
            state: e.state,
            ends_state: e.ends_state,
            calibrated_confidence: crate::rule_calibration(&e.candidate.rule)
                .map(|calibration| calibration.estimate(e.candidate.confidence)),
        })
        .collect();
    timeline.sort_by_key(|fact| fact.candidate.span);
    evidence.mentions.sort_by_key(|m| m.span);
    evidence.mentions.dedup();
    Ok(DocumentAnalysis {
        analysis: analysis_from_candidates(emitted),
        characters: evidence.characters,
        mentions: evidence.mentions,
        timeline,
        events: evidence.events,
        knowledge: evidence.knowledge,
        dialogue: evidence.dialogue,
    })
}

fn analysis_from_candidates(emitted: Vec<Emitted>) -> Analysis {
    let mut candidates: Vec<TripleCandidate> = emitted
        .iter()
        .filter(|e| e.polarity == Polarity::Asserted)
        .map(|e| e.candidate.clone())
        .collect();
    dedup_candidates(&mut candidates);
    let (aggregates, single_filler) = aggregate::aggregate(emitted);
    let conflicts = aggregate::find_conflicts(&aggregates, &single_filler);
    Analysis {
        candidates,
        aggregates,
        conflicts,
    }
}

/// One rule firing, before any collapse.
pub(crate) struct Emitted {
    candidate: TripleCandidate,
    polarity: Polarity,
    single_filler: bool,
    state: Option<RelationshipState>,
    ends_state: bool,
}

/// Every candidate the rules fire on, in document order and with repeats
/// intact. IMPORTANT: this is the one place the evidence still exists in full.
/// `dedup_candidates` keeps one candidate per triple and drops the spans of the
/// rest, so anything that needs them has to read this list, not that output.
///
/// English reads its vocabulary from `pack`; `Options::language` selects
/// Chinese, which has a reader of its own (`chinese.rs`).
fn collect_candidates(
    text: &str,
    opts: &Options,
    context: &Context,
    evidence: Option<&mut DocumentEvidence>,
) -> Result<Vec<Emitted>> {
    let min_confidence = confidence_threshold(opts)?;
    if opts.language == Language::Chinese {
        let found = chinese::read(text, opts, evidence)
            .into_iter()
            .map(|r| (r, 0));
        return Ok(emit(found, opts, min_confidence));
    }
    let pack = &pack::ENGLISH;
    let document = coreference::Document::read(text, &opts.aliases, pack, context);
    let found = collect_document(text, opts, pack, &document, evidence);
    Ok(emit(found, opts, min_confidence))
}

fn confidence_threshold(opts: &Options) -> Result<f32> {
    let min_confidence = opts.min_confidence.unwrap_or(0.0);
    if !(0.0..=1.0).contains(&min_confidence) {
        return Err(crate::NarrativeGraphError::InvalidConfidenceThreshold(
            min_confidence,
        ));
    }

    Ok(min_confidence)
}

#[derive(Default)]
struct DocumentEvidence {
    characters: Vec<CharacterProfile>,
    mentions: Vec<CharacterMention>,
    events: Vec<EventCandidate>,
    knowledge: Vec<KnowledgeFact>,
    dialogue: Dialogue,
}

fn collect_document(
    text: &str,
    opts: &Options,
    pack: &LanguagePack,
    document: &coreference::Document<'_>,
    mut evidence: Option<&mut DocumentEvidence>,
) -> Vec<(relations::RelationCandidate, usize)> {
    let mut recent = coreference::Recent::default();
    let register = &document.register;
    let mut turn = dialogue::Turn::new(evidence.is_some());
    let mut found = Vec::new();

    for sentence in &document.sentences {
        if sentence.paragraph_start {
            found.extend(
                turn.finish(&opts.ontology, register, pack, false)
                    .into_iter()
                    .map(|r| (r, 0)),
            );
        }
        let entities = document.mentions(sentence, &mut recent, pack);
        turn.observe(
            sentence.text,
            sentence.start,
            &entities,
            &sentence.quotes,
            sentence.opened,
            pack,
        );

        if entities.is_empty() {
            continue;
        }

        // Look for relations between entity pairs in the same clause
        let scope = relations::Scope {
            register,
            quotes: &sentence.quotes,
        };
        if let Some(evidence) = evidence.as_deref_mut() {
            evidence
                .mentions
                .extend(
                    entities
                        .iter()
                        .chain(sentence.descriptions())
                        .filter_map(|mention| {
                            let kind = kinds::classify(sentence.text, mention, register, pack);
                            if !matches!(kind, kinds::Kind::Person | kinds::Kind::Unknown) {
                                return None;
                            }
                            let kind = if mention.pronoun {
                                MentionKind::Pronoun
                            } else if sentence
                                .named()
                                .binary_search_by_key(&mention.start, |m| m.start)
                                .is_ok_and(|i| sentence.named()[i].end == mention.end)
                            {
                                MentionKind::Name
                            } else {
                                MentionKind::Description
                            };
                            Some(CharacterMention {
                                id: mention.normalized.clone(),
                                kind,
                                span: [
                                    sentence.start + mention.start,
                                    sentence.start + mention.end,
                                ],
                            })
                        }),
                );
            evidence.events.extend(events::extract(
                sentence.text,
                sentence.start,
                &entities,
                pack,
                &scope,
            ));
        }
        found.extend(
            relations::extract_relations(sentence.text, &entities, &opts.ontology, pack, &scope)
                .into_iter()
                .map(|r| (r, sentence.start)),
        );
        found.extend(
            knowledge::extract(
                sentence.text,
                sentence.start,
                &entities,
                pack,
                &scope,
                &opts.ontology,
                evidence.as_deref_mut().map(|e| &mut e.knowledge),
            )
            .into_iter()
            .map(|r| (r, sentence.start)),
        );
    }
    found.extend(
        turn.finish(&opts.ontology, register, pack, true)
            .into_iter()
            .map(|r| (r, 0)),
    );
    let common = dialogue::common_words(
        text,
        found
            .iter()
            .filter(|(r, _)| dialogue::is_rule(&r.rule))
            .flat_map(|(r, _)| [r.subject.as_str(), r.object.as_str()])
            .chain(turn.line_names()),
    );
    found.retain(|(r, _)| {
        !dialogue::is_rule(&r.rule)
            || (dialogue::is_voice(&r.subject, 2, &turn, register)
                && dialogue::is_voice(&r.object, 1, &turn, register)
                && ![&r.subject, &r.object]
                    .iter()
                    .any(|n| common.contains(*n) || pack.sentence_openers.contains(&n.as_str())))
    });
    if let Some(evidence) = evidence {
        evidence.characters = document.characters();
        evidence.dialogue = turn.into_dialogue(&common, register, pack, &opts.aliases);
    }

    found.sort_by_key(|(r, offset)| [offset + r.span[0], offset + r.span[1]]);

    found
}

/// Score, filter and convert what the readers found.
///
/// Each span is relative to its offset: the sentence's start, or 0 for a
/// paragraph-level candidate whose span is already absolute.
fn emit(
    found: impl IntoIterator<Item = (relations::RelationCandidate, usize)>,
    opts: &Options,
    min_confidence: f32,
) -> Vec<Emitted> {
    let rejected: BTreeSet<(&str, &str, &str)> = opts
        .rejections
        .iter()
        .map(|r| (r.subject.as_str(), r.relation.as_str(), r.object.as_str()))
        .collect();
    let mut candidates = Vec::new();
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
                state: rel.state,
                ends_state: rel.ends_state,
            });
        }
    }

    candidates
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
