use std::collections::BTreeMap;

/// Entity candidate with start and end positions in the text.
#[derive(Debug, Clone)]
pub struct EntityCandidate {
    pub text: String,
    pub normalized: String,
    pub start: usize,
    pub end: usize,
}

/// Extract entities from a sentence using capitalization + pronoun linking + alias resolution.
pub fn extract_entities(text: &str, aliases: &BTreeMap<String, String>) -> Vec<EntityCandidate> {
    let mut entities = Vec::new();

    // Find capitalized sequences (multi-word proper nouns and single capitalized words)
    let entities_from_capitalization = extract_capitalized_entities(text);
    entities.extend(entities_from_capitalization);

    // Find pronouns that can be linked to previously mentioned entities in the sentence
    let pronouns = extract_pronouns(text);
    for pronoun in pronouns {
        if let Some(antecedent) = find_pronoun_antecedent(text, &pronoun) {
            entities.push(EntityCandidate {
                text: pronoun.text.clone(),
                normalized: antecedent,
                start: pronoun.start,
                end: pronoun.end,
            });
        }
    }

    // Resolve aliases: any mention in the aliases map becomes the mapped canonical name
    for entity in &mut entities {
        if let Some(canonical) = aliases.get(&entity.text) {
            entity.normalized = canonical.clone();
        }
    }

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

/// Words that are capitalized at the start of a sentence by position alone.
///
/// IMPORTANT: a capitalized run becomes one mention, so "But Elena" normalizes
/// to `but_elena` and never unifies with `elena` elsewhere in the text. That
/// silently splits a character into two graph nodes, and narrative prose opens
/// sentences this way constantly.
///
/// Membership is restricted to words that cannot also be a given name in that
/// position: "May", "Will", "Grace", "June", "Faith" and "Summer" are all
/// names and are deliberately absent, because dropping a real mention costs
/// more than keeping a malformed one.
///
/// The pronouns `extract_pronouns` owns are listed here too. A capitalized
/// pronoun otherwise becomes a second mention at the same offsets as the
/// resolved one, and `she` as a graph node names nobody.
#[rustfmt::skip]
const SENTENCE_OPENERS: &[&str] = &[
    "a", "after", "again", "all", "although", "an", "and", "another", "any", "are", "as", "at",
    "because", "before", "both", "but", "by", "did", "do", "does", "each", "either", "even",
    "every", "for", "from", "had", "has", "have", "he", "her", "here", "him", "his", "how",
    "however", "if",
    "in", "indeed", "instead", "is", "its", "just", "later", "maybe", "meanwhile", "my", "neither",
    "never", "no", "nor", "not", "now", "of", "often", "on", "once", "only", "or", "our",
    "perhaps", "since", "so", "some", "sometimes", "soon", "still", "suddenly", "that", "the",
    "she", "their", "them", "then", "there", "these", "they", "this", "those", "though", "to",
    "today", "tomorrow",
    "tonight", "was", "were", "what", "when", "where", "which", "while", "who", "whom", "whose",
    "why", "with", "yesterday", "yet", "your",
];

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
fn opens_sentence_by_position(text: &str, word: &str, word_start: usize) -> bool {
    SENTENCE_OPENERS.contains(&word.to_lowercase().as_str()) && is_sentence_start(text, word_start)
}

fn extract_capitalized_entities(text: &str) -> Vec<EntityCandidate> {
    let mut entities = Vec::new();
    let mut current_entity = String::new();
    let mut start_idx = 0;
    let mut current_word = String::new();
    let mut word_start = 0;
    // End of the last capitalized word folded into `current_entity`. The
    // separator position that flushes the entity sits after the following
    // lowercase word, so it does not bound the mention.
    let mut entity_end = 0;

    for (byte_pos, c) in text.char_indices() {
        let is_sep = c.is_whitespace() || ",.!?;:—'\"".contains(c);

        if is_sep {
            if !current_word.is_empty() {
                let is_capitalized = current_word
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase())
                    && !(current_entity.is_empty()
                        && opens_sentence_by_position(text, &current_word, word_start));

                if is_capitalized {
                    if !current_entity.is_empty() {
                        current_entity.push(' ');
                    } else {
                        start_idx = word_start;
                    }
                    current_entity.push_str(&current_word);
                    entity_end = byte_pos;
                } else {
                    if !current_entity.is_empty() {
                        entities.push(EntityCandidate {
                            text: current_entity.clone(),
                            normalized: normalize_entity(&current_entity),
                            start: start_idx,
                            end: entity_end,
                        });
                        current_entity.clear();
                    }
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
                && opens_sentence_by_position(text, &current_word, word_start));
        if is_capitalized {
            if !current_entity.is_empty() {
                current_entity.push(' ');
            } else {
                start_idx = word_start;
            }
            current_entity.push_str(&current_word);
            entity_end = text.len();
        } else if !current_entity.is_empty() {
            entities.push(EntityCandidate {
                text: current_entity.clone(),
                normalized: normalize_entity(&current_entity),
                start: start_idx,
                end: entity_end,
            });
            current_entity.clear();
        }
    }

    if !current_entity.is_empty() {
        entities.push(EntityCandidate {
            text: current_entity.clone(),
            normalized: normalize_entity(&current_entity),
            start: start_idx,
            end: entity_end,
        });
    }

    entities
}

#[derive(Debug)]
struct Pronoun {
    text: String,
    start: usize,
    end: usize,
}

fn extract_pronouns(text: &str) -> Vec<Pronoun> {
    let pronouns = [
        "he", "she", "they", "him", "her", "them", "his", "their", "it",
    ];
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

fn find_pronoun_antecedent(text: &str, pronoun: &Pronoun) -> Option<String> {
    // Simple heuristic: find the most recent capitalized entity before this pronoun
    let before_pronoun = &text[..pronoun.start];
    let words: Vec<&str> = before_pronoun.split_whitespace().collect();

    for word in words.iter().rev() {
        // IMPORTANT: strip the possessive clitic and surrounding punctuation.
        // `extract_capitalized_entities` treats them as separators, so leaving
        // them here yields a second, misspelled referent ("marco's") that
        // never unifies with the mention it came from.
        let word = word.split(['\'', '\u{2019}']).next().unwrap_or(word);
        let word = word.trim_matches(|c: char| !c.is_alphanumeric());

        if word.chars().next().is_some_and(|c| c.is_uppercase()) && word.len() > 1 {
            return Some(normalize_entity(word));
        }
    }

    None
}

fn normalize_entity(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}
