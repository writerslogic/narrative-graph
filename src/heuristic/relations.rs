use super::cooccurrence::base;
use super::entities::EntityCandidate;
use std::collections::BTreeMap;

/// A relation found between two entities, with the span it was found in and
/// the character gap between the entities (used to weight confidence).
pub struct RelationCandidate {
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub rule: String,
    pub span: [usize; 2],
    pub base: f32,
    pub gap: usize,
}

/// Extract relations between entity pairs in the same sentence/clause.
pub fn extract_relations(
    text: &str,
    entities: &[EntityCandidate],
    ontology: &BTreeMap<String, String>,
) -> Vec<RelationCandidate> {
    let mut relations = Vec::new();

    // Only check forward entity pairs (left-to-right in text)
    // to avoid bidirectional extraction
    for i in 0..entities.len() {
        for j in (i + 1)..entities.len() {
            let subj = &entities[i];
            let obj = &entities[j];

            // Check for verb-phrase patterns between these entities
            // Subject comes before object in text
            // A pronoun carries its antecedent's normalized name, so the same
            // referent can appear twice in the pair loop.
            if subj.normalized == obj.normalized {
                continue;
            }

            if let Some(m) = find_relation_pattern(text, subj, obj) {
                let (s, o) = if m.swapped { (obj, subj) } else { (subj, obj) };
                relations.push(RelationCandidate {
                    subject: s.normalized.clone(),
                    relation: normalize_relation(&m.relation, ontology),
                    object: o.normalized.clone(),
                    rule: m.rule,
                    span: [subj.start, m.span_end.unwrap_or(obj.end)],
                    base: m.base,
                    gap: m.gap,
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
}

impl PatternMatch {
    fn new(relation: &str, rule: &str, base: f32, gap: usize) -> Self {
        Self {
            relation: relation.to_string(),
            rule: rule.to_string(),
            base,
            gap,
            swapped: false,
            span_end: None,
        }
    }

    fn swapped(self, swapped: bool) -> Self {
        Self { swapped, ..self }
    }

    fn span_end(self, span_end: usize) -> Self {
        Self {
            span_end: Some(span_end),
            ..self
        }
    }
}

/// Relational nouns recognized in a possessive, as (noun, relation, base
/// confidence). Every entry reads "X is Y's <noun>", giving relation(X, Y).
///
/// The relation is named explicitly rather than derived from the noun so a
/// noun can share a label with a verb rule: "Elena is Marco's mentor" and
/// "Elena mentors Marco" are the same fact and must not produce two edge
/// types. The rule name stays `possessive-<noun>-pattern`, so attribution
/// remains per-noun. Matching is whole-word, so entries are order-independent.
const POSSESSIVE_NOUNS: &[(&str, &str, f32)] = &[
    // Kinship and role: the possessive states the relation outright.
    ("sister", "sister_of", base::POSSESSIVE_FACTUAL),
    ("brother", "brother_of", base::POSSESSIVE_FACTUAL),
    ("mother", "mother_of", base::POSSESSIVE_FACTUAL),
    ("father", "father_of", base::POSSESSIVE_FACTUAL),
    ("grandmother", "grandmother_of", base::POSSESSIVE_FACTUAL),
    ("grandfather", "grandfather_of", base::POSSESSIVE_FACTUAL),
    ("daughter", "daughter_of", base::POSSESSIVE_FACTUAL),
    ("son", "son_of", base::POSSESSIVE_FACTUAL),
    ("wife", "wife_of", base::POSSESSIVE_FACTUAL),
    ("husband", "husband_of", base::POSSESSIVE_FACTUAL),
    ("cousin", "cousin_of", base::POSSESSIVE_FACTUAL),
    ("aunt", "aunt_of", base::POSSESSIVE_FACTUAL),
    ("uncle", "uncle_of", base::POSSESSIVE_FACTUAL),
    ("niece", "niece_of", base::POSSESSIVE_FACTUAL),
    ("nephew", "nephew_of", base::POSSESSIVE_FACTUAL),
    ("widow", "widow_of", base::POSSESSIVE_FACTUAL),
    ("guardian", "guardian_of", base::POSSESSIVE_FACTUAL),
    ("employer", "employer_of", base::POSSESSIVE_FACTUAL),
    ("servant", "servant_of", base::POSSESSIVE_FACTUAL),
    ("master", "master_of", base::POSSESSIVE_FACTUAL),
    ("teacher", "teacher_of", base::POSSESSIVE_FACTUAL),
    ("student", "student_of", base::POSSESSIVE_FACTUAL),
    ("pupil", "pupil_of", base::POSSESSIVE_FACTUAL),
    ("apprentice", "apprentice_of", base::POSSESSIVE_FACTUAL),
    // Shares its label with verb-mentor-pattern: the same fact, said two ways.
    ("mentor", "mentors", base::POSSESSIVE_FACTUAL),
    // Social stance: same shape, weaker claim.
    ("friend", "friend_of", base::POSSESSIVE_STANCE),
    ("enemy", "enemy_of", base::POSSESSIVE_STANCE),
    ("rival", "rival_of", base::POSSESSIVE_STANCE),
    ("lover", "lover_of", base::POSSESSIVE_STANCE),
    ("companion", "companion_of", base::POSSESSIVE_STANCE),
    ("ally", "ally_of", base::POSSESSIVE_STANCE),
    ("acquaintance", "acquaintance_of", base::POSSESSIVE_STANCE),
];

/// Links that let the possessive be read as a statement about the subject.
///
/// IMPORTANT: matched against the whole trimmed text between the two mentions,
/// never as a substring. "Elena visited Marco's sister" names a third person,
/// and "is not", "was never", "could be" and "believed ... was" each deny,
/// hedge or attribute the claim rather than making it. Every one of those
/// contains a copula; none of them is one.
const POSSESSIVE_COPULAS: &[&str] = &["is", "was", "are", "were"];

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
fn possessed_noun_phrase_len(rest: &str) -> usize {
    let punct = rest
        .find([',', '.', ';', ':', '!', '?'])
        .unwrap_or(rest.len());
    let conjunction = find_ascii_ci(rest, " and ")
        .map(|(start, _)| start)
        .unwrap_or(rest.len());
    punct.min(conjunction)
}

/// Closed-class words that deny the event or hold it open. IMPORTANT: this is
/// a closed class on purpose. The verbs that suspend a complement (refuse,
/// hope, intend, pretend) are an open one, and are caught structurally by the
/// `to`-infinitive instead. "will" is absent because a future tense asserts.
const SUSPENDING_WORDS: &[&str] = &[
    "not", "never", "no", "nor", "neither", "if", "unless", "whether", "could", "would", "might",
    "may", "should",
];

/// Whether the text between two mentions denies the relation or holds it open
/// rather than asserting it.
fn suspends_assertion(between: &str) -> bool {
    between.split(|c: char| !c.is_ascii_alphanumeric() && c != '\'').any(|word| {
        // "didn't", "doesn't", "isn't": the negator is a suffix, not a word.
        word.ends_with("n't") || SUSPENDING_WORDS.contains(&word)
    })
        // A complement verb suspends its infinitive: "refused to mentor",
        // "hoped to mentor", "wanted to work" assert nothing about the event.
        || between.split_whitespace().any(|word| word == "to")
}

/// Whether the sentence holding the mentions is a question. IMPORTANT: "Did
/// Elena mentor Marco?" asks whether the relation holds; it does not state it.
/// The interrogative auxiliary is not between the mentions, so only the
/// terminator can tell. An abbreviation's period ends the scan early, which
/// errs toward reading the sentence as a statement.
fn ends_in_question(text: &str, from: usize) -> bool {
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
fn heads_possessed_phrase(phrase: &str, start: usize, end: usize) -> bool {
    phrase[end..].trim().is_empty() && find_ascii_ci(&phrase[..start], "'s").is_none()
}

fn find_relation_pattern(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
) -> Option<PatternMatch> {
    // IMPORTANT: use each mention's own offsets. Re-locating a surface form by
    // substring search finds the first occurrence anywhere in the text, and an
    // offset into a lowercased copy does not address `text`.
    // Mentions can nest ("Jane" inside "Mary Jane"), so this is also the bound
    // that keeps `obj.start - subj.end` from underflowing.
    if subj.end > obj.start {
        return None;
    }

    let (start, end) = (subj.end, obj.start);
    let gap = end - start;

    // Only process if entities are within reasonable distance (30 chars for direct relations)
    if gap > 30 {
        return None;
    }

    let between = &text[start..end].to_lowercase();

    // IMPORTANT: every rule below reads the text between the mentions as an
    // assertion that the relation holds. Text that denies or suspends it is
    // not a weaker assertion, it is the opposite one, so no rule may fire.
    if suspends_assertion(between) || ends_in_question(text, obj.end) {
        return None;
    }

    // Possessive pattern: "x is y's [relation]". Matched against the original
    // text so the noun's offsets can bound the evidence span.
    let after_obj = &text[obj.end..];
    let bytes = after_obj.as_bytes();
    let possessive = bytes.len() >= 3
        && bytes[0] == b'\''
        && bytes[1].eq_ignore_ascii_case(&b's')
        && matches!(bytes[2], b' ' | b'.');

    if possessive && POSSESSIVE_COPULAS.contains(&between.trim()) {
        let rest = &after_obj["'s".len()..];
        let phrase = &rest[..possessed_noun_phrase_len(rest)];
        let phrase_start = obj.end + "'s".len();

        for (noun, relation, base) in POSSESSIVE_NOUNS {
            let Some((noun_start, noun_end)) = find_ascii_ci_word(phrase, noun) else {
                continue;
            };
            if !heads_possessed_phrase(phrase, noun_start, noun_end) {
                continue;
            }
            return Some(
                PatternMatch::new(relation, &format!("possessive-{noun}-pattern"), *base, gap)
                    .span_end(phrase_start + noun_end),
            );
        }
    }

    // Verb patterns with space before to ensure word boundaries.
    // IMPORTANT: passive voice ("was mentored by") names the mentor second, so
    // the relation's subject is the later entity, not the earlier one.
    if between.contains(" mentor") {
        let passive = between.contains(" by");
        return Some(
            PatternMatch::new("mentors", "verb-mentor-pattern", base::VERB, gap).swapped(passive),
        );
    }
    if between.contains(" work") && between.contains(" at") {
        return Some(PatternMatch::new(
            "works_at",
            "verb-works-at-pattern",
            base::VERB,
            gap,
        ));
    }
    if between.contains(", who ") && (between.contains("work") || between.contains("mentor")) {
        if between.contains("work") && between.contains("at") {
            return Some(PatternMatch::new(
                "works_at",
                "relative-works-at-pattern",
                base::RELATIVE_CLAUSE,
                gap,
            ));
        }
        if between.contains("mentor") {
            return Some(PatternMatch::new(
                "mentors",
                "relative-mentor-pattern",
                base::RELATIVE_CLAUSE,
                gap,
            ));
        }
    }

    None
}

/// Normalize a relation type against an optional caller-supplied ontology.
/// If the relation is in the ontology map, return the mapped value; otherwise return as-is.
pub fn normalize_relation(rel: &str, ontology: &BTreeMap<String, String>) -> String {
    let rel_lower = rel.to_lowercase();

    if let Some(canonical) = ontology.get(&rel_lower) {
        canonical.clone()
    } else {
        rel.to_string()
    }
}
