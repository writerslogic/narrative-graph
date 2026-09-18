use super::pack::LanguagePack;
use super::register::Register;
use std::collections::BTreeMap;

/// Entity candidate with start and end positions in the text.
#[derive(Debug, Clone)]
pub struct EntityCandidate {
    pub text: String,
    pub normalized: String,
    pub start: usize,
    pub end: usize,
}

/// Extract entities from a sentence with the pipeline's own language pack and
/// no context from any preceding sentence.
pub fn extract_entities(text: &str, aliases: &BTreeMap<String, String>) -> Vec<EntityCandidate> {
    extract_entities_with(
        text,
        aliases,
        &super::pack::ENGLISH,
        &[],
        &Register::default(),
    )
}

/// The capitalized runs of one sentence, for the register pre-pass. Aliases and
/// pronouns are deliberately absent: the register records what the text says
/// about a name, before anything is resolved or remapped.
pub fn mentions_for_register(text: &str, pack: &LanguagePack) -> Vec<EntityCandidate> {
    extract_capitalized_entities(text, pack)
}

/// The pronouns of one sentence with their offsets, for the register pre-pass.
pub fn pronouns_for_register(text: &str, pack: &LanguagePack) -> Vec<(String, usize)> {
    extract_pronouns(text, pack)
        .into_iter()
        .map(|p| (p.text, p.start))
        .collect()
}

/// Extract entities from a sentence using capitalization + pronoun linking +
/// alias resolution.
///
/// `carried` holds the referents named by the previous sentence, in the order
/// it named them, and is consulted only for a pronoun with no antecedent in
/// this sentence. See `resolve_pronouns` for what that ordering assumes.
pub fn extract_entities_with(
    text: &str,
    aliases: &BTreeMap<String, String>,
    pack: &LanguagePack,
    carried: &[String],
    register: &Register,
) -> Vec<EntityCandidate> {
    let mut entities = Vec::new();

    // Find capitalized sequences (multi-word proper nouns and single capitalized words)
    let entities_from_capitalization = extract_capitalized_entities(text, pack);
    entities.extend(entities_from_capitalization);

    // Resolve aliases: any mention in the aliases map becomes the mapped canonical name
    for entity in &mut entities {
        if let Some(canonical) = aliases.get(&entity.text) {
            entity.normalized = canonical.clone();
        }
    }

    // IMPORTANT: after the exact-match loop, never before. A lexicon mention
    // carries its canonical name already, and the loop above keys on the
    // surface form, which would overwrite it wherever the two collide.
    let from_lexicon = lexicon_mentions(text, aliases, &entities);

    entities.extend(resolve_pronouns(
        text,
        pack,
        &from_lexicon,
        carried,
        register,
    ));

    entities.extend(from_lexicon);

    // IMPORTANT: pronouns are appended grouped by pronoun word, so the vector
    // is not in text order until sorted. `extract_relations` pairs entities by
    // index and treats the earlier index as the subject.
    entities.sort_by_key(|e| e.start);

    // IMPORTANT: every mention is kept. Collapsing repeated mentions of one
    // referent hides the second relation in "Elena is Marco's sister and Marco
    // mentors Elena." `extract_relations` skips pairs that share a referent,
    // and `dedup_candidates` collapses triples that repeat.
    entities
}

fn is_title_word(word: &str, next: Option<&str>, pack: &LanguagePack) -> bool {
    let word = word.to_lowercase();
    if pack.honorific_qualifiers.contains(&word.as_str()) {
        return next.is_some_and(|n| pack.honorifics.contains(&n.to_lowercase().as_str()));
    }
    pack.honorifics.contains(&word.as_str())
}

/// Drop the titles leading a mention, keeping the last one when only a single
/// name word would remain.
///
/// IMPORTANT: the carve-out is what keeps the title that is doing the
/// distinguishing work. Stripping unconditionally normalizes "Miss Darcy" and
/// "Mr. Darcy" both to `darcy`, which is not a person and, where the two
/// appear in one sentence, turns a real relation into a self-relation that
/// `extract_relations` then drops. A title in front of two or more name words
/// distinguishes nothing the name does not.
fn strip_honorifics<'a>(entity: &'a str, pack: &LanguagePack) -> &'a str {
    let mut rest = entity;

    while let Some((head, tail)) = rest.split_once(' ') {
        // Stop while one name word would be left: that word is a bare given
        // name or surname, and the title is the only thing separating it from
        // everyone else who shares it. A particle is not one of those words —
        // "Lady de Bourgh" reduced to `de_bourgh` names the surname two people
        // share, which is the error the particle list exists to prevent.
        if name_word_count(tail, pack) < 2 || !is_title_word(head, tail.split(' ').next(), pack) {
            break;
        }
        rest = tail;
    }

    rest
}

fn name_word_count(run: &str, pack: &LanguagePack) -> usize {
    run.split(' ')
        .filter(|w| !pack.name_particles.contains(&w.to_lowercase().as_str()))
        .count()
}

/// Whether `word_start` is the first word of a sentence, looking past any
/// opening quotation or bracket that precedes it.
fn is_sentence_start(text: &str, word_start: usize) -> bool {
    let mut quoted = false;

    for c in text[..word_start].chars().rev() {
        if c.is_whitespace() {
            continue;
        }
        if "\"'\u{201C}\u{2018}([".contains(c) {
            quoted = true;
            continue;
        }
        // IMPORTANT: a quotation opening after the attribution comma starts a
        // sentence too. `Elena said, "She is Marco's student."` is where a
        // capitalized pronoun actually occurs in fiction, and the comma is the
        // only thing before it.
        return ".!?\u{2026}".contains(c) || (quoted && c == ',');
    }

    true
}

/// Whether a capitalized word is capitalized only because a sentence starts
/// there, and so must not open a mention.
fn opens_sentence_by_position(
    text: &str,
    word: &str,
    word_start: usize,
    pack: &LanguagePack,
) -> bool {
    pack.sentence_openers
        .contains(&word.to_lowercase().as_str())
        && is_sentence_start(text, word_start)
}

/// The one place a completed capitalized run becomes a candidate, so that the
/// title rule is applied once rather than at each of the flush sites.
fn push_entity(
    entities: &mut Vec<EntityCandidate>,
    run: &str,
    start: usize,
    end: usize,
    pack: &LanguagePack,
) {
    // IMPORTANT: a pronoun is never a proper name, wherever it is capitalized.
    // `SENTENCE_OPENERS` catches one that opens a sentence; one mid-sentence
    // ("Elena said that She is Marco's sister") reached here and became a node
    // named `she`, which names nobody, alongside the correct mention.
    // A deictic pronoun is excluded for a sharper reason: "I" is capitalized in
    // every sentence of first-person narration, so `i` becomes a node naming
    // whoever is talking, and the carry then offers it to the next sentence.
    let is_pronoun = |list: &[&str]| list.iter().any(|p| run.eq_ignore_ascii_case(p));
    if is_pronoun(pack.pronouns) || is_pronoun(pack.deictic_pronouns) {
        return;
    }

    entities.push(EntityCandidate {
        text: run.to_string(),
        normalized: normalize_entity(strip_honorifics(run, pack)),
        start,
        end,
    });
}

fn extract_capitalized_entities(text: &str, pack: &LanguagePack) -> Vec<EntityCandidate> {
    let mut entities = Vec::new();
    let mut current_entity = String::new();
    let mut start_idx = 0;
    let mut current_word = String::new();
    let mut word_start = 0;
    // End of the last capitalized word folded into `current_entity`. The
    // separator position that flushes the entity sits after the following
    // lowercase word, so it does not bound the mention.
    let mut entity_end = 0;
    // Particles seen since the last capitalized word, held until a capitalized
    // word proves they sit inside the name. IMPORTANT: they never move
    // `entity_end`, so a particle that turns out to be trailing cannot extend
    // the mention past the person.
    let mut pending_particles = String::new();

    for (byte_pos, c) in text.char_indices() {
        // IMPORTANT: the typographic apostrophe separates exactly as the
        // straight one does. Without it "Blanche’s" is one capitalized run and
        // normalizes to `blanche’s`, a node that never unifies with any other
        // mention of her — and Gutenberg texts are typeset with the curly form
        // throughout, so this is the common case in real prose, not the rare
        // one. `find_pronoun_antecedent` already strips both.
        let is_sep = c.is_whitespace() || ",.!?;:—'\u{2019}\"".contains(c);

        if is_sep {
            if !current_word.is_empty() {
                let is_capitalized = current_word
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase())
                    && !(current_entity.is_empty()
                        && opens_sentence_by_position(text, &current_word, word_start, pack));

                if is_capitalized {
                    if !current_entity.is_empty() {
                        current_entity.push(' ');
                        current_entity.push_str(&pending_particles);
                    } else {
                        start_idx = word_start;
                    }
                    pending_particles.clear();
                    current_entity.push_str(&current_word);
                    entity_end = byte_pos;
                } else if !current_entity.is_empty()
                    && c.is_whitespace()
                    && pack
                        .name_particles
                        .contains(&current_word.to_lowercase().as_str())
                {
                    // Held, not folded: only a capitalized word after it proves
                    // the particle sits inside the name. Punctuation ends the
                    // name instead, so "Catherine de, who left" keeps its comma.
                    pending_particles.push_str(&current_word);
                    pending_particles.push(' ');
                } else {
                    if !current_entity.is_empty() {
                        push_entity(&mut entities, &current_entity, start_idx, entity_end, pack);
                        current_entity.clear();
                    }
                    pending_particles.clear();
                }
                current_word.clear();
            }
        } else {
            if current_word.is_empty() {
                word_start = byte_pos;
            }
            current_word.push(c);
        }
    }

    if !current_word.is_empty() {
        let is_capitalized = current_word
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase())
            && !(current_entity.is_empty()
                && opens_sentence_by_position(text, &current_word, word_start, pack));
        if is_capitalized {
            if !current_entity.is_empty() {
                current_entity.push(' ');
                current_entity.push_str(&pending_particles);
            } else {
                start_idx = word_start;
            }
            current_entity.push_str(&current_word);
            entity_end = text.len();
        } else if !current_entity.is_empty() {
            push_entity(&mut entities, &current_entity, start_idx, entity_end, pack);
            current_entity.clear();
        }
    }

    if !current_entity.is_empty() {
        push_entity(&mut entities, &current_entity, start_idx, entity_end, pack);
    }

    entities
}

/// Mentions a capitalized-run detector cannot produce, read from the lowercase
/// keys of `aliases`.
///
/// IMPORTANT: the caller has asserted the phrase names a person by putting it
/// in the map, so nothing here is inferred. The key's own shape selects the
/// rule: a key containing an uppercase character is the exact surface-form
/// match the loop above does, a wholly lowercase key is searched for here as a
/// literal, word-bounded, case-insensitive phrase. "The detective" opening a
/// sentence and "the detective" inside one are the same phrase, which is why
/// this half ignores case while the other half cannot.
///
/// A match overlapping a capitalized run is dropped, so no detected mention is
/// ever replaced: a key of "detective" cannot delete the run "Detective Marcus"
/// and take `marcus` with it. Longer keys are tried first, so "the detective"
/// wins over "detective" where both are supplied. A pronoun cuts the other way
/// and is dropped in favour of the lexicon mention covering it, because a
/// pronoun is a referent resolved by guess and a key is one the caller named.
fn lexicon_mentions(
    text: &str,
    aliases: &BTreeMap<String, String>,
    existing: &[EntityCandidate],
) -> Vec<EntityCandidate> {
    let mut keys: Vec<(&String, &String)> = aliases
        .iter()
        .filter(|(key, _)| !key.is_empty() && !key.chars().any(char::is_uppercase))
        .collect();
    if keys.is_empty() {
        return Vec::new();
    }
    keys.sort_by(|a, b| {
        b.0.chars()
            .count()
            .cmp(&a.0.chars().count())
            .then_with(|| a.0.cmp(b.0))
    });

    let mut found: Vec<EntityCandidate> = Vec::new();

    for (key, canonical) in keys {
        for (start, _) in text.char_indices() {
            if !is_word_boundary(text[..start].chars().next_back()) {
                continue;
            }
            let Some(end) = match_phrase(text, start, key) else {
                continue;
            };
            if !is_word_boundary(text[end..].chars().next()) {
                continue;
            }
            let overlaps = |m: &EntityCandidate| start < m.end && m.start < end;
            if existing.iter().any(overlaps) || found.iter().any(overlaps) {
                continue;
            }
            found.push(EntityCandidate {
                text: text[start..end].to_string(),
                normalized: canonical.clone(),
                start,
                end,
            });
        }
    }

    found
}

/// Whether the character bounding a phrase lets it stand as a whole word. The
/// end of the text bounds one too, hence `None`.
fn is_word_boundary(c: Option<char>) -> bool {
    !c.is_some_and(char::is_alphanumeric)
}

/// The end offset of `key` matched case-insensitively at `start`, or `None`.
///
/// IMPORTANT: walks the source text rather than a lowercased copy. A lowercase
/// form can differ in byte length from the character it came from, so offsets
/// taken from a lowercased copy do not address `text` and slicing with one
/// panics. `extract_pronouns` walks the source for the same reason.
fn match_phrase(text: &str, start: usize, key: &str) -> Option<usize> {
    let mut end = start;
    let mut want = key.chars();

    for got in text[start..].chars() {
        // A lowercase form can be several characters ("İ" lowercases to "i" and
        // a combining dot), so the key is consumed as a stream and the match
        // ends only on a character boundary of the source.
        for lowered in got.to_lowercase() {
            if want.next()? != lowered {
                return None;
            }
        }
        end += got.len_utf8();
        if want.as_str().is_empty() {
            return Some(end);
        }
    }

    None
}

#[derive(Debug)]
struct Pronoun {
    text: String,
    start: usize,
    end: usize,
}

fn extract_pronouns(text: &str, pack: &LanguagePack) -> Vec<Pronoun> {
    let pronouns = pack.pronouns;
    let mut found = Vec::new();

    // Walk whole words in the source text. Searching a lowercased copy yields
    // offsets that do not address `text` once a character's lowercase form has
    // a different byte length, and slicing `text` with one panics.
    let mut word_start: Option<usize> = None;
    let sentinel = std::iter::once((text.len(), ' '));
    for (byte_pos, c) in text.char_indices().chain(sentinel) {
        if c.is_alphabetic() {
            word_start.get_or_insert(byte_pos);
            continue;
        }
        if let Some(start) = word_start.take() {
            let word = &text[start..byte_pos];
            if let Some(pronoun) = pronouns.iter().find(|p| word.eq_ignore_ascii_case(p)) {
                found.push(Pronoun {
                    text: (*pronoun).to_string(),
                    start,
                    end: byte_pos,
                });
            }
        }
    }

    found
}

/// Resolve every pronoun in the sentence, in text order.
///
/// A pronoun with a mention before it in its own sentence takes that one, as
/// it always has. A pronoun with none — one opening a sentence, which is where
/// narrative prose puts them — falls back to a referent the previous sentence
/// named, and `runs(elena, archive)` in "Elena grew up in the Archive. She runs
/// it now." is invisible without that fallback.
///
/// IMPORTANT: two assumptions carry the whole fallback, and neither is free.
///
/// - Carried referents are consumed in the order the previous sentence named
///   them, so the first unresolved pronoun takes its topic. That is a
///   different rule from the within-sentence one: "nearest preceding" is about
///   syntactic proximity, and across a boundary the relevant relation is topic
///   continuity, whose best single guess is the previous sentence's subject.
/// - Two distinct pronouns in one sentence never take the same referent. "She
///   runs it" cannot mean Elena runs Elena, so a carried referent is consumed
///   once. This is the only thing standing in for agreement, which the
///   pipeline does not have: with no gender or number check, nothing else
///   keeps "it" off the person "she" just named.
///
/// The scope ends at the previous sentence and at a paragraph break, which
/// `collect_candidates` enforces by clearing the carry. One sentence is the
/// smallest window that fixes the case above, and every additional sentence
/// multiplies the error rate of a rule with no agreement behind it.
fn resolve_pronouns(
    text: &str,
    pack: &LanguagePack,
    lexicon: &[EntityCandidate],
    carried: &[String],
    register: &Register,
) -> Vec<EntityCandidate> {
    let mut resolved = Vec::new();
    let mut used: Vec<&str> = Vec::new();

    // A lexicon mention covering the pronoun's own offsets replaces it: the
    // caller named that referent outright, which beats resolving one.
    for pronoun in extract_pronouns(text, pack) {
        let covered = lexicon
            .iter()
            .any(|m| pronoun.start < m.end && m.start < pronoun.end);
        if covered {
            continue;
        }

        let antecedent = match find_pronoun_antecedent(text, &pronoun, lexicon, pack) {
            Some(antecedent) => antecedent,
            None => {
                // The first referent the previous sentence named that agrees
                // with this pronoun and has not already been taken by one.
                let Some(candidate) = carried.iter().find(|name| {
                    !used.contains(&name.as_str()) && register.allows(&pronoun.text, name, pack)
                }) else {
                    continue;
                };
                used.push(candidate.as_str());
                candidate.clone()
            }
        };

        resolved.push(EntityCandidate {
            text: pronoun.text.clone(),
            normalized: antecedent,
            start: pronoun.start,
            end: pronoun.end,
        });
    }

    resolved
}

/// The referents this sentence names outright, in order and without repeats,
/// for the next sentence's pronouns to fall back on.
///
/// IMPORTANT: a pronoun's own mention is not carried. It is a referent resolved
/// by guess, and carrying it would let one wrong guess seed the next sentence's.
/// IMPORTANT: a possessor is not carried either. "When Blanche's husband
/// offered him work" names Blanche, but the sentence is about the husband and
/// the one after it is about neither; a possessor sits low enough in the
/// salience order that carrying it hands the next sentence's pronoun the wrong
/// referent with nothing else on offer. That is the one link this whole
/// mechanism made over three novels before the rule existed, and it was wrong.
pub fn carried_referents(
    sentence: &str,
    entities: &[EntityCandidate],
    pack: &LanguagePack,
) -> Vec<String> {
    let mut carried: Vec<String> = Vec::new();

    for entity in entities {
        let is_pronoun = pack
            .pronouns
            .iter()
            .any(|p| entity.text.eq_ignore_ascii_case(p));
        let is_possessor = sentence[entity.end..]
            .trim_start_matches(['\u{2019}', '\''])
            .starts_with(['s', 'S'])
            && sentence[entity.end..].starts_with(['\u{2019}', '\'']);

        if is_pronoun || is_possessor || carried.contains(&entity.normalized) {
            continue;
        }
        carried.push(entity.normalized.clone());
    }

    carried
}

/// The referent of a pronoun: the nearest mention before it.
///
/// IMPORTANT: a lexicon mention counts here exactly as a capitalized word does,
/// because the caller asserted it names a person: it takes the pronoun where it
/// sits nearer than the capitalized word the backward walk finds, and where the
/// walk finds no capitalized word at all. With no lexicon mention in front of
/// the pronoun this is the backward walk over raw text it has always been, so a
/// caller supplying no lowercase key resolves as before.
fn find_pronoun_antecedent(
    text: &str,
    pronoun: &Pronoun,
    lexicon: &[EntityCandidate],
    pack: &LanguagePack,
) -> Option<String> {
    let nearest_lexicon = lexicon
        .iter()
        .filter(|m| m.end <= pronoun.start)
        .max_by_key(|m| m.end);

    // Simple heuristic: find the most recent capitalized entity before this pronoun
    let before_pronoun = &text[..pronoun.start];

    for (offset, word) in words_with_offsets(before_pronoun).into_iter().rev() {
        // IMPORTANT: strip the possessive clitic and surrounding punctuation.
        // `extract_capitalized_entities` treats them as separators, so leaving
        // them here yields a second, misspelled referent ("marco's") that
        // never unifies with the mention it came from.
        let word = word.split(['\'', '\u{2019}']).next().unwrap_or(word);
        let word = word.trim_matches(|c: char| !c.is_alphanumeric());

        // IMPORTANT: a capitalized pronoun is not an antecedent. "She runs it"
        // would otherwise resolve "it" to the word "She" and put a graph node
        // named `she` in the output, which names nobody — and it would hide
        // the pronoun that has no antecedent here from the cross-sentence
        // fallback, which is the only thing that can resolve it.
        let is_pronoun = pack.pronouns.iter().any(|p| word.eq_ignore_ascii_case(p));

        if !is_pronoun && word.chars().next().is_some_and(|c| c.is_uppercase()) && word.len() > 1 {
            return match nearest_lexicon {
                Some(m) if m.end > offset => Some(m.normalized.clone()),
                _ => Some(normalize_entity(word)),
            };
        }
    }

    nearest_lexicon.map(|m| m.normalized.clone())
}

/// Whitespace-separated words paired with their byte offset in `text`.
fn words_with_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut words = Vec::new();
    let mut word_start: Option<usize> = None;
    let sentinel = std::iter::once((text.len(), ' '));

    for (byte_pos, c) in text.char_indices().chain(sentinel) {
        if !c.is_whitespace() {
            word_start.get_or_insert(byte_pos);
            continue;
        }
        if let Some(start) = word_start.take() {
            words.push((start, &text[start..byte_pos]));
        }
    }

    words
}

fn normalize_entity(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}
