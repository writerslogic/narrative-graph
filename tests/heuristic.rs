use narrative_graph::heuristic::extract_entities;
use narrative_graph::{
    extract_aggregates, extract_candidate_triples, find_conflicts, ConflictKind, Options, Polarity,
    Rejection, TripleCandidate,
};
use std::collections::BTreeSet;

fn extract(text: &str) -> Vec<TripleCandidate> {
    extract_candidate_triples(text, &Options::default()).expect("extraction failed")
}

fn has(candidates: &[TripleCandidate], subject: &str, relation: &str, object: &str) -> bool {
    candidates
        .iter()
        .any(|c| c.subject == subject && c.relation == relation && c.object == object)
}

#[test]
fn a_simple_statement_yields_exactly_its_triple() {
    for (text, subject, relation, object) in [
        ("Elena is Marco's sister.", "elena", "sister_of", "marco"),
        ("Marco mentors Dev.", "marco", "mentors", "dev"),
        ("Dev works at the Archive.", "dev", "works_at", "archive"),
        (
            "Elena is Marco's older sister.",
            "elena",
            "sister_of",
            "marco",
        ),
        // Passive voice names the mentor second.
        ("Elena was mentored by Marco.", "marco", "mentors", "elena"),
        ("Marco mentors Elena.", "marco", "mentors", "elena"),
        // A longer kinship noun is not matched as the shorter one inside it.
        (
            "Elena is Marco's grandmother.",
            "elena",
            "grandmother_of",
            "marco",
        ),
    ] {
        let candidates = extract(text);
        assert_eq!(candidates.len(), 1, "{text:?} yielded {candidates:?}");
        assert!(
            has(&candidates, subject, relation, object),
            "{text:?} yielded {candidates:?}"
        );
    }
}

#[test]
fn each_relation_form_yields_its_triple() {
    for (text, subject, relation, object) in [
        ("Elena was Marco's sister.", "elena", "sister_of", "marco"),
        // Reversed possessive: "y's [relation] is x".
        (
            "Mrs. Manson Mingott's father was Bob Spicer.",
            "bob_spicer",
            "father_of",
            "manson_mingott",
        ),
        // An appositive renames the subject.
        (
            "Bob Spicer, old Mrs. Manson Mingott's father, came to the house.",
            "bob_spicer",
            "father_of",
            "manson_mingott",
        ),
        // The "of" genitive, appositive and copular.
        (
            "Newland Archer, the husband of Ellen Olenska, said nothing.",
            "newland_archer",
            "husband_of",
            "ellen_olenska",
        ),
        (
            "Struthers was the guardian of Ellen Olenska.",
            "struthers",
            "guardian_of",
            "ellen_olenska",
        ),
        // Issue #7: a lowercase particle inside a name does not truncate it.
        (
            "Lady Catherine de Bourgh, widow of Sir Lewis de Bourgh, said nothing.",
            "catherine_de_bourgh",
            "widow_of",
            "lewis_de_bourgh",
        ),
        // A capitalized particle opens a mention like any capitalized word.
        (
            "De Souza is Marco's sister.",
            "de_souza",
            "sister_of",
            "marco",
        ),
    ] {
        let candidates = extract(text);
        assert!(
            has(&candidates, subject, relation, object),
            "{text:?} yielded {candidates:?}"
        );
    }
}

#[test]
fn text_that_states_no_relation_yields_nothing() {
    for text in [
        "",
        "The quick brown fox jumps over the lazy dog.",
        // "X <verb> Y's sister" names a third person.
        "Elena visited Marco's sister.",
        "Elena killed Marco's brother.",
        "Elena knew Marco's mother.",
        "Elena hated Marco's friend.",
        "Elena, unlike Marco's brother, stayed.",
        // A relative clause or a conjunction is not an apposition.
        "Elena, who visited Marco's sister, left before dawn.",
        "Dev, and Marco's sister, waited at the gate.",
        // The relational noun must head the possessed phrase.
        "Elena is Marco's master key.",
        "Elena is Marco's student union.",
        "Elena is Marco's friend's sister.",
        // A plural copula has two subjects; the pair loop would claim it for one.
        "Dev and Elena are Marco's cousins.",
        // Denied, modal, suspended, or asked.
        "Elena is not Marco's sister.",
        "Elena was never Marco's wife.",
        "Elena could be Marco's sister.",
        "Elena might be Marco's daughter.",
        "Newland Archer was not the husband of Ellen Olenska.",
        "Elena did not mentor Marco.",
        "Elena never mentored Marco.",
        "Elena no longer works at the Archive.",
        "Elena does not work at the Archive.",
        "Elena refused to mentor Marco.",
        "Elena hoped to mentor Marco.",
        "Elena wanted to work at the Archive.",
        "Did Elena mentor Marco?",
        // A pronoun chain ends at a paragraph break or a sentence naming nobody.
        "Mrs. Bennet joined the Archive.\n\nShe works at it.",
        "Mrs. Bennet joined the Archive. Nothing happened. She works at it.",
    ] {
        let candidates = extract(text);
        assert!(candidates.is_empty(), "{text:?} yielded {candidates:?}");
    }
}

#[test]
fn a_relation_is_not_read_past_the_possessed_phrase() {
    for (text, relation, object) in [
        // The noun belongs to a later clause and a different referent.
        (
            "Elena is Marco's dog, and she has a sister.",
            "sister_of",
            None,
        ),
        // The copula links to a second possessor, not to a person.
        (
            "Elena's mother was Marco's sister.",
            "mother_of",
            Some("elena"),
        ),
        // The genitive hands off to a further possessive.
        (
            "Elena, the sister of Marco's wife, arrived late.",
            "sister_of",
            Some("marco"),
        ),
    ] {
        let candidates = extract(text);
        assert!(
            !candidates
                .iter()
                .any(|c| c.relation == relation && object.map_or(true, |o| c.object == o)),
            "{text:?} yielded {candidates:?}"
        );
    }
}

#[test]
fn statements_in_sequence_yield_every_triple() {
    let candidates =
        extract("Elena is Marco's sister. Marco mentors Dev, who works at the Archive.");
    assert!(
        has(&candidates, "elena", "sister_of", "marco"),
        "{candidates:?}"
    );
    assert!(
        has(&candidates, "marco", "mentors", "dev"),
        "{candidates:?}"
    );
    // The relative clause attaches to its antecedent.
    assert!(
        has(&candidates, "dev", "works_at", "archive"),
        "{candidates:?}"
    );
}

#[test]
fn a_resolved_pronoun_yields_clean_distinct_referents() {
    let candidates = extract("Elena is Marco's sister and she mentors Dev.");
    assert!(
        candidates.iter().all(|c| c.subject != c.object),
        "self-relation emitted: {candidates:?}"
    );
    assert!(
        candidates
            .iter()
            .all(|c| !c.subject.contains('\'') && !c.object.contains('\'')),
        "possessive clitic leaked into an entity name: {candidates:?}"
    );
}

#[test]
fn a_capitalized_pronoun_is_never_a_graph_node() {
    for text in [
        "She is Marco's sister.",
        "Elena said that She is Marco's sister.",
        "Elena arrived. She is Marco's student.",
        // A quotation after the attribution comma starts a sentence.
        "Elena said, \"She is Marco's student.\"",
    ] {
        let candidates = extract(text);
        assert!(
            candidates
                .iter()
                .all(|c| c.subject != "she" && c.object != "she"),
            "a pronoun became a graph node in {text:?}: {candidates:?}"
        );
    }
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
fn an_invalid_threshold_is_rejected_even_for_empty_text() {
    for threshold in [-0.1, 1.1, f32::NAN] {
        let opts = Options {
            min_confidence: Some(threshold),
            ..Default::default()
        };
        for text in ["", "Elena is Marco's sister."] {
            assert!(
                extract_candidate_triples(text, &opts).is_err(),
                "{threshold} {text:?}"
            );
        }
    }
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
        "Mary Jane mentors Jane.",
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
        ("sister", "sister_of"),
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

/// The third of issue #9's design questions: a lexicon mention is a referent a
/// pronoun can take. Without it the backward walk finds the sentence-opening
/// "The" and the pronoun names a function word.
#[test]
fn test_a_lexicon_mention_resolves_a_pronoun() {
    let aliases =
        std::collections::BTreeMap::from([("the detective".to_string(), "marcus".to_string())]);
    let text = "The detective said he was Elena's brother.";
    let mentions = extract_entities(text, &aliases);

    let pronoun = mentions
        .iter()
        .find(|m| m.text == "he")
        .expect("the pronoun was dropped");
    assert_eq!(pronoun.normalized, "marcus");
}

/// A capitalized run standing between the lexicon mention and the pronoun is
/// nearer, so it keeps the pronoun. The lexicon adds a referent; it does not
/// outrank one.
#[test]
fn test_the_nearest_mention_keeps_the_pronoun() {
    let aliases =
        std::collections::BTreeMap::from([("the detective".to_string(), "marcus".to_string())]);
    let text = "The detective told Elena she was wrong.";
    let mentions = extract_entities(text, &aliases);

    let pronoun = mentions
        .iter()
        .find(|m| m.text == "she")
        .expect("the pronoun was dropped");
    assert_eq!(pronoun.normalized, "elena");
}

/// A key spanning a pronoun replaces it rather than doubling it. The caller
/// named that referent outright, which beats resolving one, and two mentions at
/// one set of offsets would pair with each other.
#[test]
fn test_a_lexicon_key_covering_a_pronoun_replaces_it() {
    let aliases =
        std::collections::BTreeMap::from([("her ladyship".to_string(), "catherine".to_string())]);
    let text = "Her ladyship is Elizabeth's aunt.";
    let mentions = extract_entities(text, &aliases);

    assert!(
        mentions.iter().all(|m| m.text != "her"),
        "the pronoun and the lexicon mention both stand: {mentions:?}"
    );
    assert!(
        mentions.iter().any(|m| m.normalized == "catherine"),
        "the lexicon mention was dropped with the pronoun: {mentions:?}"
    );
}

/// The lexicon mention is the antecedent outright where the backward walk finds
/// no capitalized word at all, which is the case that has no nearness to judge.
#[test]
fn test_a_lexicon_mention_is_the_antecedent_when_nothing_else_precedes() {
    let aliases =
        std::collections::BTreeMap::from([("the detective".to_string(), "marcus".to_string())]);
    let text = "the detective knew she was late.";
    let mentions = extract_entities(text, &aliases);

    let pronoun = mentions
        .iter()
        .find(|m| m.text == "she")
        .expect("the pronoun was dropped");
    assert_eq!(pronoun.normalized, "marcus");
}

#[test]
fn a_rejection_suppresses_only_the_triple_it_names() {
    let text = "Elena is Marco's sister. Marco mentors Dev.";
    let opts = Options {
        rejections: vec![Rejection {
            subject: "elena".into(),
            relation: "sister_of".into(),
            object: "marco".into(),
        }],
        ..Default::default()
    };
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].relation, "mentors");
}

#[test]
fn a_rejection_is_directional_and_exact() {
    let text = "Elena is Marco's sister.";
    // The same pair the other way round, and the same pair under another
    // relation, are different claims and must still be emitted.
    for wrong in [
        Rejection {
            subject: "marco".into(),
            relation: "sister_of".into(),
            object: "elena".into(),
        },
        Rejection {
            subject: "elena".into(),
            relation: "brother_of".into(),
            object: "marco".into(),
        },
    ] {
        let opts = Options {
            rejections: vec![wrong],
            ..Default::default()
        };
        let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
        assert_eq!(
            candidates.len(),
            1,
            "a near-miss rejection suppressed a claim it does not name"
        );
    }
}

#[test]
fn a_rejection_keys_on_the_relation_the_caller_saw() {
    // The candidate carries the ontology-mapped relation, so that is what a
    // caller has to reject. Rejecting the pre-mapping name must not work.
    let text = "Elena is Marco's sister.";
    let with_ontology = || Options {
        ontology: [("sister_of".to_string(), "sibling".to_string())]
            .into_iter()
            .collect(),
        ..Default::default()
    };
    let rejected = |relation: &str| Options {
        rejections: vec![Rejection {
            subject: "elena".into(),
            relation: relation.into(),
            object: "marco".into(),
        }],
        ..with_ontology()
    };
    let mapped = with_ontology();
    assert_eq!(
        extract_candidate_triples(text, &mapped).unwrap()[0].relation,
        "sibling"
    );
    assert!(extract_candidate_triples(text, &rejected("sibling"))
        .unwrap()
        .is_empty());
    assert_eq!(
        extract_candidate_triples(text, &rejected("sister_of"))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn one_fact_stated_three_ways_yields_one_aggregate_with_three_spans() {
    let text = "Elena is Marco's sister. Marco mentors Dev. \
                Elena, the sister of Marco, left. Elena is Marco's sister.";
    let opts = Options::default();

    let aggregates = extract_aggregates(text, &opts).expect("extraction failed");
    let sisterhood = aggregates
        .iter()
        .find(|a| a.relation == "sister_of")
        .expect("no sister_of aggregate");

    assert_eq!(sisterhood.subject, "elena");
    assert_eq!(sisterhood.object, "marco");
    assert_eq!(sisterhood.spans.len(), 3);
    // Two rules produced them, each named once however many spans it produced.
    assert_eq!(
        sisterhood.rules,
        vec!["possessive-sister-pattern", "of-genitive-sister-pattern"]
    );

    // Every span addresses the source text and they are in document order.
    for pair in sisterhood.spans.windows(2) {
        assert!(pair[0][0] <= pair[1][0]);
    }
    for span in &sisterhood.spans {
        assert!(text.get(span[0]..span[1]).is_some());
    }

    // The per-sentence output is unchanged: one candidate for the fact, one span.
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
    let one = candidates
        .iter()
        .filter(|c| c.relation == "sister_of")
        .count();
    assert_eq!(one, 1);
}

#[test]
fn repetition_does_not_raise_an_aggregate_confidence() {
    let once = "Elena is Marco's sister.";
    let thrice = "Elena is Marco's sister. Elena is Marco's sister. Elena is Marco's sister.";
    let opts = Options::default();

    let a = &extract_aggregates(once, &opts).unwrap()[0];
    let b = &extract_aggregates(thrice, &opts).unwrap()[0];

    assert_eq!(b.spans.len(), 3);
    assert_eq!(a.confidence, b.confidence);
}

#[test]
fn aggregates_are_ordered_by_where_the_story_states_them() {
    // Two relations over the same pair, the second contradicting the first.
    // Ordering by first span is what lets a caller say which one the story
    // ends on; the alphabetical order of the per-sentence output cannot.
    let text = "Elena is Marco's enemy. Dev works at the Archive. Elena is Marco's ally.";
    let opts = Options::default();

    let aggregates = extract_aggregates(text, &opts).expect("extraction failed");
    let order: Vec<&str> = aggregates.iter().map(|a| a.relation.as_str()).collect();
    assert_eq!(order, vec!["enemy_of", "works_at", "ally_of"]);

    // The per-sentence output sorts the same facts by triple, subject first,
    // which is the documented behaviour and must not have moved.
    let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
    let alphabetical: Vec<(&str, &str)> = candidates
        .iter()
        .map(|c| (c.subject.as_str(), c.relation.as_str()))
        .collect();
    assert_eq!(
        alphabetical,
        vec![
            ("dev", "works_at"),
            ("elena", "ally_of"),
            ("elena", "enemy_of")
        ]
    );
}

#[test]
fn an_empty_passage_aggregates_to_nothing() {
    let opts = Options::default();
    assert!(extract_aggregates("", &opts).unwrap().is_empty());
    assert!(extract_aggregates("Nothing relational here.", &opts)
        .unwrap()
        .is_empty());
}

#[test]
fn a_denial_is_recorded_as_the_opposite_claim_not_dropped() {
    let opts = Options::default();

    for text in [
        "Elena is not Marco's sister.",
        "Elena isn't Marco's sister.",
        "Elena is not the sister of Marco.",
    ] {
        let aggregates = extract_aggregates(text, &opts).expect("extraction failed");
        assert_eq!(aggregates.len(), 1, "no denial recorded for {text:?}");
        assert_eq!(aggregates[0].polarity, Polarity::Denied, "for {text:?}");
        assert_eq!(aggregates[0].relation, "sister_of");

        // The per-sentence output has nowhere to say "denied", so it says
        // nothing — exactly as before denials were representable.
        assert!(extract_candidate_triples(text, &opts).unwrap().is_empty());
    }
}

#[test]
fn a_conditional_denial_settles_nothing_and_is_not_a_denial() {
    let opts = Options::default();

    // A suspender outranks a negator. Reading these as denials is how a
    // contradiction check invents a conflict the text never states.
    for text in [
        "If Elena is not Marco's sister, nothing follows.",
        "Elena might not be Marco's sister.",
        "Did Elena mentor Marco?",
        "Marco refused to mentor Dev.",
    ] {
        assert!(
            extract_aggregates(text, &opts).unwrap().is_empty(),
            "suspended text produced a claim: {text:?}"
        );
    }
}

#[test]
fn a_passage_asserting_and_denying_one_fact_is_flagged_with_both_spans() {
    let text = "Elena is Marco's sister. Later, Elena is not Marco's sister.";
    let opts = Options::default();

    let conflicts = find_conflicts(text, &opts).expect("extraction failed");
    assert_eq!(conflicts.len(), 1);

    let conflict = &conflicts[0];
    assert_eq!(conflict.kind, ConflictKind::Denial);
    assert_eq!(conflict.left.polarity, Polarity::Asserted);
    assert_eq!(conflict.right.polarity, Polarity::Denied);
    // Left is the claim the passage makes first, and both sides carry evidence.
    assert!(conflict.left.spans[0][0] < conflict.right.spans[0][0]);
    for aggregate in [&conflict.left, &conflict.right] {
        let span = aggregate.spans[0];
        assert!(text.get(span[0]..span[1]).is_some());
    }
}

#[test]
fn two_compatible_facts_about_one_pair_are_not_a_conflict() {
    let opts = Options::default();

    // Compatible, and a stance that changes over the story is a character arc
    // rather than a continuity error: neither is reported.
    for text in [
        "Elena is Marco's sister. Elena is Marco's student.",
        "Elena is Marco's enemy. Later, Elena is Marco's ally.",
    ] {
        assert!(
            find_conflicts(text, &opts).unwrap().is_empty(),
            "invented a conflict in {text:?}"
        );
    }
}

#[test]
fn one_person_cannot_have_two_mothers() {
    let opts = Options::default();

    let conflicts = find_conflicts("Elena is Dev's mother. Sofia is Dev's mother.", &opts).unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, ConflictKind::Cardinality);
    assert_eq!(conflicts[0].left.subject, "elena");
    assert_eq!(conflicts[0].right.subject, "sofia");

    // The constraint binds the object, not the subject: one mother of two
    // children is ordinary.
    let siblings =
        find_conflicts("Elena is Dev's mother. Elena is Marco's mother.", &opts).unwrap();
    assert!(siblings.is_empty());

    // And it is per relation: siblings are not cardinality-constrained.
    let many = find_conflicts("Elena is Dev's sister. Sofia is Dev's sister.", &opts).unwrap();
    assert!(many.is_empty());
}

#[test]
fn a_pronoun_opening_a_sentence_can_take_the_previous_sentence_referent() {
    // "Elena" carries no title, so the only thing that can say she is a woman
    // is the document itself: the middle sentence names her and one subject
    // pronoun, which settles it, and the last sentence can then use it.
    let text = "The Archive hired Elena. \
                Elena said she would start at the Archive. \
                She works at it.";
    let candidates =
        extract_candidate_triples(text, &Options::default()).expect("extraction failed");
    let works_at = candidates
        .iter()
        .find(|c| c.subject == "elena" && c.object == "archive")
        .unwrap_or_else(|| panic!("no cross-sentence relation: {candidates:?}"));
    assert!(text.get(works_at.span[0]..works_at.span[1]).is_some());
}

#[test]
fn two_pronouns_in_one_sentence_do_not_take_the_same_referent() {
    // "She runs it" cannot mean Elena runs Elena. With no agreement check,
    // consuming each carried referent once is the only thing keeping "it" off
    // the person "she" just took.
    let aggregates = extract_aggregates(
        "Mrs. Bennet joined the Archive. She works at it.",
        &Options::default(),
    )
    .expect("extraction failed");

    assert!(aggregates.iter().all(|a| a.subject != a.object));
    assert!(
        aggregates
            .iter()
            .any(|a| a.subject == "mrs_bennet" && a.object == "archive"),
        "{aggregates:?}"
    );
}

#[test]
fn a_typographic_apostrophe_separates_a_mention_as_a_straight_one_does() {
    let opts = Options::default();
    let curly = extract_candidate_triples("Elena is Blanche\u{2019}s sister.", &opts).unwrap();
    let straight = extract_candidate_triples("Elena is Blanche's sister.", &opts).unwrap();

    assert_eq!(straight.len(), 1);
    assert_eq!(straight[0].object, "blanche");
    // The curly form must at least not fold the clitic into the name.
    assert!(curly.iter().all(|c| !c.object.contains('\u{2019}')));
}

#[test]
fn a_cross_sentence_pronoun_agrees_in_gender() {
    // Nearest-first would take Mr. Darcy, who is named first. The document
    // says he is male and that "she" is therefore not him.
    let text = "Mr. Darcy watched Mrs. Bennet. She works at the Archive.";
    let candidates =
        extract_candidate_triples(text, &Options::default()).expect("extraction failed");

    assert_eq!(candidates.len(), 1, "{candidates:?}");
    assert_eq!(candidates[0].subject, "mrs_bennet");
    assert_eq!(candidates[0].object, "archive");
}

#[test]
fn a_personal_pronoun_does_not_take_a_thing_and_it_does_not_take_a_person() {
    // "the Archive" is introduced with a determiner, so it is a thing: "he"
    // cannot be it and "it" cannot be Mr. Darcy.
    let text = "Mr. Darcy entered the Archive. He works at it.";
    let candidates =
        extract_candidate_triples(text, &Options::default()).expect("extraction failed");

    assert_eq!(candidates.len(), 1, "{candidates:?}");
    assert_eq!(candidates[0].subject, "mr_darcy");
    assert_eq!(candidates[0].object, "archive");

    // Neither pronoun has anything left to take, so nothing is claimed.
    let mismatched = "Mr. Darcy entered the Archive. She works at it.";
    let none = extract_candidate_triples(mismatched, &Options::default()).unwrap();
    assert!(
        none.iter()
            .all(|c| c.subject != "mr_darcy" && c.subject != "archive"),
        "{none:?}"
    );
}

#[test]
fn an_unestablished_capital_is_not_a_referent() {
    // "Behind" is capitalized only because the sentence starts there, and the
    // opener list is closed and does not hold it. It is not a name, so it is
    // not something a pronoun can mean.
    // A capitalized run swallows the words after it, so the opener has to be
    // followed by a lowercase word to stand alone as a mention.
    let text = "Penned in haste, the letter reached Mr. Darcy. He works at the Archive.";
    let candidates =
        extract_candidate_triples(text, &Options::default()).expect("extraction failed");

    assert!(
        candidates.iter().all(|c| c.subject != "behind"),
        "linked a pronoun to a sentence-opening word: {candidates:?}"
    );
    assert!(
        candidates.iter().any(|c| c.subject == "mr_darcy"),
        "{candidates:?}"
    );
}

#[test]
fn a_plural_pronoun_never_links_across_a_sentence() {
    // "They" names a group assembled over several sentences, which a carry
    // holding one referent at a time cannot represent.
    let text = "Mr. Darcy entered the Archive. They work at it.";
    let candidates =
        extract_candidate_triples(text, &Options::default()).expect("extraction failed");

    assert!(
        candidates.iter().all(|c| c.subject != "mr_darcy"),
        "a plural pronoun claimed a single referent: {candidates:?}"
    );
}

#[test]
fn a_comma_ends_a_name_so_a_list_is_not_one_person() {
    let text = "Dev, Marco's younger brother, left early.";
    let found = extract_candidate_triples(text, &Options::default()).expect("extraction failed");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (&*found[0].subject, &*found[0].relation, &*found[0].object),
        ("dev", "brother_of", "marco")
    );
    let names: Vec<_> = extract_entities("Nora, Dev and Marco came.", &Default::default())
        .into_iter()
        .map(|e| e.normalized)
        .collect();
    assert_eq!(names, ["nora", "dev", "marco"]);
}

#[test]
fn a_relation_is_not_read_across_a_quotation_mark() {
    for text in [
        "\"Papa's stuck at work,\" Nora says.",
        "\"Marco,\" Elena said, \"married Nora.\"",
        "Cass said, \"He married Dev.\"",
    ] {
        let found = extract_aggregates(text, &Options::default()).expect("extraction failed");
        assert!(found.is_empty(), "{text}: {found:?}");
    }
    // A statement made whole inside one quotation is still the speaker's claim.
    let spoken = extract_aggregates(
        "\"Elena is Marco's sister,\" Dev said.",
        &Options::default(),
    )
    .expect("extraction failed");
    assert_eq!(spoken.len(), 1);
}

#[test]
fn a_vocative_inside_speech_is_not_the_speaker_a_pronoun_names() {
    let text = "\"Thanks, Cass,\" she says. Then she kisses Hollis.";
    let found = extract_aggregates(text, &Options::default()).expect("extraction failed");
    assert!(
        found.iter().all(|a| a.subject != "cass"),
        "the listener was taken for the speaker: {found:?}"
    );
}

#[test]
fn a_sentence_opener_is_not_a_pronoun_antecedent() {
    let text = "Then she married Dev.";
    let found = extract_aggregates(text, &Options::default()).expect("extraction failed");
    assert!(found.iter().all(|a| a.subject != "then"), "{found:?}");
}

#[test]
fn a_phrase_rule_reads_only_the_exact_window_between_the_mentions() {
    let opts = Options::default();
    let claims = |text: &str| {
        extract_aggregates(text, &opts)
            .expect("extraction failed")
            .into_iter()
            .map(|a| (a.subject, a.relation, a.object, a.polarity))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        claims("Nora married Dev."),
        [(
            "nora".into(),
            "spouse_of".into(),
            "dev".into(),
            Polarity::Asserted
        )]
    );
    assert_eq!(
        claims("Nora never married Dev."),
        [(
            "nora".into(),
            "spouse_of".into(),
            "dev".into(),
            Polarity::Denied
        )]
    );
    for text in [
        "Nora met her friend Dev.",
        "Nora wanted to marry Dev.",
        "If Nora married Dev, she left.",
        "Did Nora marry Dev?",
        "Nora met Dev's sister.",
        "Nora loves Paris.",
        "Nora owns Dev.",
    ] {
        assert!(claims(text).is_empty(), "{text}: {:?}", claims(text));
    }
}

#[test]
fn a_narrator_context_resolves_the_first_person_outside_speech_only() {
    use narrative_graph::extract_aggregates_in;
    use narrative_graph::heuristic::Context;

    let context = Context::default().with_narrator("wren");
    let opts = Options::default();
    let found = extract_aggregates_in("I live with Cass.", &opts, &context).expect("failed");
    assert_eq!(found.len(), 1);
    assert_eq!(
        (&*found[0].subject, &*found[0].relation, &*found[0].object),
        ("wren", "lives_with", "cass")
    );
    // Without a narrator the first person is never a mention, as before.
    assert!(extract_aggregates("I live with Cass.", &opts)
        .expect("failed")
        .is_empty());
    let spoken = extract_aggregates_in("\"I live with Cass,\" Marco says.", &opts, &context)
        .expect("failed");
    assert!(spoken.is_empty(), "{spoken:?}");
}

#[test]
fn a_conditional_suspends_its_own_clause_in_either_polarity() {
    let opts = Options::default();
    for text in [
        "If Elena is Marco's sister, the will is void.",
        "If Elena is not Marco's sister, the will stands.",
        "\"If Elena is Marco's sister, say so.\"",
    ] {
        let aggregates = extract_aggregates(text, &opts).unwrap();
        assert!(aggregates.is_empty(), "{text:?} yielded {aggregates:?}");
    }
    // The suspender governs its clause, not one past a semicolon.
    let candidates =
        extract_candidate_triples("Dev might stay; Elena is Marco's sister.", &opts).unwrap();
    assert_eq!(candidates.len(), 1, "{candidates:?}");
}

#[test]
fn the_one_subject_constraint_follows_the_rule_not_the_ontology_label() {
    let ontology = |pairs: &[(&str, &str)]| Options {
        ontology: pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..Default::default()
    };
    let cardinality = |text: &str, opts: &Options| {
        find_conflicts(text, opts)
            .unwrap()
            .iter()
            .filter(|c| c.kind == ConflictKind::Cardinality)
            .count()
    };
    let mothers = "Ann is Zoe's mother. Bea is Zoe's mother.";
    assert_eq!(
        cardinality(mothers, &ontology(&[("mother_of", "parent")])),
        1
    );
    let sisters = "Ann is Zoe's sister. Bea is Zoe's sister.";
    assert_eq!(
        cardinality(sisters, &ontology(&[("sister_of", "mother_of")])),
        0
    );
    // Merged from a constrained and an unconstrained rule: unconstrained.
    let merged = "Ann is Zoe's mother. Bea is Zoe's sister. Ann is Zoe's sister.";
    let opts = ontology(&[("mother_of", "kin"), ("sister_of", "kin")]);
    assert_eq!(cardinality(merged, &opts), 0);
}

#[test]
fn analyze_returns_what_the_separate_calls_return() {
    let text = "Ann is Zoe's mother. Bea is Zoe's mother. Ann is not Zoe's mother. \
                Marco mentors Dev, who works at the Archive.";
    let opts = Options::default();
    let analysis = narrative_graph::analyze(text, &opts).unwrap();
    assert_eq!(
        format!("{:?}", analysis.candidates),
        format!("{:?}", extract_candidate_triples(text, &opts).unwrap())
    );
    assert_eq!(
        format!("{:?}", analysis.aggregates),
        format!("{:?}", extract_aggregates(text, &opts).unwrap())
    );
    assert_eq!(
        format!("{:?}", analysis.conflicts),
        format!("{:?}", find_conflicts(text, &opts).unwrap())
    );
    assert!(!analysis.conflicts.is_empty());
}

#[test]
fn an_apostrophe_before_a_capital_stays_inside_the_name() {
    for (text, subject) in [
        (
            "Goodwin O\u{2019}Leary is Eibhlín\u{2019}s grandson.",
            "goodwin_o\u{2019}leary",
        ),
        ("Liam O'Brien is Nora's brother.", "liam_o'brien"),
        (
            "Anne D\u{2019}Arcy is Blanche\u{2019}s sister.",
            "anne_d\u{2019}arcy",
        ),
    ] {
        let candidates = extract(text);
        assert_eq!(candidates.len(), 1, "{text}: {candidates:?}");
        assert_eq!(candidates[0].subject, subject, "{text}");
        assert!(!candidates[0].object.contains(['\'', '\u{2019}']), "{text}");
    }
}

#[test]
fn a_tagged_speaker_speaks_to_the_name_they_address() {
    let speaks = |text: &str| -> Vec<(String, String)> {
        extract(text)
            .into_iter()
            .filter(|c| c.relation == "speaks_to")
            .map(|c| (c.subject, c.object))
            .collect()
    };
    let pair = |s: &str, o: &str| vec![(s.to_string(), o.to_string())];

    assert_eq!(
        speaks("\u{201C}Cathy, are you busy this afternoon?\u{201D} asked Heathcliff.\n\u{201C}No,\u{201D} said Cathy.\n\u{201C}Why?\u{201D} asked Heathcliff."),
        pair("heathcliff", "cathy")
    );
    assert_eq!(
        speaks("\"Well, Emma,\" Knightley said. \"I am going.\"\n\"Go,\" said Emma.\n\"I will,\" Knightley said."),
        pair("knightley", "emma")
    );
    // One paragraph is one turn: the address in its second sentence counts.
    assert_eq!(
        speaks("\"I am tired,\" said Nora. \"Are you coming, Marco?\"\n\"Yes,\" said Marco.\n\"Good,\" said Nora."),
        pair("nora", "marco")
    );
    // A name the speech is about, not addressed to.
    assert!(speaks("\"I saw Marco at the dock,\" said Nora.").is_empty());
    // Two tagged speakers in one paragraph: the listener is unknown.
    assert!(speaks("\"Marco!\" said Nora, and Elena said, \"Hush, Dev.\"").is_empty());
    // A listener needs evidence of being a person: a speaker tag of their
    // own, or a title or pronoun elsewhere.
    assert!(speaks("\"Come, Lizzy,\" said Nora.\n\"Go,\" said Nora.").is_empty());
    // A new paragraph is a new turn.
    assert!(speaks("\"Go,\" said Nora.\n\"Marco, wait.\"").is_empty());
}

#[test]
fn a_two_person_conversation_carries_its_speakers_across_untagged_turns() {
    let speaks = |text: &str| -> Vec<(String, String, [usize; 2])> {
        extract_aggregates(text, &Options::default())
            .expect("extraction failed")
            .into_iter()
            .filter(|a| a.relation == "speaks_to")
            .flat_map(|a| {
                let (s, o) = (a.subject, a.object);
                a.spans
                    .into_iter()
                    .map(move |span| (s.clone(), o.clone(), span))
            })
            .collect()
    };
    let said_in = |found: &[(String, String, [usize; 2])], text: &str, line: &str| {
        let start = text.find(line).expect("line in text");
        found
            .iter()
            .filter(|(_, _, span)| (start..start + line.len()).contains(&span[0]))
            .map(|(s, o, _)| (s.as_str().to_owned(), o.as_str().to_owned()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    let pair = |s: &str, o: &str| vec![(s.to_string(), o.to_string())];
    // Each is tagged twice elsewhere, which is what makes them voices.
    let voices = "\nThe sun set.\nIt was late.\n\"Go,\" said Nora.\n\"No,\" said Marco.\n\"Go,\" said Nora.\n\"No,\" said Marco.";

    // The untagged third turn is Nora's again, said to Marco.
    let text =
        format!("\"Are you coming?\" asked Nora.\n\"No,\" said Marco.\n\"Why not?\"{voices}");
    let found = speaks(&text);
    assert_eq!(
        said_in(&found, &text, "\"Why not?\""),
        pair("nora", "marco")
    );
    assert_eq!(
        said_in(&found, &text, "\"No,\" said Marco."),
        pair("marco", "nora")
    );

    // An address names who answers, but each side needs a tag of its own
    // before untagged turns are given to it.
    let text = format!("\"Marco, are you coming?\" asked Nora.\n\"I am tired.\"{voices}");
    let found = speaks(&text);
    assert!(
        said_in(&found, &text, "\"I am tired.\"").is_empty(),
        "{found:?}"
    );
    let text = format!(
        "\"Marco, are you coming?\" asked Nora.\n\"I am tired.\"\n\"Then rest.\"\n\"I will,\" said Marco.{voices}"
    );
    let found = speaks(&text);
    assert_eq!(
        said_in(&found, &text, "\"I am tired.\""),
        pair("marco", "nora")
    );
    assert_eq!(
        said_in(&found, &text, "\"Then rest.\""),
        pair("nora", "marco")
    );

    // Speech left open at a paragraph's end is the same speaker's next paragraph.
    let text = format!(
        "\"I went to the dock,\" said Nora, \"and then I waited.\n\"Marco, I came home.\"{voices}"
    );
    let found = speaks(&text);
    assert_eq!(
        said_in(&found, &text, "\"Marco, I came home.\""),
        pair("nora", "marco")
    );

    // With a third voice in the conversation, an untagged turn is no one's.
    let text = format!("\"Are you coming?\" asked Nora.\n\"No,\" said Marco.\n\"Nor I,\" said Elena.\n\"Why not?\"\n\"Go,\" said Elena.{voices}");
    let found = speaks(&text);
    assert!(
        said_in(&found, &text, "\"Why not?\"").is_empty(),
        "{found:?}"
    );
}
