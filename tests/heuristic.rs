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
fn test_span_covers_both_mentions_and_opens_on_the_first() {
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
                covered.contains(&candidate.subject.replace('_', " ")),
                "span {covered:?} does not cover the subject {:?}",
                candidate.subject
            );
            assert!(
                covered.contains(&candidate.object.replace('_', " ")),
                "span {covered:?} does not cover the object {:?}",
                candidate.object
            );
            // The span opens on whichever mention comes first in the text;
            // passive voice puts the relation's object there.
            assert!(
                covered.starts_with(&candidate.subject.replace('_', " "))
                    || covered.starts_with(&candidate.object.replace('_', " ")),
                "span {covered:?} does not open on either mention"
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

#[test]
fn test_span_covers_the_token_that_licensed_the_relation() {
    // The possessed noun sits past the object, so a span ending at the object
    // excludes the only evidence for the relation.
    let text = "Elena is Marco's sister.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(
        &text[candidates[0].span[0]..candidates[0].span[1]],
        "Elena is Marco's sister"
    );
}

#[test]
fn test_a_referent_mentioned_twice_yields_both_relations() {
    let text = "Elena is Marco's sister and Marco mentors Elena.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .any(|c| c.subject == "elena" && c.relation == "sister_of" && c.object == "marco"));
    assert!(candidates
        .iter()
        .any(|c| c.subject == "marco" && c.relation == "mentors" && c.object == "elena"));
}

#[test]
fn test_no_relation_holds_between_a_referent_and_itself() {
    let text = "Elena is Marco's sister and she mentors Dev.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert!(
        candidates.iter().all(|c| c.subject != c.object),
        "self-relation emitted: {candidates:?}"
    );
}

#[test]
fn test_pronoun_antecedent_strips_the_possessive_clitic() {
    // "Marco's" as an antecedent yields a second referent that never unifies
    // with the mention it came from.
    let text = "Elena is Marco's sister and she mentors Dev.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert!(
        candidates
            .iter()
            .all(|c| !c.subject.contains('\'') && !c.object.contains('\'')),
        "possessive clitic leaked into an entity name: {candidates:?}"
    );
}

#[test]
fn test_a_longer_kinship_noun_is_not_matched_as_the_shorter_one_inside_it() {
    let text = "Elena is Marco's grandmother.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].relation, "grandmother_of");
}

#[test]
fn test_social_stance_scores_below_stated_kinship() {
    let opts = Options::default();
    let kin =
        extract_candidate_triples("Elena is Marco's sister.", &opts).expect("extraction failed");
    let stance =
        extract_candidate_triples("Elena is Marco's friend.", &opts).expect("extraction failed");

    assert_eq!(kin.len(), 1);
    assert_eq!(stance.len(), 1);
    assert_eq!(stance[0].relation, "friend_of");
    assert!(
        stance[0].confidence < kin[0].confidence,
        "stance {} should score below kinship {}",
        stance[0].confidence,
        kin[0].confidence
    );
}

#[test]
fn test_every_lexicon_noun_yields_a_distinctly_attributed_rule() {
    let opts = Options::default();

    for (noun, relation) in [
        ("employer", "employer_of"),
        ("apprentice", "apprentice_of"),
        ("husband", "husband_of"),
        ("rival", "rival_of"),
    ] {
        let text = format!("Elena is Marco's {noun}.");
        let candidates = extract_candidate_triples(&text, &opts).expect("extraction failed");

        assert_eq!(candidates.len(), 1, "no candidate for {text:?}");
        assert_eq!(candidates[0].relation, relation);
        assert_eq!(candidates[0].rule, format!("possessive-{noun}-pattern"));
    }
}

#[test]
fn test_a_noun_and_a_verb_for_one_relation_share_a_label() {
    // "Elena is Marco's mentor" and "Elena mentors Marco" are the same fact.
    // Two labels would give a consumer two edge types for one relation.
    let opts = Options::default();
    let noun =
        extract_candidate_triples("Elena is Marco's mentor.", &opts).expect("extraction failed");
    let verb = extract_candidate_triples("Elena mentors Marco.", &opts).expect("extraction failed");

    assert_eq!(noun.len(), 1);
    assert_eq!(verb.len(), 1);
    assert_eq!(noun[0].relation, verb[0].relation);
    assert_eq!(noun[0].subject, verb[0].subject);
    assert_eq!(noun[0].object, verb[0].object);
    // Attribution still distinguishes how each was found.
    assert_ne!(noun[0].rule, verb[0].rule);
}

#[test]
fn test_a_possessive_without_a_copula_claims_nothing() {
    // "X <verb> Y's sister" names a third person. Reading it as sister_of(X, Y)
    // was the single highest-confidence false positive in the system.
    let opts = Options::default();

    for text in [
        "Elena visited Marco's sister.",
        "Elena killed Marco's brother.",
        "Elena knew Marco's mother.",
        "Elena hated Marco's friend.",
        "Elena is not Marco's sister.",
        "Elena was never Marco's wife.",
        "Elena could be Marco's sister.",
        "Elena might be Marco's daughter.",
        "Elena, unlike Marco's brother, stayed.",
    ] {
        let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
        assert!(candidates.is_empty(), "{text:?} yielded {candidates:?}");
    }
}

#[test]
fn test_a_reported_possessive_does_not_claim_the_reporter() {
    // Only the pair joined by the copula is a claim; "Dev" merely believes it.
    let opts = Options::default();
    let candidates = extract_candidate_triples("Dev believed Elena was Marco's sister.", &opts)
        .expect("extraction failed");

    assert_eq!(candidates.len(), 1, "{candidates:?}");
    assert_eq!(candidates[0].subject, "elena");
    assert_eq!(candidates[0].object, "marco");
}

#[test]
fn test_a_relational_noun_must_head_the_possessed_phrase() {
    // A noun modifying a later one names a thing, and a noun behind a second
    // possessive belongs to that possessor, not to the object.
    let opts = Options::default();

    for text in [
        "Elena is Marco's master key.",
        "Elena is Marco's student union.",
        "Elena is Marco's friend's sister.",
    ] {
        let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
        assert!(candidates.is_empty(), "{text:?} yielded {candidates:?}");
    }
}

#[test]
fn test_a_denied_or_suspended_relation_is_not_asserted() {
    // Negation, an open condition and a suspended infinitive are not weaker
    // claims that the relation holds; they are the opposite claim, or none.
    let opts = Options::default();

    for text in [
        "Elena did not mentor Marco.",
        "Elena never mentored Marco.",
        "Elena no longer works at the Archive.",
        "Elena does not work at the Archive.",
        "Elena refused to mentor Marco.",
        "Elena hoped to mentor Marco.",
        "Elena wanted to work at the Archive.",
    ] {
        let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
        assert!(candidates.is_empty(), "{text:?} yielded {candidates:?}");
    }
}

#[test]
fn test_a_question_asks_the_relation_rather_than_stating_it() {
    let opts = Options::default();
    let asked =
        extract_candidate_triples("Did Elena mentor Marco?", &opts).expect("extraction failed");
    let stated =
        extract_candidate_triples("Elena mentors Marco.", &opts).expect("extraction failed");

    assert!(asked.is_empty(), "{asked:?}");
    assert_eq!(stated.len(), 1);
}

#[test]
fn test_a_sentence_opener_is_not_folded_into_the_mention() {
    // "But Elena" normalizes to `but_elena` and never unifies with `elena`,
    // which splits one character into two nodes of the graph.
    let opts = Options::default();

    for text in [
        "But Elena mentors Marco.",
        "And Elena mentors Marco.",
        "When Elena mentors Marco, things change.",
        "She left. But Elena mentors Marco.",
    ] {
        let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
        assert_eq!(candidates.len(), 1, "{text:?} yielded {candidates:?}");
        assert_eq!(candidates[0].subject, "elena", "{text:?}");
    }
}

#[test]
fn test_a_given_name_that_reads_as_a_function_word_survives() {
    // Dropping a real mention costs more than keeping a malformed one, so the
    // opener list holds no word that can also be a name.
    let opts = Options::default();

    for (text, subject) in [
        ("May Vance is Marco's sister.", "may_vance"),
        ("Will Vance is Marco's sister.", "will_vance"),
        ("Grace Vance is Marco's sister.", "grace_vance"),
    ] {
        let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
        assert_eq!(candidates.len(), 1, "{text:?} yielded {candidates:?}");
        assert_eq!(candidates[0].subject, subject, "{text:?}");
    }
}

#[test]
fn test_a_capitalized_pronoun_is_not_a_second_mention() {
    // `extract_pronouns` owns pronouns. A capitalized one otherwise becomes a
    // rival mention at the same offsets whose normalized name, `she`, is a
    // graph node naming nobody.
    let opts = Options::default();
    let candidates = extract_candidate_triples("Elena arrived. She is Marco's student.", &opts)
        .expect("extraction failed");

    assert!(
        candidates.iter().all(|c| c.subject != "she"),
        "{candidates:?}"
    );
}
