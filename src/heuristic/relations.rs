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
            if let Some(m) = find_relation_pattern(text, subj, obj) {
                let (s, o) = if m.swapped { (obj, subj) } else { (subj, obj) };
                relations.push(RelationCandidate {
                    subject: s.normalized.clone(),
                    relation: normalize_relation(&m.relation, ontology),
                    object: o.normalized.clone(),
                    rule: m.rule,
                    span: [subj.start, obj.end],
                    gap: m.gap,
                });
            }
        }
    }

    relations
}

/// A pattern hit. `swapped` means the phrasing puts the relation's subject
/// second in the text, as passive voice does.
struct PatternMatch {
    relation: String,
    rule: String,
    gap: usize,
    swapped: bool,
}

impl PatternMatch {
    fn new(relation: &str, rule: &str, gap: usize) -> Self {
        Self {
            relation: relation.to_string(),
            rule: rule.to_string(),
            gap,
            swapped: false,
        }
    }

    fn swapped(self, swapped: bool) -> Self {
        Self { swapped, ..self }
    }
}

/// The possessed noun phrase introduced by `'s`, cut at the first clause
/// boundary. IMPORTANT: the phrase bounds the relational-noun search. Scanning
/// the whole remainder of the sentence matches a noun belonging to a later
/// clause ("Marco's dog, and she has a sister") and emits the highest
/// confidence rule in the system on it.
fn possessed_noun_phrase(after_obj: &str) -> &str {
    let rest = &after_obj["'s".len()..];
    let punct = rest
        .find([',', '.', ';', ':', '!', '?'])
        .unwrap_or(rest.len());
    let conjunction = rest.find(" and ").unwrap_or(rest.len());
    &rest[..punct.min(conjunction)]
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

    // Also check what comes after the object (for possessive patterns like "x is y's sister")
    // Bound to a named value rather than borrowing a temporary out of the
    // `if`: temporary lifetime extension there is not accepted on the MSRV.
    let after_obj_owned = text[obj.end..].to_lowercase();
    let after_obj = after_obj_owned.as_str();

    // Possessive pattern: "x is y's [relation]"
    if after_obj.starts_with("'s ") || after_obj.starts_with("'s.") {
        let possessed = possessed_noun_phrase(after_obj);
        if possessed.contains("sister") {
            return Some(PatternMatch::new(
                "sister_of",
                "possessive-sister-pattern",
                gap,
            ));
        }
        if possessed.contains("brother") {
            return Some(PatternMatch::new(
                "brother_of",
                "possessive-brother-pattern",
                gap,
            ));
        }
    }

    // Verb patterns with space before to ensure word boundaries.
    // IMPORTANT: passive voice ("was mentored by") names the mentor second, so
    // the relation's subject is the later entity, not the earlier one.
    if between.contains(" mentor") {
        let passive = between.contains(" by");
        return Some(PatternMatch::new("mentors", "verb-mentor-pattern", gap).swapped(passive));
    }
    if between.contains(" work") && between.contains(" at") {
        return Some(PatternMatch::new("works_at", "verb-works-at-pattern", gap));
    }
    if between.contains(", who ") && (between.contains("work") || between.contains("mentor")) {
        if between.contains("work") && between.contains("at") {
            return Some(PatternMatch::new(
                "works_at",
                "relative-works-at-pattern",
                gap,
            ));
        }
        if between.contains("mentor") {
            return Some(PatternMatch::new("mentors", "relative-mentor-pattern", gap));
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
