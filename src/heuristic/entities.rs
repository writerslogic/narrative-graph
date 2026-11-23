use super::pack::LanguagePack;
use super::quotes::QuoteMap;
use std::collections::BTreeMap;

/// Entity candidate with start and end positions in the text.
#[derive(Debug, Clone)]
pub struct EntityCandidate {
    pub text: String,
    pub normalized: String,
    pub start: usize,
    pub end: usize,
    /// The mention is a pronoun (or the narrator's first person) resolved to a
    /// referent, not a name the text spells out.
    pub pronoun: bool,
    /// The mention sits inside quoted speech.
    pub quoted: bool,
}

/// Extract entities from a sentence with the pipeline's own language pack and
/// no context from any preceding sentence.
pub fn extract_entities(text: &str, aliases: &BTreeMap<String, String>) -> Vec<EntityCandidate> {
    let pack = &super::pack::ENGLISH;
    let quotes = QuoteMap::default();
    let mut entities = named_mentions(text, aliases, pack, &quotes);
    entities.extend(resolve_pronouns(text, pack, &entities));
    entities.sort_by_key(|m| m.start);
    entities
}

/// The capitalized runs of one sentence, for the register pre-pass. Aliases and
/// pronouns are deliberately absent: the register records what the text says
/// about a name, before anything is resolved or remapped.
pub fn mentions_for_register(text: &str, pack: &LanguagePack) -> Vec<EntityCandidate> {
    extract_capitalized_entities(text, pack)
}

pub(super) fn named_mentions(
    text: &str,
    aliases: &BTreeMap<String, String>,
    pack: &LanguagePack,
    quotes: &QuoteMap,
) -> Vec<EntityCandidate> {
    let mut entities = extract_capitalized_entities(text, pack);
    for entity in &mut entities {
        if let Some(canonical) = aliases.get(&entity.text) {
            entity.normalized = canonical.clone();
        }
    }

    // IMPORTANT: after the exact-match loop, never before. A lexicon mention
    // carries its canonical name already, and the loop above keys on the
    // surface form, which would overwrite it wherever the two collide.
    entities.extend(lexicon_mentions(text, aliases, &entities));
    entities.sort_by_key(|e| e.start);
    for entity in &mut entities {
        entity.quoted = quotes.inside(entity.start);
    }

    // Every mention is kept: collapsing repeats of one referent would hide the
    // second relation in "Elena is Marco's sister and Marco mentors Elena."
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
        // Keep at least one name word after the title: a bare given name or
        // surname is ambiguous without it. A particle is not a name word.
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
        // A quotation opening after an attribution comma also starts a sentence:
        // `Elena said, "She is Marco's student."`
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
    // The openers are ASCII, so an ASCII-case-insensitive compare matches
    // what lowercasing the word would, without allocating.
    pack.sentence_openers
        .iter()
        .any(|opener| word.eq_ignore_ascii_case(opener))
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
    // A pronoun is never a proper name, even capitalized mid-sentence. First-person
    // "I" would otherwise become a node for whoever is narrating.
    let is_pronoun = |list: &[&str]| list.iter().any(|p| run.eq_ignore_ascii_case(p));
    if is_pronoun(pack.pronouns) || is_pronoun(pack.deictic_pronouns) {
        return;
    }

    entities.push(EntityCandidate {
        text: run.to_string(),
        normalized: normalize_entity(strip_honorifics(run, pack)),
        start,
        end,
        pronoun: false,
        quoted: false,
    });
}

/// Whether `c` ends a word. A period does but is not a hard break: "Mrs.
/// Bennet" and "J. R. R. Tolkien" are one run across it.
fn is_separator(c: char) -> bool {
    c == '.' || is_hard_break(c)
}

/// Whether `c` ends the name as well as the word. IMPORTANT: a comma between
/// two capitalized words does not join them. "Nora, Dev and Marco" is three
/// names, and "Dev, Marco's brother" is a name, a comma and a possessor, not a
/// person called `dev_marco`; a run that survives a comma merges the first
/// name of every list in fiction into the second.
/// An apostrophe between a letter and a capital, as in "O’Leary" or
/// "D'Arcy", where it is part of the name rather than a possessive.
fn joins_name(text: &str, at: usize, c: char) -> bool {
    matches!(c, '\'' | '\u{2019}')
        && text[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphabetic)
        && text[at + c.len_utf8()..]
            .chars()
            .next()
            .is_some_and(char::is_uppercase)
}

fn is_hard_break(c: char) -> bool {
    matches!(
        c,
        ',' | '!'
            | '?'
            | ';'
            | ':'
            | '\u{2014}'
            | '\''
            | '\u{2019}'
            | '\u{2018}'
            | '"'
            | '\u{201C}'
            | '\u{201D}'
            | '('
            | ')'
            | '['
            | ']'
    )
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
        // The typographic apostrophe separates like the straight one; Gutenberg texts
        // use it throughout, so "Blanche’s" must not normalize to `blanche’s`.
        // Between a letter and a capital it is inside the name: "O’Leary".
        let is_sep = c.is_whitespace() || (is_separator(c) && !joins_name(text, byte_pos, c));

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
            if is_hard_break(c) && !current_entity.is_empty() {
                push_entity(&mut entities, &current_entity, start_idx, entity_end, pack);
                current_entity.clear();
                pending_particles.clear();
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
                pronoun: false,
                quoted: false,
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
pub(super) struct Pronoun {
    pub text: &'static str,
    pub start: usize,
    pub end: usize,
}

impl Pronoun {
    pub fn mention(&self, name: &str) -> EntityCandidate {
        EntityCandidate {
            text: self.text.to_string(),
            normalized: name.to_string(),
            start: self.start,
            end: self.end,
            pronoun: true,
            quoted: false,
        }
    }
}

pub(super) fn extract_pronouns(text: &str, pack: &LanguagePack) -> Vec<Pronoun> {
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
            if let Some(pronoun) = pack.pronouns.iter().find(|p| word.eq_ignore_ascii_case(p)) {
                found.push(Pronoun {
                    text: pronoun,
                    start,
                    end: byte_pos,
                });
            }
        }
    }

    found
}

fn resolve_pronouns(
    text: &str,
    pack: &LanguagePack,
    mentions: &[EntityCandidate],
) -> Vec<EntityCandidate> {
    extract_pronouns(text, pack)
        .into_iter()
        .filter_map(|pronoun| {
            if mentions
                .iter()
                .any(|m| pronoun.start < m.end && m.start < pronoun.end)
            {
                return None;
            }
            let name = mentions.iter().rev().find(|m| m.end <= pronoun.start)?;
            Some(pronoun.mention(&name.normalized))
        })
        .collect()
}

/// The first person as a mention of the narrator, outside speech only: inside
/// a quotation "I" is whoever is speaking, which this pass cannot know.
pub(super) fn first_person_mentions(
    text: &str,
    pack: &LanguagePack,
    narrator: &str,
    quotes: &QuoteMap,
) -> Vec<EntityCandidate> {
    let mut found = Vec::new();
    let mut word_start: Option<usize> = None;
    let sentinel = std::iter::once((text.len(), ' '));
    for (byte_pos, c) in text.char_indices().chain(sentinel) {
        if c.is_alphabetic() {
            word_start.get_or_insert(byte_pos);
            continue;
        }
        let Some(start) = word_start.take() else {
            continue;
        };
        let word = &text[start..byte_pos];
        // "I'm" and "I'll" read as "I" followed by a clitic; a word that
        // continues past an apostrophe into letters is not a lone "i" only when
        // it is something else ("ill" never reaches here: the walk is by
        // alphabetic run).
        if pack
            .first_person_pronouns
            .iter()
            .any(|p| word.eq_ignore_ascii_case(p))
            && !quotes.inside(start)
        {
            found.push(EntityCandidate {
                text: word.to_string(),
                normalized: narrator.to_string(),
                start,
                end: byte_pos,
                pronoun: true,
                quoted: false,
            });
        }
    }
    found
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
pub(super) fn carried_referents(
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
        let is_possessor = super::relations::opens_possessive(sentence, entity.end, pack);

        if is_pronoun
            || entity.pronoun
            || entity.quoted
            || is_possessor
            || carried.contains(&entity.normalized)
        {
            continue;
        }
        carried.push(entity.normalized.clone());
    }

    carried
}

fn normalize_entity(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}
