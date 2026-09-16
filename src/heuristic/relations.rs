use super::entities::EntityCandidate;
use super::pack::{LanguagePack, VerbRule};
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
    pack: &LanguagePack,
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

            if let Some(m) = find_relation_pattern(text, subj, obj, pack) {
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

/// Whether the text between two mentions denies the relation or holds it open
/// rather than asserting it.
fn suspends_assertion(between: &str, pack: &LanguagePack) -> bool {
    between.split(|c: char| !c.is_ascii_alphanumeric() && c != '\'').any(|word| {
        // "didn't", "doesn't", "isn't": the negator is a suffix, not a word.
        word.ends_with(pack.negation_clitic) || pack.suspending_words.contains(&word)
    })
        // A complement verb suspends its infinitive: "refused to mentor",
        // "hoped to mentor", "wanted to work" assert nothing about the event.
        || between
            .split_whitespace()
            .any(|word| word == pack.infinitive_marker)
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
fn heads_possessed_phrase(phrase: &str, start: usize, end: usize, pack: &LanguagePack) -> bool {
    phrase[end..].trim().is_empty()
        && find_ascii_ci(&phrase[..start], pack.possessive_clitic).is_none()
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

    let mut words: Vec<&str> = rest.split_whitespace().collect();
    if words.pop()? != pack.genitive_link {
        return None;
    }
    let noun = words.pop()?;
    words
        .iter()
        .all(|word| pack.appositive_modifiers.contains(word))
        .then_some(noun)
}

/// Whether an apostrophe-s opens at `from`, marking the mention before it as a
/// possessor.
fn opens_possessive(text: &str, from: usize, pack: &LanguagePack) -> bool {
    starts_with_clitic(&text.as_bytes()[from..], pack)
}

/// Whether `bytes` opens with the pack's possessive clitic, matched
/// case-insensitively as the rest of the relation rules match.
fn starts_with_clitic(bytes: &[u8], pack: &LanguagePack) -> bool {
    let clitic = pack.possessive_clitic.as_bytes();
    bytes
        .get(..clitic.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(clitic))
}

fn find_relation_pattern(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
    pack: &LanguagePack,
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
    if suspends_assertion(between, pack) || ends_in_question(text, obj.end) {
        return None;
    }

    // Possessive pattern: "x is y's [relation]". Matched against the original
    // text so the noun's offsets can bound the evidence span.
    let after_obj = &text[obj.end..];
    let clitic_len = pack.possessive_clitic.len();
    let bytes = after_obj.as_bytes();
    let possessive =
        starts_with_clitic(bytes, pack) && matches!(bytes.get(clitic_len), Some(b' ') | Some(b'.'));

    let asserts_possessive =
        pack.possessive_copulas.contains(&between.trim()) || is_appositive_link(between, pack);

    if possessive && asserts_possessive {
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
                PatternMatch::new(relation, &format!("possessive-{noun}-pattern"), *base, gap)
                    .span_end(phrase_start + noun_end),
            );
        }
    }

    // "of" genitive: "x, the [relation] of y". IMPORTANT: same guard as the
    // reversed possessive — "Elena, the sister of Marco's wife" names the wife,
    // not Marco.
    if let Some(noun) = of_genitive_noun(between, pack) {
        if !opens_possessive(text, obj.end, pack) {
            if let Some((_, relation, base)) =
                pack.possessive_nouns.iter().find(|(n, _, _)| *n == noun)
            {
                return Some(PatternMatch::new(
                    relation,
                    &format!("of-genitive-{noun}-pattern"),
                    *base,
                    gap,
                ));
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
                    PatternMatch::new(relation, &format!("possessive-{noun}-pattern"), *base, gap)
                        .swapped(true),
                );
            }
        }
    }

    // Verb patterns. The markers carry their own word boundary, so the table is
    // walked in order and the first rule whose tokens are all present wins.
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
    let rel_lower = rel.to_lowercase();

    if let Some(canonical) = ontology.get(&rel_lower) {
        canonical.clone()
    } else {
        rel.to_string()
    }
}
