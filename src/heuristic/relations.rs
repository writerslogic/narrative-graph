use super::entities::EntityCandidate;
use super::kinds::{classify, Kind};
use super::pack::{LanguagePack, Slot, VerbRule};
use super::quotes::QuoteMap;
use super::register::Register;
use crate::types::RelationshipState;
use std::collections::BTreeMap;

/// What the relation stage needs to know beyond the sentence's own mentions.
pub struct Scope<'a> {
    pub register: &'a Register,
    pub quotes: &'a QuoteMap,
}

/// A relation found between two entities, with the span it was found in and
/// the byte gap between the entities (used to weight confidence).
pub struct RelationCandidate {
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub rule: String,
    pub span: [usize; 2],
    pub base: f32,
    pub gap: usize,
    pub stance: Stance,
    /// Whether the rule's own relation admits one subject per object, read
    /// before the caller's ontology renames it.
    pub single_filler: bool,
    pub role: Option<&'static str>,
    pub state: Option<RelationshipState>,
    pub ends_state: bool,
}

/// Widest byte gap between two mentions any rule reads across.
const MAX_GAP_BYTES: usize = 30;

/// Extract relations between entity pairs in the same sentence/clause.
pub fn extract_relations(
    text: &str,
    entities: &[EntityCandidate],
    ontology: &BTreeMap<String, String>,
    pack: &LanguagePack,
    scope: &Scope,
) -> Vec<RelationCandidate> {
    let mut relations = Vec::new();

    debug_assert!(entities.windows(2).all(|w| w[0].start <= w[1].start));

    for (i, subj) in entities.iter().enumerate() {
        for obj in &entities[i + 1..] {
            // Mentions are ordered by start, so the gap only grows from here.
            if obj.start >= subj.end && obj.start - subj.end > MAX_GAP_BYTES {
                break;
            }
            // A pronoun carries its antecedent's normalized name, so the same
            // referent can appear twice in the pair loop.
            if subj.normalized == obj.normalized {
                continue;
            }
            // A pair read across a quotation mark pairs speech with narration.
            if !scope.quotes.contains_span(text, subj.start, obj.end) {
                continue;
            }

            if let Some(m) = find_relation_pattern(text, subj, obj, pack, scope) {
                let (s, o) = if m.swapped { (obj, subj) } else { (subj, obj) };
                let state = RelationshipState::from_relation(&m.relation);
                let span_end = m.span_end.unwrap_or(obj.end);
                if span_end > obj.end && !scope.quotes.contains_span(text, obj.end, span_end) {
                    continue;
                }
                relations.push(RelationCandidate {
                    single_filler: pack.single_filler_relations.contains(&m.relation.as_str()),
                    role: m.role,
                    state,
                    ends_state: state.is_some()
                        && m.stance == Stance::Denied
                        && text[subj.end..span_end]
                            .to_ascii_lowercase()
                            .contains(" no longer "),
                    subject: s.normalized.clone(),
                    relation: normalize_relation(&m.relation, ontology),
                    object: o.normalized.clone(),
                    rule: m.rule,
                    span: [subj.start, span_end],
                    base: m.base,
                    gap: m.gap,
                    stance: m.stance,
                });
            }
        }
    }

    relations
}

/// A pattern hit. `swapped` means the phrasing puts the relation's subject
/// second in the text, as passive voice does. `span_end` overrides the end of
/// the evidence span when the token licensing the relation sits past the
/// object, as the noun does in a possessive.
struct PatternMatch {
    relation: String,
    rule: String,
    base: f32,
    gap: usize,
    swapped: bool,
    span_end: Option<usize>,
    stance: Stance,
    role: Option<&'static str>,
}

impl PatternMatch {
    fn new(relation: &str, rule: impl Into<String>, base: f32, gap: usize) -> Self {
        Self {
            relation: relation.to_string(),
            rule: rule.into(),
            base,
            gap,
            swapped: false,
            span_end: None,
            stance: Stance::Asserted,
            role: None,
        }
    }

    fn swapped(self, swapped: bool) -> Self {
        Self { swapped, ..self }
    }

    fn stance(self, stance: Stance) -> Self {
        Self { stance, ..self }
    }

    fn span_end(self, span_end: usize) -> Self {
        Self {
            span_end: Some(span_end),
            ..self
        }
    }

    fn role(self, role: &'static str) -> Self {
        Self {
            role: Some(role),
            ..self
        }
    }
}

/// Byte range of the first ASCII-case-insensitive *whole-word* occurrence of
/// `needle`. IMPORTANT: whole-word matching is what stops "grandmother" from
/// matching "mother" and labeling it `mother_of`. A substring match would make
/// the lexicon order-dependent and mislabel silently when a new noun contains
/// an existing one.
fn find_ascii_ci_word(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    debug_assert!(needle.is_ascii() && !needle.is_empty());
    let (hay, needle) = (haystack.as_bytes(), needle.as_bytes());
    if hay.len() < needle.len() {
        return None;
    }

    (0..=hay.len() - needle.len())
        .find(|&i| {
            hay[i..i + needle.len()].eq_ignore_ascii_case(needle)
                && !hay
                    .get(i.wrapping_sub(1))
                    .is_some_and(u8::is_ascii_alphanumeric)
                && !hay
                    .get(i + needle.len())
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
        .map(|i| (i, i + needle.len()))
}

/// Byte range of the first ASCII-case-insensitive occurrence of `needle`.
/// IMPORTANT: offsets address `haystack` itself. Searching a lowercased copy
/// yields offsets that do not address the original once a character's
/// lowercase form has a different byte length. A match is always on a char
/// boundary because a UTF-8 continuation or lead byte cannot equal an ASCII
/// byte under `eq_ignore_ascii_case`.
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    debug_assert!(needle.is_ascii() && !needle.is_empty());
    let needle = needle.as_bytes();
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
        .map(|start| (start, start + needle.len()))
}

/// Length of the possessed noun phrase introduced by `'s`, measured from the
/// end of that `'s` and cut at the first clause boundary. IMPORTANT: the
/// phrase bounds the relational-noun search. Scanning the whole remainder of
/// the sentence matches a noun belonging to a later clause ("Marco's dog, and
/// she has a sister") and emits the highest confidence rule in the system.
fn possessed_noun_phrase_len(rest: &str, pack: &LanguagePack) -> usize {
    let punct = rest
        .find([',', '.', ';', ':', '!', '?'])
        .unwrap_or(rest.len());
    let conjunction = find_ascii_ci(rest, pack.phrase_conjunction)
        .map(|(start, _)| start)
        .unwrap_or(rest.len());
    punct.min(conjunction)
}

/// What the text between two mentions does to the relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    /// The text states the relation holds.
    Asserted,
    /// The text states it does not. A denial is not a weak assertion; it is
    /// the opposite claim, and it is what makes a contradiction representable.
    Denied,
    /// The text neither states nor denies it: a conditional, a modal, or a
    /// complement the main verb holds open. Nothing is emitted for these.
    Suspended,
}

/// Whether the text between two mentions holds an infinitive "to". A complement
/// verb suspends its infinitive: "refused to mentor", "hoped to mentor",
/// "wanted to work" assert nothing about the event. A phrase rule whose own
/// words include "to" ("moved to", "married to") is not suspended by it.
fn has_infinitive(between: &str, pack: &LanguagePack) -> bool {
    between
        .split_whitespace()
        .any(|word| word == pack.infinitive_marker)
}

/// Read the text between two mentions as a stance on the relation.
///
/// IMPORTANT: a suspender outranks a negator. "if Elena is not Marco's sister"
/// carries both and settles neither, so it is suspended; treating it as a
/// denial would hand the contradiction check a conflict the text never states.
fn assertion_stance(between: &str, pack: &LanguagePack) -> Stance {
    let mut denied = false;

    for word in between.split(|c: char| !c.is_ascii_alphanumeric() && c != '\'') {
        if pack.suspending_words.contains(&word) {
            return Stance::Suspended;
        }
        // "didn't", "doesn't", "isn't": the negator is a suffix, not a word.
        if word.ends_with(pack.negation_clitic) || pack.negators.contains(&word) {
            denied = true;
        }
    }

    if denied {
        Stance::Denied
    } else {
        Stance::Asserted
    }
}

/// Whether a word before `from` holds the clause open. `text` is one sentence,
/// so the scan cannot reach back into a previous one.
fn suspended_before(text: &str, from: usize, pack: &LanguagePack) -> bool {
    text[..from]
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| {
            let word = word.to_lowercase();
            pack.suspending_words.contains(&word.as_str()) || word == pack.infinitive_marker
        })
}

/// Whether the sentence holding the mentions is a question. IMPORTANT: "Did
/// Elena mentor Marco?" asks whether the relation holds; it does not state it.
/// The interrogative auxiliary is not between the mentions, so only the
/// terminator can tell. An abbreviation's period ends the scan early, which
/// errs toward reading the sentence as a statement.
pub(super) fn ends_in_question(text: &str, from: usize) -> bool {
    let tail = &text[from..];
    matches!(tail.find(['.', '!', '?']), Some(i) if tail[i..].starts_with('?'))
}

/// Whether the relational noun spanning `[start, end)` is the head of the
/// possessed phrase and is possessed by the object itself.
///
/// IMPORTANT: a noun that modifies a later one names a thing, not a relation
/// ("Marco's master key"), and a noun behind a second possessive belongs to
/// that possessor ("Marco's friend's sister" is the sister of the friend). The
/// phrase is already cut at the first clause boundary, so what follows the
/// noun here is the rest of one noun phrase.
fn heads_possessed_phrase(phrase: &str, start: usize, end: usize, pack: &LanguagePack) -> bool {
    let after = phrase[end..].trim();
    let closes = after.is_empty()
        || after
            .split(|c: char| !c.is_alphabetic())
            .next()
            .is_some_and(|word| {
                pack.possessive_tails
                    .contains(&word.to_lowercase().as_str())
            });
    closes && find_ascii_ci(&fold_apostrophe(&phrase[..start]), pack.possessive_clitic).is_none()
}

/// The possessed phrase in "y's <phrase> <copula> x", where the possessive sits
/// on the *earlier* mention and the copula hands the relation to the later one.
///
/// IMPORTANT: this is the mirror of the forward possessive, not a loosening of
/// it. The copula must still be the whole link between the phrase and the
/// object, so "Mingott's father was once Bob Spicer" does not match; the same
/// closed set of copulas applies, and the assertion gate has already rejected
/// anything that denies or suspends the claim.
fn reversed_possessive_phrase<'a>(between: &'a str, pack: &LanguagePack) -> Option<&'a str> {
    let rest = between.strip_prefix(pack.possessive_clitic)?;
    if !rest.starts_with(' ') {
        return None;
    }
    let (phrase, copula) = rest.trim().rsplit_once(' ')?;
    pack.possessive_copulas
        .contains(&copula)
        .then(|| phrase.trim_end())
}

/// Whether the text between the mentions is the whole link of a possessive
/// statement, either way round: "x is y's sister", "x is not y's sister".
///
/// IMPORTANT: the link must still be the whole text between the mentions, and
/// the negated form is as closed as the bare one. Only a copula, a copula
/// followed by exactly one negator, or a copula carrying the `n't` clitic will
/// do; "was once", "believed ... was" and "is not merely" are none of those.
/// Widening this is how "Elena visited Marco's sister" starts claiming a third
/// person's relation for the subject at the highest confidence in the system.
fn is_possessive_link(between: &str, pack: &LanguagePack) -> bool {
    let trimmed = between.trim();
    if pack.possessive_copulas.contains(&trimmed) || is_appositive_link(between, pack) {
        return true;
    }

    match trimmed.split_once(' ') {
        Some((copula, negator)) => {
            pack.possessive_copulas.contains(&copula)
                && (pack.negators.contains(&negator) || negator == "no longer")
        }
        None => trimmed
            .strip_suffix(pack.negation_clitic)
            .is_some_and(|copula| pack.possessive_copulas.contains(&copula)),
    }
}

/// Whether the text between the mentions is an appositive comma, so that
/// "x, y's <noun>" renames x as that noun.
///
/// IMPORTANT: everything after the comma must be a pre-nominal modifier. A verb
/// or a relative pronoun there means the possessive belongs to a clause about x
/// rather than a renaming of it, and the relation would name the wrong person.
fn is_appositive_link(between: &str, pack: &LanguagePack) -> bool {
    let Some(rest) = between.trim_start().strip_prefix(',') else {
        return false;
    };
    rest.split_whitespace()
        .all(|word| pack.appositive_modifiers.contains(&word))
}

/// The relational noun in an "of" genitive: "x, the <noun> of y", or the same
/// with a copula in place of the comma.
///
/// IMPORTANT: this is the form English prose actually prefers, and the reason
/// a possessive-only rule set reads almost nothing. Measured over three novels
/// in `docs/EVALUATION.md`, "x is y's <noun>" occurs zero times and the "of"
/// genitive nineteen.
fn of_genitive_noun<'a>(between: &'a str, pack: &LanguagePack) -> Option<&'a str> {
    let trimmed = between.trim();
    let rest = match trimmed.strip_prefix(',') {
        Some(rest) => rest,
        None => {
            let (link, rest) = trimmed.split_once(' ')?;
            pack.possessive_copulas.contains(&link).then_some(rest)?
        }
    };

    let mut words = rest.split_whitespace();
    if words.next_back()? != pack.genitive_link {
        return None;
    }
    let noun = words.next_back()?;
    // A negator is admitted alongside the modifiers so that "x is not the
    // sister of y" reaches the rule at all. Which claim it makes is the
    // caller's question; `assertion_stance` has already read the same window.
    words
        .all(|word| pack.appositive_modifiers.contains(&word) || pack.negators.contains(&word))
        .then_some(noun)
}

/// Whether an apostrophe-s opens at `from`, marking the mention before it as a
/// possessor.
pub(super) fn opens_possessive(text: &str, from: usize, pack: &LanguagePack) -> bool {
    clitic_len(&text.as_bytes()[from..], pack).is_some()
}

/// Byte length of the possessive clitic `bytes` opens with, matched
/// case-insensitively, or `None`. IMPORTANT: the typographic apostrophe is the
/// same clitic as the straight one. Word processors emit it throughout a
/// manuscript, and a rule that reads only the straight form reads nothing there.
pub(super) fn clitic_len(bytes: &[u8], pack: &LanguagePack) -> Option<usize> {
    let clitic = pack.possessive_clitic.as_bytes();
    if bytes
        .get(..clitic.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(clitic))
    {
        return Some(clitic.len());
    }
    const CURLY: &[u8] = "\u{2019}".as_bytes();
    let rest = &clitic[1..];
    (bytes.starts_with(CURLY)
        && bytes
            .get(CURLY.len()..CURLY.len() + rest.len())
            .is_some_and(|tail| tail.eq_ignore_ascii_case(rest)))
    .then_some(CURLY.len() + rest.len())
}

/// `text` with the typographic apostrophe folded to the straight one, so that
/// the clitic compares equal wherever it is read from a window.
fn fold_apostrophe(text: &str) -> String {
    text.replace('\u{2019}', "'")
}

fn find_relation_pattern(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
    pack: &LanguagePack,
    scope: &Scope,
) -> Option<PatternMatch> {
    // IMPORTANT: use each mention's own offsets. Re-locating a surface form by
    // substring search finds the first occurrence anywhere in the text, and an
    // offset into a lowercased copy does not address `text`.
    // Mentions can nest ("Jane" inside "Mary Jane"), so this is also the bound
    // that keeps `obj.start - subj.end` from underflowing.
    if subj.end > obj.start || super::coreference::genitive_head(text, subj.start, pack).is_some() {
        return None;
    }

    let (start, end) = (subj.end, obj.start);
    let gap = end - start;

    if gap > MAX_GAP_BYTES {
        return None;
    }

    let between = &fold_apostrophe(&text[start..end].to_lowercase());

    // IMPORTANT: a question is suspended, never denied. "Did Elena mentor
    // Marco?" does not claim the relation and does not deny it either.
    // A denial is also suspended by "to" or a capitalized suspender before the
    // subject, which `conditioned_before` below does not count.
    let stance = match assertion_stance(between, pack) {
        Stance::Denied if suspended_before(text, subj.start, pack) => Stance::Suspended,
        other => other,
    };
    if stance == Stance::Suspended || ends_in_question(text, obj.end) {
        return None;
    }

    // A clause opened by a suspender states nothing, whichever rule reads it.
    if conditioned_before(text, subj.start, pack) {
        return None;
    }

    // The rules written before phrase rules existed read an infinitive as a
    // suspension. A phrase rule owns its own "to".
    if !has_infinitive(between, pack) {
        if let Some(m) = match_rules(text, subj, obj, pack, between, gap) {
            return Some(m.stance(stance));
        }
    }

    if !usable_mention(subj, pack) || !usable_mention(obj, pack) {
        return None;
    }
    let found = match_pronoun_possessive(text, subj, obj, pack, scope, between, gap)
        .or_else(|| match_kin_prefix(text, subj, obj, pack, scope, between, gap))
        .or_else(|| match_plural_pair(text, subj, obj, pack, scope, between, gap))
        .or_else(|| match_phrase_rules(text, subj, obj, pack, scope, between, gap))?;
    // A plural pair carries its own negation ("are not sisters"), which sits
    // after the object and so is invisible to the window read above.
    Some(match found.stance {
        Stance::Asserted => found.stance(stance),
        _ => found,
    })
}

/// Whether a suspending word opens the clause the subject sits in: "If Nora
/// married Dev, ..." and "Wren asked if she loved Dev" state nothing, in either
/// polarity. A lowercase "may" is a modal and a capitalized one a name, so only the
/// lowercase form counts, except at the start of the sentence.
pub(super) fn conditioned_before(text: &str, from: usize, pack: &LanguagePack) -> bool {
    // A suspender governs its own clause. Reading past a semicolon, dash or
    // quotation mark reaches a clause the segmenter failed to split off.
    let prefix = &text[..from];
    let clause_start = prefix
        .rfind([
            ';', ':', '\u{2014}', '"', '\u{201C}', '\u{201D}', '\u{2018}',
        ])
        .map(|i| i + prefix[i..].chars().next().map_or(1, char::len_utf8))
        .max(prefix.rfind("--").map(|i| i + 2))
        .unwrap_or(0);
    text[clause_start..from]
        .split(|c: char| !c.is_alphabetic())
        .enumerate()
        .any(|(index, word)| {
            let lower = word.to_lowercase();
            pack.suspending_words.contains(&lower.as_str())
                && (word == lower || (index == 0 && lower != "may"))
        })
}

/// Whether a mention can stand in a rule added after phrase rules: not a plural
/// pronoun, which names a group the pipeline resolves to one person.
fn usable_mention(entity: &EntityCandidate, pack: &LanguagePack) -> bool {
    !(entity.pronoun
        && pack
            .plural_pronouns
            .contains(&entity.text.to_lowercase().as_str()))
}

/// Whether a pronoun mention may be one end of a phrase rule. A possessive
/// determiner is a possessor, never an actor or a patient ("she meets my
/// gaze"), and an object pronoun must end its phrase: "kissed her" is an
/// object and "kissed her cheek" is not.
pub(super) fn pronoun_fits_phrase(
    text: &str,
    entity: &EntityCandidate,
    pack: &LanguagePack,
    subject: bool,
) -> bool {
    if !entity.pronoun {
        return true;
    }
    let word = entity.text.to_lowercase();
    if matches!(word.as_str(), "my" | "his" | "their" | "its" | "our") {
        return false;
    }
    if subject {
        word == "i" || pack.subject_pronouns.contains(&word.as_str())
    } else {
        object_pronoun_ends_phrase(text, entity.end, pack)
    }
}

/// Whether "her" after a verb is an object: the clause ends, or a preposition or
/// conjunction follows. "kissed her" is an object and "kissed her cheek" is not.
fn object_pronoun_ends_phrase(text: &str, from: usize, pack: &LanguagePack) -> bool {
    let rest = text[from..].trim_start();
    match rest.chars().next() {
        None => true,
        Some(c) if !c.is_alphabetic() => true,
        Some(_) => {
            let word: String = rest
                .chars()
                .take_while(|c| c.is_alphabetic())
                .collect::<String>()
                .to_lowercase();
            pack.object_followers.contains(&word.as_str())
        }
    }
}

/// The mention's kind, with the sentence it stands in.
fn kind_of(text: &str, entity: &EntityCandidate, pack: &LanguagePack, scope: &Scope) -> Kind {
    classify(text, entity, scope.register, pack)
}

/// Words of the window between two mentions, with commas as words of their own,
/// or `None` where the window holds anything but letters, apostrophes, hyphens
/// and commas. A dash, a bracket or a semicolon is a clause boundary the phrase
/// rules do not read across.
fn window_tokens(between: &str) -> Option<Vec<&str>> {
    if between
        .chars()
        .any(|c| !(c.is_alphabetic() || c.is_whitespace() || matches!(c, ',' | '\'' | '-')))
    {
        return None;
    }
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, c) in between
        .char_indices()
        .chain(std::iter::once((between.len(), ' ')))
    {
        if c.is_whitespace() || c == ',' {
            if let Some(from) = start.take() {
                tokens.push(&between[from..index]);
            }
            if c == ',' {
                tokens.push(",");
            }
        } else {
            start.get_or_insert(index);
        }
    }
    Some(tokens)
}

/// The exact-phrase rules. See `pack/phrases.rs` for what "exact" means.
fn match_phrase_rules(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
    pack: &LanguagePack,
    scope: &Scope,
    between: &str,
    gap: usize,
) -> Option<PatternMatch> {
    if !pronoun_fits_phrase(text, subj, pack, true) || !pronoun_fits_phrase(text, obj, pack, false)
    {
        return None;
    }
    let mut tokens = window_tokens(between)?;
    // "Marco, who married Elena": the relative opener is the only comma form.
    if tokens.first() == Some(&",") {
        if !matches!(tokens.get(1), Some(&"who") | Some(&"whom")) {
            return None;
        }
        tokens.drain(..2);
    }
    while tokens
        .last()
        .is_some_and(|w| pack.phrase_trailers.contains(w))
    {
        tokens.pop();
    }
    if tokens.is_empty() || tokens.contains(&",") {
        return None;
    }

    // Possessives of the object make it a possessor ("met Marco's sister"), not
    // the thing the verb acts on.
    if opens_possessive(text, obj.end, pack) {
        return None;
    }

    let (subject_kind, object_kind) = (
        kind_of(text, subj, pack, scope),
        kind_of(text, obj, pack, scope),
    );
    for split in 0..tokens.len() {
        if !tokens[..split]
            .iter()
            .all(|w| pack.phrase_fillers.contains(w))
        {
            break;
        }
        let rest = tokens[split..].join(" ");
        for rule in pack.phrase_rules {
            if !rule.phrases.contains(&rest.as_str()) {
                continue;
            }
            if !rule.subject.admits(subject_kind) || !rule.object.admits(object_kind) {
                continue;
            }
            if rule.relation == "knows" && super::knowledge::opens_complement(&text[obj.end..]) {
                continue;
            }
            return Some(
                PatternMatch::new(rule.relation, rule.rule, rule.base, gap).swapped(rule.passive),
            );
        }
    }
    None
}

/// "x and y are sisters" / "x and y are not friends" / "x and y are married".
fn match_plural_pair(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
    pack: &LanguagePack,
    scope: &Scope,
    between: &str,
    gap: usize,
) -> Option<PatternMatch> {
    if between.trim() != "and" || subj.pronoun || obj.pronoun {
        return None;
    }
    // "A, B and C are friends" would pair B with C and leave A out; the pair
    // that is read must be the whole subject.
    if text[..subj.start].trim_end().ends_with(',') {
        return None;
    }
    let mut words = text[obj.end..]
        .split(|c: char| !c.is_alphabetic() && c != '\'')
        .filter(|w| !w.is_empty());
    let copula = words.next()?.to_lowercase();
    if !pack.plural_copulas.contains(&copula.as_str()) {
        return None;
    }
    let mut word = words.next()?;
    let mut denied = false;
    if pack.negators.iter().any(|n| word.eq_ignore_ascii_case(n)) {
        denied = true;
        let no_longer = word.eq_ignore_ascii_case("no");
        word = words.next()?;
        if no_longer && word.eq_ignore_ascii_case("longer") {
            word = words.next()?;
        }
    }
    // "are old friends", "are very close friends".
    let mut skipped = 0;
    while skipped < 2
        && pack
            .appositive_modifiers
            .iter()
            .chain(pack.plural_modifiers)
            .any(|m| word.eq_ignore_ascii_case(m))
    {
        word = words.next()?;
        skipped += 1;
    }
    let (_, relation, base) = pack
        .plural_nouns
        .iter()
        .find(|(noun, _, _)| word.eq_ignore_ascii_case(noun))?;
    if !Slot::Person.admits(kind_of(text, subj, pack, scope))
        || !Slot::Person.admits(kind_of(text, obj, pack, scope))
    {
        return None;
    }
    let end = word.as_ptr() as usize - text.as_ptr() as usize + word.len();
    let mut found = PatternMatch::new(
        relation,
        format!("plural-{}-pattern", word.to_ascii_lowercase()),
        *base,
        gap,
    )
    .span_end(end);
    if let Some((role, _, _)) = pack.possessive_nouns.iter().find(|(role, name, _)| {
        name == relation
            && word
                .strip_suffix(['s', 'S'])
                .is_some_and(|w| w.eq_ignore_ascii_case(role))
    }) {
        found = found.role(role);
    }
    if denied {
        found = found.stance(Stance::Denied);
    }
    Some(found)
}

/// "x's <noun> y" and "his <noun> y": the possessor comes first, the possessed
/// person is named after the noun, and the relation reads `<noun>_of(y, x)`.
fn match_kin_prefix(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
    pack: &LanguagePack,
    scope: &Scope,
    between: &str,
    gap: usize,
) -> Option<PatternMatch> {
    let pronoun_possessor = subj.pronoun
        && pack
            .possessive_pronouns
            .contains(&subj.text.to_lowercase().as_str());
    let rest = if pronoun_possessor {
        between
    } else if subj.pronoun {
        return None;
    } else {
        between.strip_prefix(pack.possessive_clitic)?
    };
    if !rest.starts_with(' ') {
        return None;
    }
    if pronoun_possessor
        && rest.trim_end().ends_with(',')
        && !text[obj.end..]
            .trim_start()
            .starts_with([',', '.', ';', '?', '!', '”', '"'])
    {
        return None;
    }
    let tokens = window_tokens(rest.trim_end_matches([' ', ',']))?;
    let (noun, modifiers) = tokens.split_last()?;
    if tokens.contains(&",")
        || !modifiers
            .iter()
            .all(|w| pack.appositive_modifiers.contains(w))
    {
        return None;
    }
    let (role, relation, base) = pack.possessive_nouns.iter().find(|(n, _, _)| n == noun)?;
    if obj.pronoun {
        return None;
    }
    if !Slot::Person.admits(kind_of(text, subj, pack, scope))
        || !Slot::Person.admits(kind_of(text, obj, pack, scope))
    {
        return None;
    }
    if pronoun_possessor && !possessor_is_sure(subj, scope, pack) {
        return None;
    }
    if pronoun_possessor
        && !super::coreference::role_gender_agrees(scope.register, &obj.normalized, role)
    {
        return None;
    }
    let base = if pronoun_possessor {
        base - 0.12
    } else {
        *base
    };
    Some(
        PatternMatch::new(relation, format!("kin-prefix-{noun}-pattern"), base, gap)
            .role(role)
            .swapped(true),
    )
}

/// "x, my <noun>" and "x is her <noun>": the possessor is a pronoun after the
/// subject, and the relation reads `<noun>_of(x, owner)`.
fn match_pronoun_possessive(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
    pack: &LanguagePack,
    scope: &Scope,
    between: &str,
    gap: usize,
) -> Option<PatternMatch> {
    if !obj.pronoun
        || subj.pronoun
        || !pack
            .possessive_pronouns
            .contains(&obj.text.to_lowercase().as_str())
        || !is_possessive_link(between, pack)
    {
        return None;
    }
    let rest = &text[obj.end..];
    let phrase = &rest[..possessed_noun_phrase_len(rest, pack)];
    if !phrase.starts_with(' ') {
        return None;
    }
    if !Slot::Person.admits(kind_of(text, subj, pack, scope)) {
        return None;
    }
    if !possessor_is_sure(obj, scope, pack) {
        return None;
    }
    for (noun, relation, base) in pack.possessive_nouns {
        let Some((noun_start, noun_end)) = find_ascii_ci_word(phrase, noun) else {
            continue;
        };
        if !heads_possessed_phrase(phrase, noun_start, noun_end, pack)
            || !phrase[..noun_start].split_whitespace().all(|w| {
                pack.appositive_modifiers
                    .contains(&w.to_lowercase().as_str())
            })
        {
            continue;
        }
        let base = if is_first_person(obj, pack) {
            *base
        } else {
            base - 0.12
        };
        return Some(
            PatternMatch::new(relation, format!("pronoun-{noun}-pattern"), base, gap)
                .role(noun)
                .span_end(obj.end + noun_end),
        );
    }
    None
}

fn is_first_person(entity: &EntityCandidate, pack: &LanguagePack) -> bool {
    pack.first_person_pronouns
        .contains(&entity.text.to_lowercase().as_str())
}

/// Whether a possessive pronoun's referent can be trusted. The narrator's "my"
/// is a referent the caller named; any other pronoun must agree with what the
/// document has said about the name it resolved to, because "her sister" in a
/// sentence with two women is a coin toss the nearest-mention rule makes.
fn possessor_is_sure(pronoun: &EntityCandidate, scope: &Scope, pack: &LanguagePack) -> bool {
    is_first_person(pronoun, pack)
        || scope
            .register
            .allows(&pronoun.text, &pronoun.normalized, pack)
}

/// The rule table proper, once the window between the mentions has been bounded
/// and read for stance. IMPORTANT: every rule here is stance-blind — it decides
/// *which* relation the phrasing names, and the caller decides whether the text
/// asserts or denies it. The two questions were one `bool` before denials
/// existed, and keeping them apart is what stops a negated sentence from being
/// silently dropped instead of recorded as the opposite claim.
fn match_rules(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
    pack: &LanguagePack,
    between: &str,
    gap: usize,
) -> Option<PatternMatch> {
    // Possessive pattern: "x is y's [relation]". Matched against the original
    // text so the noun's offsets can bound the evidence span.
    let after_obj = &text[obj.end..];
    let bytes = after_obj.as_bytes();
    let clitic_len = clitic_len(bytes, pack).unwrap_or(0);
    let possessive = clitic_len > 0 && matches!(bytes.get(clitic_len), Some(b' ') | Some(b'.'));

    if possessive && is_possessive_link(between, pack) {
        let rest = &after_obj[clitic_len..];
        let phrase = &rest[..possessed_noun_phrase_len(rest, pack)];
        let phrase_start = obj.end + clitic_len;

        for (noun, relation, base) in pack.possessive_nouns {
            let Some((noun_start, noun_end)) = find_ascii_ci_word(phrase, noun) else {
                continue;
            };
            if !heads_possessed_phrase(phrase, noun_start, noun_end, pack) {
                continue;
            }
            return Some(
                PatternMatch::new(relation, format!("possessive-{noun}-pattern"), *base, gap)
                    .role(noun)
                    .span_end(phrase_start + noun_end),
            );
        }
    }

    // "of" genitive: "x, the [relation] of y". IMPORTANT: same guard as the
    // reversed possessive — "Elena, the sister of Marco's wife" names the wife,
    // not Marco.
    if let Some(noun) = of_genitive_noun(between, pack) {
        if !opens_possessive(text, obj.end, pack) {
            if let Some((role, relation, base)) =
                pack.possessive_nouns.iter().find(|(n, _, _)| *n == noun)
            {
                return Some(
                    PatternMatch::new(relation, format!("of-genitive-{noun}-pattern"), *base, gap)
                        .role(role),
                );
            }
        }
    }

    // Reversed possessive: "y's [relation] is x". IMPORTANT: the object must not
    // open a possessive of its own. In "Elena's mother was Marco's sister" the
    // sister belongs to Marco, and reading the copula as linking Elena's mother
    // to Marco states a relation the sentence never makes.
    if let Some(phrase) = reversed_possessive_phrase(between, pack) {
        if !opens_possessive(text, obj.end, pack) {
            for (noun, relation, base) in pack.possessive_nouns {
                let Some((noun_start, noun_end)) = find_ascii_ci_word(phrase, noun) else {
                    continue;
                };
                if !heads_possessed_phrase(phrase, noun_start, noun_end, pack) {
                    continue;
                }
                return Some(
                    PatternMatch::new(relation, format!("possessive-{noun}-pattern"), *base, gap)
                        .role(noun)
                        .swapped(true),
                );
            }
        }
    }

    // Verb patterns. The markers carry their own word boundary, so the table is
    // walked in order and the first rule whose tokens are all present wins.
    if opens_possessive(text, subj.end, pack)
        || opens_possessive(text, obj.end, pack)
        || super::coreference::genitive_head(text, obj.start, pack).is_some()
        || !pronoun_fits_phrase(text, subj, pack, true)
        || !pronoun_fits_phrase(text, obj, pack, false)
    {
        return None;
    }
    if let Some(m) = match_verb_rules(between, pack.verb_rules, gap) {
        return Some(m);
    }
    if between.contains(pack.relative_opener) {
        if let Some(m) = match_verb_rules(between, pack.relative_rules, gap) {
            return Some(m);
        }
    }

    None
}

/// The first rule in `rules` whose marker, and second marker where it has one,
/// both appear in the text between the mentions.
fn match_verb_rules(between: &str, rules: &[VerbRule], gap: usize) -> Option<PatternMatch> {
    rules
        .iter()
        .find(|rule| {
            between.contains(rule.marker) && rule.also.map_or(true, |also| between.contains(also))
        })
        .map(|rule| {
            // IMPORTANT: passive voice ("was mentored by") names the mentor
            // second, so the relation's subject is the later entity.
            let passive = rule
                .passive_marker
                .is_some_and(|marker| between.contains(marker));
            PatternMatch::new(rule.relation, rule.rule, rule.base, gap).swapped(passive)
        })
}

/// Normalize a relation type against an optional caller-supplied ontology.
/// If the relation is in the ontology map, return the mapped value; otherwise return as-is.
pub fn normalize_relation(rel: &str, ontology: &BTreeMap<String, String>) -> String {
    if ontology.is_empty() {
        return rel.to_string();
    }
    let rel_lower = rel.to_lowercase();

    if let Some(canonical) = ontology.get(&rel_lower) {
        canonical.clone()
    } else {
        rel.to_string()
    }
}
