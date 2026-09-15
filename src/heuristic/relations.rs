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
            if let Some((rel, rule, gap)) = find_relation_pattern(text, subj, obj) {
                let normalized_rel = normalize_relation(&rel, ontology);
                relations.push(RelationCandidate {
                    subject: subj.normalized.clone(),
                    relation: normalized_rel,
                    object: obj.normalized.clone(),
                    rule,
                    span: [subj.start, obj.end],
                    gap,
                });
            }
        }
    }

    relations
}

fn find_relation_pattern(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
) -> Option<(String, String, usize)> {
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
        if after_obj.contains("sister") {
            return Some((
                "sister_of".to_string(),
                "possessive-sister-pattern".to_string(),
                gap,
            ));
        }
        if after_obj.contains("brother") {
            return Some((
                "brother_of".to_string(),
                "possessive-brother-pattern".to_string(),
                gap,
            ));
        }
    }

    // Verb patterns with space before to ensure word boundaries
    if between.contains(" mentor") {
        return Some((
            "mentors".to_string(),
            "verb-mentor-pattern".to_string(),
            gap,
        ));
    }
    if between.contains(" work") && between.contains(" at") {
        return Some((
            "works_at".to_string(),
            "verb-works-at-pattern".to_string(),
            gap,
        ));
    }
    if between.contains(", who ") && (between.contains("work") || between.contains("mentor")) {
        if between.contains("work") && between.contains("at") {
            return Some((
                "works_at".to_string(),
                "relative-works-at-pattern".to_string(),
                gap,
            ));
        }
        if between.contains("mentor") {
            return Some((
                "mentors".to_string(),
                "relative-mentor-pattern".to_string(),
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
