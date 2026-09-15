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

    let dev_works_at = candidates
        .iter()
        .find(|c| c.subject == "dev" && c.relation == "works_at");
    assert!(
        dev_works_at.is_some(),
        "relative clause pattern should extract works_at"
    );
}

#[test]
fn test_confidence_threshold() {
    let text = "Elena is Marco's sister. Marco mentors Dev.";
    let opts = Options {
        min_confidence: Some(0.8),
        ..Default::default()
    };

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

    let has_elena_marco = candidates
        .iter()
        .any(|c| c.subject == "elena" && c.object == "marco");
    let has_marco_dev = candidates
        .iter()
        .any(|c| c.subject == "marco" && c.object == "dev");
    let has_dev_archive = candidates
        .iter()
        .any(|c| c.subject == "dev" && c.object == "archive");

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

    assert!(
        candidate.span[0] < candidate.span[1],
        "span end must be after start"
    );
    assert!(
        candidate.span[1] <= text.len(),
        "span must not exceed text length"
    );
    assert_eq!(
        candidate.span[0], 0,
        "span should start at beginning of text"
    );
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

#[test]
fn test_overlapping_mentions_do_not_panic() {
    let text = "Mary Jane mentors Jane.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    for candidate in &candidates {
        assert!(
            candidate.span[0] <= candidate.span[1] && candidate.span[1] <= text.len(),
            "span out of range"
        );
    }
}

#[test]
fn test_subject_matched_at_its_own_mention_not_an_earlier_substring() {
    let text = "Devon greeted Dev, who works at the Archive.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let dev_works_at = candidates
        .iter()
        .find(|c| c.subject == "dev" && c.relation == "works_at");
    assert!(
        dev_works_at.is_some(),
        "\"Dev\" must match its own mention, not the \"Dev\" inside \"Devon\": {candidates:?}"
    );
}

#[test]
fn test_entity_whose_lowercase_changes_byte_length() {
    // "İ" is 2 bytes and lowercases to 3, so offsets taken from a lowercased
    // copy of the text do not address the original.
    let text = "İstanbul mentors Dev.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let mentors = candidates
        .iter()
        .find(|c| c.relation == "mentors" && c.object == "dev");
    assert!(
        mentors.is_some(),
        "relation lost to a lowercase byte-length shift: {candidates:?}"
    );
}

#[test]
fn test_span_bounds_the_subject_and_object_mentions() {
    // A multi-word entity's `text` is rebuilt with single spaces, so compare
    // against the source with its own whitespace collapsed.
    fn collapse(s: &str) -> String {
        s.to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    for text in [
        "Elena is Marco's sister.",
        "Marco mentors Dev.",
        "Dev works at the Archive.",
        "Mary\nJane mentors Dev.",
        "Mary  Jane mentors Dev.",
    ] {
        let opts = Options::default();
        let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
        assert!(!candidates.is_empty(), "no candidates for {text:?}");

        for candidate in &candidates {
            let [start, end] = candidate.span;
            assert!(
                text.is_char_boundary(start) && text.is_char_boundary(end),
                "non-char-boundary span {:?} for {text:?}",
                candidate.span
            );
            let covered = collapse(&text[start..end]);
            assert!(
                covered.starts_with(&candidate.subject.replace('_', " ")),
                "span {covered:?} does not start at the subject {:?}",
                candidate.subject
            );
            assert!(
                covered.ends_with(&candidate.object.replace('_', " ")),
                "span {covered:?} does not end at the object {:?}",
                candidate.object
            );
        }
    }
}

#[test]
fn test_pipeline_never_panics_and_spans_stay_valid() {
    let alphabet = [
        "Elena",
        "Marco",
        "Dev",
        "Jane",
        "Mary Jane",
        "Devon",
        "İstanbul",
        "the Archive",
        "is",
        "mentors",
        "works",
        "at",
        "who",
        "'s",
        "sister",
        "brother",
        " ",
        ",",
        ".",
        "!",
        "she",
        "her",
        "they",
        "—",
        "é",
        "\u{1F600}",
    ];
    // Deterministic pseudo-random walk over the alphabet.
    let mut state: u64 = 0x2545F4914F6CDD1D;
    for _ in 0..20000 {
        let mut s = String::new();
        let len = (state % 16) as usize;
        for _ in 0..len {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            s.push_str(alphabet[(state >> 33) as usize % alphabet.len()]);
            s.push(' ');
        }
        let opts = Options::default();
        let candidates = extract_candidate_triples(&s, &opts).expect("extraction failed");
        for candidate in &candidates {
            let [start, end] = candidate.span;
            assert!(
                start <= end && end <= s.len(),
                "span out of range for {s:?}"
            );
            assert!(
                s.is_char_boundary(start) && s.is_char_boundary(end),
                "non-char-boundary span for {s:?}"
            );
        }
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
    }
}

#[test]
fn test_possessive_does_not_reach_into_a_later_clause() {
    // The relational noun belongs to a different clause and a different
    // referent; matching it emitted the highest-confidence rule in the system.
    let text = "Elena is Marco's dog, and she has a sister.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert!(
        !candidates
            .iter()
            .any(|c| c.relation == "sister_of" || c.relation == "brother_of"),
        "possessive matched past the possessed noun phrase: {candidates:?}"
    );
}

#[test]
fn test_possessive_allows_a_modifier_before_the_noun() {
    let text = "Elena is Marco's older sister.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].subject, "elena");
    assert_eq!(candidates[0].relation, "sister_of");
    assert_eq!(candidates[0].object, "marco");
}

#[test]
fn test_passive_voice_orients_the_relation_by_role_not_text_order() {
    let text = "Elena was mentored by Marco.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].relation, "mentors");
    assert_eq!(candidates[0].subject, "marco");
    assert_eq!(candidates[0].object, "elena");
}

#[test]
fn test_active_voice_mentor_direction_is_unchanged() {
    let text = "Marco mentors Elena.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].subject, "marco");
    assert_eq!(candidates[0].object, "elena");
}
