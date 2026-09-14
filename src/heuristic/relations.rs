use super::entities::EntityCandidate;
use std::collections::BTreeMap;

/// Extract relations between entity pairs in the same sentence/clause.
pub fn extract_relations(
    text: &str,
    entities: &[EntityCandidate],
    ontology: &BTreeMap<String, String>,
) -> Vec<(String, String, String, String)> {
    let mut relations = Vec::new();

    // Only check forward entity pairs (left-to-right in text)
    // to avoid bidirectional extraction
    for i in 0..entities.len() {
        for j in (i + 1)..entities.len() {
            let subj = &entities[i];
            let obj = &entities[j];

            // Check for verb-phrase patterns between these entities
            // Subject comes before object in text
            if let Some((rel, rule)) = find_relation_pattern(text, subj, obj) {
                let normalized_rel = normalize_relation(&rel, ontology);
                relations.push((
                    subj.normalized.clone(),
                    normalized_rel,
                    obj.normalized.clone(),
                    rule,
                ));
            }
        }
    }

    relations
}

fn find_relation_pattern(
    text: &str,
    subj: &EntityCandidate,
    obj: &EntityCandidate,
) -> Option<(String, String)> {
    let text_lower = text.to_lowercase();
    let subj_lower = subj.text.to_lowercase();
    let obj_lower = obj.text.to_lowercase();

    // Subject must come before object (forward direction only)
    if let (Some(subj_pos), Some(obj_pos)) =
        (text_lower.find(&subj_lower), text_lower.find(&obj_lower))
    {
        if subj_pos >= obj_pos {
            return None; // Object must come after subject
        }

        let start = subj_pos + subj_lower.len();
        let end = obj_pos;

        // Only process if entities are within reasonable distance
        if end - start > 60 || end <= start {
            return None;
        }

        let between = &text[start..end].to_lowercase();

        // Possessive pattern: "x's [relation] y"
        if between.contains("'s") {
            if between.contains("sister") {
                return Some((
                    "sister_of".to_string(),
                    "possessive-sister-pattern".to_string(),
                ));
            }
            if between.contains("brother") {
                return Some((
                    "brother_of".to_string(),
                    "possessive-brother-pattern".to_string(),
                ));
            }
        }

        // Verb patterns with space before to ensure word boundaries
        if between.contains(" mentor") {
            return Some(("mentors".to_string(), "verb-mentor-pattern".to_string()));
        }
        if between.contains(" work") && between.contains(" at") {
            return Some(("works_at".to_string(), "verb-works-at-pattern".to_string()));
        }
        if between.contains(", who ") && (between.contains("work") || between.contains("mentor"))
        {
            if between.contains("work") && between.contains("at") {
                return Some((
                    "works_at".to_string(),
                    "relative-works-at-pattern".to_string(),
                ));
            }
            if between.contains("mentor") {
                return Some(("mentors".to_string(), "relative-mentor-pattern".to_string()));
            }
        }

        None
    } else {
        None
    }
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
