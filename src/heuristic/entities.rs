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

    // Deduplicate by keeping first occurrence, preserving text order
    let mut seen = std::collections::HashSet::new();
    entities.retain(|e| seen.insert(e.normalized.clone()));
    entities
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
                    .is_some_and(|c| c.is_uppercase());

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
            .is_some_and(|c| c.is_uppercase());
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
