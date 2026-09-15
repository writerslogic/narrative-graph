use narrative_graph::{extract_candidate_triples, Options};

#[test]
fn test_simple_possessive_pattern() {
    let text = "Elena is Marco's sister.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].subject, "elena");
    assert_eq!(candidates[0].relation, "sister_of");
    assert_eq!(candidates[0].object, "marco");
    assert!(candidates[0].confidence > 0.7);
}

#[test]
fn test_simple_verb_pattern() {
    let text = "Marco mentors Dev.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].subject, "marco");
    assert_eq!(candidates[0].relation, "mentors");
    assert_eq!(candidates[0].object, "dev");
    assert!(candidates[0].confidence > 0.7);
}

#[test]
fn test_works_at_pattern() {
    let text = "Dev works at the Archive.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].subject, "dev");
    assert_eq!(candidates[0].relation, "works_at");
    assert_eq!(candidates[0].object, "archive");
    assert!(candidates[0].confidence > 0.6);
}

#[test]
fn test_relation_rule_attribution() {
    let text = "Elena is Marco's sister.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].rule, "possessive-sister-pattern");
}

#[test]
fn test_relative_clause_pattern() {
    let text = "Marco mentors Dev, who works at the Archive.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let dev_works_at = candidates.iter().find(|c| c.subject == "dev" && c.relation == "works_at");
    assert!(dev_works_at.is_some(), "relative clause pattern should extract works_at");
}

#[test]
fn test_confidence_threshold() {
    let text = "Elena is Marco's sister. Marco mentors Dev.";
    let mut opts = Options::default();
    opts.min_confidence = Some(0.8);

    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    for candidate in &candidates {
        assert!(candidate.confidence >= 0.8, "candidate confidence too low");
    }
}

#[test]
fn test_multiple_relations_same_sentence() {
    let text = "Elena is Marco's sister. Marco mentors Dev, who works at the Archive.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert!(candidates.len() >= 3, "should extract at least 3 relations");

    let has_elena_marco = candidates.iter().any(|c| c.subject == "elena" && c.object == "marco");
    let has_marco_dev = candidates.iter().any(|c| c.subject == "marco" && c.object == "dev");
    let has_dev_archive = candidates.iter().any(|c| c.subject == "dev" && c.object == "archive");

    assert!(has_elena_marco, "missing elena-marco relation");
    assert!(has_marco_dev, "missing marco-dev relation");
    assert!(has_dev_archive, "missing dev-archive relation");
}

#[test]
fn test_span_coverage() {
    let text = "Elena is Marco's sister.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];

    assert!(candidate.span[0] < candidate.span[1], "span end must be after start");
    assert!(candidate.span[1] <= text.len(), "span must not exceed text length");
    assert_eq!(candidate.span[0], 0, "span should start at beginning of text");
}

#[test]
fn test_empty_text() {
    let opts = Options::default();
    let candidates = extract_candidate_triples("", &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 0);
}

#[test]
fn test_no_entities() {
    let text = "The quick brown fox jumps over the lazy dog.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 0);
}

#[test]
fn test_deduplication() {
    let text = "Marco mentors Dev. Marco mentors Dev again.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let marco_dev_count = candidates
        .iter()
        .filter(|c| c.subject == "marco" && c.relation == "mentors" && c.object == "dev")
        .count();

    assert_eq!(marco_dev_count, 1, "should deduplicate identical triples");
}
