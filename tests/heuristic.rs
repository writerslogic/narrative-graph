use narrative_graph::heuristic::extract_entities;
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

#[test]
fn test_every_copula_the_possessive_accepts_is_reachable() {
    // The whitelist is the gate; an entry nothing exercises can be narrowed
    // away without a test failing.
    let opts = Options::default();

    for (text, subject) in [
        ("Elena is Marco's sister.", "elena"),
        ("Elena was Marco's sister.", "elena"),
    ] {
        let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
        assert!(
            candidates
                .iter()
                .any(|c| c.subject == subject && c.object == "marco"),
            "{text:?} yielded {candidates:?}"
        );
    }

    // A plural copula is deliberately not on the whitelist: the sentence has
    // two subjects and the pair loop would claim the relation for one.
    let plural = extract_candidate_triples("Dev and Elena are Marco's cousins.", &opts)
        .expect("extraction failed");
    assert!(plural.is_empty(), "{plural:?}");
}

#[test]
fn test_a_quoted_sentence_after_the_attribution_comma_starts_a_sentence() {
    // Fiction puts capitalized pronouns inside dialogue, where the only thing
    // before the opening quote is the comma of the attribution.
    let opts = Options::default();
    let candidates = extract_candidate_triples("Elena said, \"She is Marco's student.\"", &opts)
        .expect("extraction failed");

    assert!(
        candidates.iter().all(|c| c.subject != "she"),
        "{candidates:?}"
    );
}

/// The mirror of "x is y's sister". Prose writes the possessive first at least
/// as often, and before this the whole form extracted nothing.
#[test]
fn test_reversed_possessive_pattern() {
    let text = "Mrs. Manson Mingott's father was Bob Spicer.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let found = candidates
        .iter()
        .find(|c| c.relation == "father_of")
        .expect("reversed possessive not matched");
    assert_eq!(found.subject, "bob_spicer");
    assert_eq!(found.object, "manson_mingott");
}

/// The copula links the possessed phrase to a second possessor, not to a
/// person: the sister is Marco's, and no relation holds between Elena and
/// Marco.
#[test]
fn test_reversed_possessive_rejects_second_possessor() {
    let text = "Elena's mother was Marco's sister.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert!(
        !candidates
            .iter()
            .any(|c| c.relation == "mother_of" && c.object == "elena"),
        "reversed possessive fired across a second possessive: {candidates:?}"
    );
}

/// The appositive renames the subject. "Bob Spicer, old Mrs. Mingott's father"
/// asserts exactly what "Bob Spicer is Mrs. Mingott's father" does.
#[test]
fn test_appositive_possessive_pattern() {
    let text = "Bob Spicer, old Mrs. Manson Mingott's father, came to the house.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let found = candidates
        .iter()
        .find(|c| c.relation == "father_of")
        .expect("appositive not matched");
    assert_eq!(found.subject, "bob_spicer");
    assert_eq!(found.object, "manson_mingott");
}

/// A relative clause is not an apposition. "Elena, who visited Marco's sister"
/// says Elena visited someone; it does not say Elena is that someone.
#[test]
fn test_appositive_rejects_relative_clause() {
    let text = "Elena, who visited Marco's sister, left before dawn.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert!(
        candidates.is_empty(),
        "relative clause read as an apposition: {candidates:?}"
    );
}

/// A conjunction makes a list, not a renaming. "Dev, and Marco's sister" names
/// two people.
#[test]
fn test_appositive_rejects_conjunction() {
    let text = "Dev, and Marco's sister, waited at the gate.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert!(
        candidates.is_empty(),
        "conjunction read as an apposition: {candidates:?}"
    );
}

/// The "of" genitive, which is what prose actually uses. Measured over three
/// novels it outnumbers every Saxon-possessive form combined.
#[test]
fn test_of_genitive_appositive() {
    let text = "Newland Archer, the husband of Ellen Olenska, said nothing.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let found = candidates
        .iter()
        .find(|c| c.relation == "husband_of")
        .expect("of genitive not matched");
    assert_eq!(found.subject, "newland_archer");
    assert_eq!(found.object, "ellen_olenska");
}

/// The same form with a copula instead of the appositive comma.
#[test]
fn test_of_genitive_copula() {
    let text = "Struthers was the guardian of Ellen Olenska.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let found = candidates
        .iter()
        .find(|c| c.relation == "guardian_of")
        .expect("of genitive with copula not matched");
    assert_eq!(found.subject, "struthers");
    assert_eq!(found.object, "ellen_olenska");
}

/// The genitive hands off to a further possessive, so the relation holds
/// against the wife rather than against Marco.
#[test]
fn test_of_genitive_rejects_trailing_possessive() {
    let text = "Elena, the sister of Marco's wife, arrived late.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert!(
        !candidates
            .iter()
            .any(|c| c.relation == "sister_of" && c.object == "marco"),
        "of genitive fired across a trailing possessive: {candidates:?}"
    );
}

/// A denial is not a weaker assertion. The gate runs ahead of every rule,
/// including this one.
#[test]
fn test_of_genitive_respects_assertion_gate() {
    let text = "Newland Archer was not the husband of Ellen Olenska.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert!(
        candidates.is_empty(),
        "denied of genitive still extracted: {candidates:?}"
    );
}

/// Issue #7. A lowercase particle inside a name ended the capitalized run, so
/// the mention was the fragment after it and both people in this sentence
/// normalized to the surname they share.
#[test]
fn test_a_particle_does_not_truncate_a_name() {
    let text = "Lady Catherine de Bourgh, widow of Sir Lewis de Bourgh, said nothing.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let found = candidates
        .iter()
        .find(|c| c.relation == "widow_of")
        .expect("particle-bearing names lost the relation");
    assert_eq!(found.subject, "catherine_de_bourgh");
    assert_eq!(found.object, "lewis_de_bourgh");
}

/// A particle only continues a run that has already started. Capitalized, it
/// opens a mention the way any capitalized word does, and that shape is
/// unchanged by the particle list.
#[test]
fn test_a_capitalized_particle_still_opens_a_mention() {
    let text = "De Souza is Marco's sister.";
    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    let found = candidates
        .iter()
        .find(|c| c.relation == "sister_of")
        .expect("sentence-initial particle lost the relation");
    assert_eq!(found.subject, "de_souza");
}

/// A trailing particle belongs to the clause after it, not to the name before
/// it. Gluing it on would extend the mention past the person, and the mention's
/// end offset is what bounds every relation rule's between-text.
#[test]
fn test_a_trailing_particle_is_not_folded_into_the_mention() {
    let aliases = std::collections::BTreeMap::new();

    for text in [
        "Elena van derided Marco.",
        "Catherine de, Marco's sister, said nothing.",
    ] {
        let mentions = extract_entities(text, &aliases);
        let first = &mentions[0];
        assert!(
            !first.text.contains(' '),
            "a dangling particle was folded into {first:?} from {text:?}"
        );
        assert_eq!(
            &text[first.start..first.end],
            first.text,
            "the mention's offsets do not bound its own text"
        );
    }
}

/// Issue #8. A title is capitalized and belongs to the surface form, so the
/// capitalized run swallows it and the mention names the honorific as well as
/// the person. The identity has to come out the same with the titles and
/// without them, or every styled mention is its own graph node.
#[test]
fn test_a_title_is_not_part_of_the_identity() {
    let aliases = std::collections::BTreeMap::new();

    for (text, surface, expected) in [
        (
            "the Right Honourable Lady Catherine de Bourgh spoke",
            "Right Honourable Lady Catherine de Bourgh",
            "catherine_de_bourgh",
        ),
        (
            "Lady Catherine de Bourgh spoke",
            "Lady Catherine de Bourgh",
            "catherine_de_bourgh",
        ),
        (
            "Catherine de Bourgh spoke",
            "Catherine de Bourgh",
            "catherine_de_bourgh",
        ),
        (
            "Sir Lewis de Bourgh spoke",
            "Sir Lewis de Bourgh",
            "lewis_de_bourgh",
        ),
        ("Mr. Percy Cahill spoke", "Mr Percy Cahill", "percy_cahill"),
    ] {
        let mentions = extract_entities(text, &aliases);
        assert_eq!(mentions[0].normalized, expected, "from {text:?}");
        // The title leaves the identity only. It is what the text says, so the
        // surface form keeps it, and `aliases` is keyed on that form.
        assert_eq!(mentions[0].text, surface, "from {text:?}");
    }
}

/// The title in front of a bare surname is the only thing distinguishing the
/// people who share it. Stripping it would make Miss Darcy her own father, and
/// `extract_relations` then drops the pair as a self-relation, so the rule
/// costs a correct candidate rather than repairing one.
#[test]
fn test_a_title_in_front_of_one_name_word_is_kept() {
    let aliases = std::collections::BTreeMap::new();

    for (text, expected) in [
        ("Miss Darcy spoke", "miss_darcy"),
        ("Mr. Darcy spoke", "mr_darcy"),
        ("Mrs. Charles spoke", "mrs_charles"),
        // A particle is not a name word. Stripping here would leave the
        // surname shared with every other de Bourgh.
        ("Lady de Bourgh spoke", "lady_de_bourgh"),
        // A run that is only a title is left alone: a mention has to name
        // something, and there is no name here to reduce it to.
        ("the Colonel spoke", "colonel"),
    ] {
        let mentions = extract_entities(text, &aliases);
        assert_eq!(mentions[0].normalized, expected, "from {text:?}");
    }
}

/// "Right" and "Most" are titles only in front of another title. Alone they
/// are ordinary adjectives, and a mention that opens with one is a name.
#[test]
fn test_a_style_qualifier_alone_is_not_a_title() {
    let aliases = std::collections::BTreeMap::new();

    for (text, expected) in [
        ("Right Whale Bay is far south", "right_whale_bay"),
        ("Most Holy Redeemer stands there", "most_holy_redeemer"),
    ] {
        let mentions = extract_entities(text, &aliases);
        assert_eq!(mentions[0].normalized, expected, "from {text:?}");
    }
}

fn lexicon(pairs: &[(&str, &str)]) -> Options {
    Options {
        aliases: pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        ..Options::default()
    }
}

/// Issue #9. A lowercase phrase is never a capitalized run, so the alias map
/// could not reach the case it was documented for: a novel naming someone
/// "Marcus" in one paragraph and "the detective" in the next.
#[test]
fn test_a_lowercase_alias_key_becomes_a_mention() {
    let opts = lexicon(&[("the detective", "marcus")]);
    let candidates = extract_candidate_triples("The detective is Ms. Chen's brother.", &opts)
        .expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].subject, "marcus");
    assert_eq!(candidates[0].relation, "brother_of");
    // The capitalized run is untouched: only a lowercase key is a lexicon key.
    assert_eq!(candidates[0].object, "ms_chen");
}

/// The match ignores case because "The detective" opening a sentence and "the
/// detective" inside one are the same phrase. A key with any uppercase
/// character stays the exact surface-form match it was, and both kinds coexist
/// in one map.
#[test]
fn test_a_lexicon_key_matches_either_case_and_an_exact_key_still_does_not() {
    let opts = lexicon(&[("the detective", "marcus"), ("Ms Chen", "chen")]);
    let candidates =
        extract_candidate_triples("Elena knew the detective was Ms. Chen's brother.", &opts)
            .expect("extraction failed");

    let found = candidates
        .iter()
        .find(|c| c.relation == "brother_of")
        .expect("the mid-sentence lexicon phrase was not matched");
    assert_eq!(found.subject, "marcus");
    assert_eq!(found.object, "chen");
}

/// A lexicon mention is added, never substituted. A key overlapping a run would
/// otherwise delete the run and everything the longer name distinguishes.
#[test]
fn test_a_lexicon_match_never_replaces_a_detected_mention() {
    let opts = lexicon(&[("detective", "marcus")]);
    let candidates = extract_candidate_triples("Detective Marcus is Elena's brother.", &opts)
        .expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].subject, "detective_marcus");
}

/// Longest key first, so the caller can supply both halves of a phrase without
/// the shorter one claiming the text the longer one names.
#[test]
fn test_the_longest_lexicon_key_claims_the_phrase() {
    let opts = lexicon(&[("the detective", "marcus"), ("detective", "someone_else")]);
    let candidates = extract_candidate_triples("The detective is Elena's brother.", &opts)
        .expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].subject, "marcus");
}

/// The match is word-bounded, so a key cannot fire inside a longer word.
#[test]
fn test_a_lexicon_key_does_not_match_inside_a_word() {
    let opts = lexicon(&[("the detective", "marcus")]);
    let candidates = extract_candidate_triples("The detectives are Elena's brothers.", &opts)
        .expect("extraction failed");

    assert!(
        candidates.iter().all(|c| c.subject != "marcus"),
        "a lexicon key matched inside a longer word: {candidates:?}"
    );
}

/// A capitalized run is a mention already, so a key spanning one is dropped
/// whole rather than cut down to the part that is free. The offsets a lexicon
/// mention carries address the source text, which a match run over a lowercased
/// copy would not: a lowercase form can differ in byte length from the
/// character it came from.
#[test]
fn test_a_lexicon_mention_offsets_address_the_source_text() {
    let aliases = std::collections::BTreeMap::from([
        ("the i\u{307}stanbul agent".to_string(), "kemal".to_string()),
        ("the detective".to_string(), "marcus".to_string()),
    ]);
    let text = "The \u{130}stanbul agent told the detective everything.";
    let mentions = extract_entities(text, &aliases);

    for mention in &mentions {
        assert_eq!(
            &text[mention.start..mention.end],
            mention.text,
            "the mention's offsets do not bound its own text: {mention:?}"
        );
    }
    assert!(
        mentions.iter().any(|m| m.normalized == "marcus"),
        "the phrase past the multi-byte character was not matched: {mentions:?}"
    );
    assert!(
        mentions.iter().all(|m| m.normalized != "kemal"),
        "a key overlapping the run it contains replaced it: {mentions:?}"
    );
}
