use narrative_graph::{
    analyze_document, extract_candidate_triples, ChapterPosition, EventKind, Gender, Options,
    Polarity, RelationshipState, Series,
};

#[test]
fn dialogue_counts_complete_utterances_with_exact_source_spans() {
    let text = concat!(
        "“Marco, follow me.\nWe’re late 🦊,” said Nora. “Nora, wait,” said Marco.\n\n",
        "“Hurry,” said Nora to Marco, “or we'll miss it.”\n\n",
        "Nora read “The Times” yesterday.\n\n",
        "The room fell quiet.\n\n",
        "Someone asked, “What happened?”",
    );
    let doc = analyze_document(text, &Options::default()).unwrap();
    let lines = &doc.dialogue.lines;
    assert_eq!(lines.len(), 4, "{lines:?}");
    let source = |[start, end]: [usize; 2]| &text[start..end];
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.speaker.as_deref(), line.listener.as_deref()))
            .collect::<Vec<_>>(),
        [
            (Some("nora"), Some("marco")),
            (Some("marco"), Some("nora")),
            (Some("nora"), Some("marco")),
            (None, None),
        ]
    );
    assert_eq!(source(lines[0].span), "“Marco, follow me.\nWe’re late 🦊,”");
    assert_eq!(lines[0].spoken_spans, [lines[0].span]);
    assert_eq!(source(lines[1].span), "“Nora, wait,”");
    assert_eq!(
        lines[2]
            .spoken_spans
            .iter()
            .copied()
            .map(source)
            .collect::<Vec<_>>(),
        ["“Hurry,”", "“or we'll miss it.”"]
    );
    assert_eq!(source(lines[3].span), "“What happened?”");
    assert_eq!(doc.dialogue.unattributed, 1);
}

#[test]
fn dialogue_keeps_repeated_and_partially_attributed_turns() {
    let text = concat!(
        "\"Can you stay?\" asked Nora.\n",
        "\"Yes,\" said Marco.\n",
        "\"Good.\"\n",
        "\"Thank you.\"\n",
        "Night fell.\n",
        "The room fell quiet.\n",
        "\"Enough,\" said Elena.",
    );
    let dialogue = analyze_document(text, &Options::default())
        .unwrap()
        .dialogue;
    assert_eq!(
        dialogue
            .lines
            .iter()
            .map(|line| (line.speaker.as_deref(), line.listener.as_deref()))
            .collect::<Vec<_>>(),
        [
            (Some("nora"), Some("marco")),
            (Some("marco"), Some("nora")),
            (Some("nora"), Some("marco")),
            (Some("marco"), Some("nora")),
            (Some("elena"), None),
        ]
    );
    assert_eq!(dialogue.unattributed, 1);
    assert!(dialogue
        .lines
        .windows(2)
        .all(|pair| pair[0].span[1] <= pair[1].span[0]));
}

#[test]
fn a_caller_alias_can_identify_a_listener_who_only_appears_in_speech() {
    let opts = Options {
        aliases: [
            ("Dev".into(), "devon".into()),
            ("Elena".into(), "captain".into()),
        ]
        .into(),
        ..Options::default()
    };
    let doc = analyze_document("“Dev!” cried Elena.", &opts).unwrap();
    assert_eq!(doc.dialogue.lines.len(), 1);
    let line = &doc.dialogue.lines[0];
    assert_eq!(line.speaker.as_deref(), Some("captain"));
    assert_eq!(line.listener.as_deref(), Some("devon"));
    assert_eq!(doc.dialogue.unattributed, 0);
}

#[test]
fn named_aliases_and_gender_cues_feed_every_relation_rule() {
    let text = concat!(
        "Fitzwilliam Darcy said he would stay. Mrs. Bennet waved at Mr. Darcy. ",
        "Darcy waited. Outside, the snow fell. He mentored Nora. ",
        "Mrs. Bennet told Mr. Darcy that she knew Dev.",
    );
    let document = analyze_document(text, &Options::default()).unwrap();
    let darcy = document
        .characters
        .iter()
        .find(|p| p.id == "fitzwilliam_darcy")
        .unwrap();
    assert_eq!(darcy.gender, Some(Gender::Masculine));
    for alias in ["Fitzwilliam Darcy", "Mr. Darcy", "Darcy"] {
        assert!(darcy.aliases.iter().any(|a| a == alias), "{darcy:?}");
    }
    let claims = &document.analysis.candidates;
    assert!(
        claims.iter().any(|c| c.subject == darcy.id
            && c.relation == "mentors"
            && c.object == "nora"
            && &text[c.span[0]..c.span[1]] == "He mentored Nora"),
        "{claims:?}"
    );
    assert!(
        claims
            .iter()
            .any(|c| c.subject == "mrs_bennet" && c.relation == "knows" && c.object == "dev"),
        "{claims:?}"
    );
}

#[test]
fn a_chapter_heading_cannot_replace_a_character_name() {
    let text = "Chapter IX\n\nEmma said she would attend. Emma met Nora.";
    let claims = extract_candidate_triples(text, &Options::default()).unwrap();
    assert!(
        claims
            .iter()
            .any(|c| c.subject == "emma" && c.relation == "meets"),
        "{claims:?}"
    );
}

#[test]
fn a_surname_cannot_merge_an_unconfirmed_family_member() {
    let text =
        "Miss Pross read. Solomon Pross arrived. Miss Pross met Nora. Solomon Pross knew Dev.";
    let doc = analyze_document(text, &Options::default()).unwrap();
    for (name, relation, object) in [
        ("miss_pross", "meets", "nora"),
        ("solomon_pross", "knows", "dev"),
    ] {
        assert!(
            doc.analysis
                .candidates
                .iter()
                .any(|c| c.subject == name && c.relation == relation && c.object == object),
            "{:?}",
            doc.analysis.candidates
        );
    }
}

#[test]
fn explicit_descriptions_and_family_links_resolve_original_surface_spans() {
    let text = concat!(
        "Élodie met the old man. Her sister mentored Dev. ",
        "Mr. Hale, the old man, waved. Élodie said she would come. Nora is Élodie’s sister. ",
        "Élodie phoned her sister. The old man married Cass.",
    );
    let document = analyze_document(text, &Options::default()).unwrap();
    let claims = &document.analysis.candidates;
    for (subject, relation, object, source) in [
        ("élodie", "meets", "mr_hale", "Élodie met the old man"),
        ("nora", "mentors", "dev", "Her sister mentored Dev"),
        ("élodie", "contacts", "nora", "Élodie phoned her sister"),
        ("mr_hale", "spouse_of", "cass", "The old man married Cass"),
    ] {
        assert!(
            claims.iter().any(|c| c.subject == subject
                && c.relation == relation
                && c.object == object
                && &text[c.span[0]..c.span[1]] == source),
            "{claims:?}"
        );
    }
    let nora = document.characters.iter().find(|p| p.id == "nora").unwrap();
    assert_eq!(nora.gender, Some(Gender::Feminine));
    assert!(nora
        .family
        .iter()
        .any(|f| f.role == "sister" && f.relative == "élodie"));
}

#[test]
fn a_nested_family_description_follows_only_supported_links() {
    let text = "Nora is Mrs. Shaw's sister. Mrs. Shaw waved. The sister of her sister met Dev.";
    let claims = extract_candidate_triples(text, &Options::default()).unwrap();
    assert!(
        claims
            .iter()
            .any(|c| c.subject == "mrs_shaw" && c.relation == "meets" && c.object == "dev"),
        "{claims:?}"
    );
    let ambiguous = "Nora is Mrs. Shaw's sister. Elena is Nora's sister. Mrs. Shaw waved. The sister of her sister met Dev.";
    let claims = extract_candidate_triples(ambiguous, &Options::default()).unwrap();
    assert!(claims.iter().all(|c| c.relation != "meets"), "{claims:?}");
}

#[test]
fn a_plural_family_statement_identifies_both_relatives() {
    let text = "Nora and Elena are sisters. Elena called. Her sister trained Dev.";
    let document = analyze_document(text, &Options::default()).unwrap();
    let elena = document
        .characters
        .iter()
        .find(|p| p.id == "elena")
        .unwrap();
    assert_eq!(elena.gender, Some(Gender::Feminine));
    assert!(elena
        .family
        .iter()
        .any(|f| f.relative == "nora" && f.role == "sister"));
    assert!(document
        .analysis
        .candidates
        .iter()
        .any(|c| c.subject == "nora" && c.object == "dev"));
}

#[test]
fn the_dialogue_vote_names_the_narrator_for_nondialogue_relations() {
    let text = concat!(
        "\"Wren, come,\" said Nora.\n",
        "\"Yes,\" I said.\n",
        "\"Wren, wait,\" said Nora.\n",
        "\"Fine,\" I replied.\n",
        "\"Wren, stay,\" said Nora.\n",
        "I met Dev.",
    );
    let claims = extract_candidate_triples(text, &Options::default()).unwrap();
    assert!(
        claims
            .iter()
            .any(|c| c.subject == "wren" && c.relation == "meets" && c.object == "dev"),
        "{claims:?}"
    );
}

#[test]
fn calibrated_thresholds_use_labeled_observations_and_skip_unsupported_rules() {
    let document =
        analyze_document("Nora met Dev. Nora mentored Dev.", &Options::default()).unwrap();
    let estimated = document
        .timeline
        .iter()
        .find(|f| f.candidate.relation == "meets")
        .unwrap();
    let score = estimated.calibrated_confidence.unwrap();
    assert_eq!(
        document
            .facts_at_least(score - f32::EPSILON)
            .unwrap()
            .count(),
        1
    );
    assert_eq!(document.facts_at_least(score).unwrap().count(), 1);
    assert_eq!(
        document
            .facts_at_least(score + f32::EPSILON)
            .unwrap()
            .count(),
        0
    );
    assert!(document.facts_at_least(f32::NAN).is_err());
}

#[test]
fn ambiguous_mentions_require_a_named_referent_and_preserve_quote_scope() {
    let options = Options::default();
    for text in [
        "Mr. Hale, the old man, sat. Mr. Fox, the old man, left. The old man mentored Dev.",
        "Nora is Mrs. Shaw's sister. Elena is Mrs. Shaw's sister. Mrs. Shaw called. Her sister mentored Dev.",
        "Mr. Hale greeted Mr. Fox. He met Nora.",
        "Mr. Hale left. She mentored Dev.",
        "Mr. Hale said, \"He mentored Dev.\"",
        "If Nora is Mrs. Shaw's sister, say so. Mrs. Shaw called. Her sister mentored Dev.",
        "Nora is not Mrs. Shaw's sister. Mrs. Shaw called. Her sister mentored Dev.",
        "Mr. Shaw watched. The letter was for his daughter, Cass was to write it.",
    ] {
        let claims = extract_candidate_triples(text, &options).unwrap();
        assert!(claims.iter().all(|c| !matches!(c.relation.as_str(), "mentors" | "meets" | "daughter_of")), "{text:?}: {claims:?}");
    }
}

#[test]
fn repeated_nearby_pronouns_cannot_overrule_a_title() {
    let text = format!(
        "Mr. Hale waved. {}Hale arrived. She mentored Dev.",
        "Hale heard she would attend. ".repeat(9)
    );
    let opts = Options {
        aliases: [
            ("Mr Hale".to_string(), "hale".to_string()),
            ("Hale".to_string(), "hale".to_string()),
        ]
        .into(),
        ..Options::default()
    };
    let doc = analyze_document(&text, &opts).unwrap();
    let hale = doc.characters.iter().find(|p| p.id == "hale").unwrap();
    assert_eq!(hale.gender, Some(Gender::Masculine));
    assert!(doc
        .analysis
        .candidates
        .iter()
        .all(|c| c.relation != "mentors"));
}

#[test]
fn a_gazetteer_place_cannot_be_a_personal_pronouns_antecedent() {
    let text = "She preferred Germany because she knew the roads. Nora met her.";
    let doc = analyze_document(text, &Options::default()).unwrap();
    assert!(
        doc.analysis.candidates.is_empty(),
        "{:?}",
        doc.analysis.candidates
    );
}

#[test]
fn relationship_states_keep_each_observation_in_story_order() {
    let text = concat!(
        "Nora was engaged to Dev. Dev married Nora. Nora is Dev’s widow. ",
        "Nora is Dev's friend. Nora is no longer Dev's friend. Nora is Dev's enemy.",
    );
    let mut options = Options::default();
    options
        .ontology
        .insert("spouse_of".to_string(), "wed".to_string());
    let doc = analyze_document(text, &options).unwrap();
    let states: Vec<_> = doc
        .timeline
        .iter()
        .filter_map(|entry| Some((entry.state?, entry.polarity)))
        .collect();
    assert_eq!(
        states,
        [
            (RelationshipState::Engaged, Polarity::Asserted),
            (RelationshipState::Married, Polarity::Asserted),
            (RelationshipState::Widowed, Polarity::Asserted),
            (RelationshipState::Friends, Polarity::Asserted),
            (RelationshipState::Friends, Polarity::Denied),
            (RelationshipState::Enemies, Polarity::Asserted),
        ]
    );
    assert_eq!(doc.timeline[1].candidate.relation, "wed");
    assert!(doc
        .timeline
        .windows(2)
        .all(|w| w[0].candidate.span < w[1].candidate.span));
    assert!(
        doc.analysis.conflicts.is_empty(),
        "{:?}",
        doc.analysis.conflicts
    );
}

#[test]
fn event_occurrences_use_resolved_actors_targets_and_locations() {
    let text = concat!(
        "Ms. Sol waved. Her sister is Nora. She went to Paris. ",
        "Nora proposed to Dev at the Hollow Alder Diner. Dev dueled Mr. Hale in Paris. ",
        "Mr. Hale died. Mr. Hale did not die in Paris. Did Nora propose to Dev? ",
        "If Nora died, Dev would leave. Nora proposed to Dev that they leave.",
    );
    let doc = analyze_document(text, &Options::default()).unwrap();
    let events: Vec<_> = doc
        .events
        .iter()
        .map(|e| {
            (
                e.kind,
                e.actor.as_str(),
                e.target.as_deref(),
                e.location.as_deref(),
                &text[e.span[0]..e.span[1]],
            )
        })
        .collect();
    assert_eq!(
        events,
        [
            (
                EventKind::Journey,
                "ms_sol",
                None,
                Some("paris"),
                "She went to Paris"
            ),
            (
                EventKind::Proposal,
                "nora",
                Some("dev"),
                Some("hollow_alder_diner"),
                "Nora proposed to Dev at the Hollow Alder Diner"
            ),
            (
                EventKind::Duel,
                "dev",
                Some("mr_hale"),
                Some("paris"),
                "Dev dueled Mr. Hale in Paris"
            ),
            (EventKind::Death, "mr_hale", None, None, "Mr. Hale died"),
        ]
    );
}

#[test]
fn knowledge_links_ground_the_recipient_and_preserve_the_claim() {
    let text = concat!(
        "Ms. Reed waved. Ms. Shaw told her about Dev’s departure. ",
        "Ms. Reed learned that the lights had gone out. Ms. Shaw knows Dev. ",
        "Ms. Reed learned that Dev had gone. Ms. Shaw knows that Dev is away.",
    );
    let doc = analyze_document(text, &Options::default()).unwrap();
    assert_eq!(doc.knowledge.len(), 4, "{:?}", doc.knowledge);
    let told = &doc.knowledge[0];
    assert_eq!(told.actor, "ms_shaw");
    assert_eq!(told.recipient.as_deref(), Some("ms_reed"));
    assert_eq!(told.topic.as_deref(), Some("dev"));
    assert_eq!(&text[told.content[0]..told.content[1]], "Dev’s departure");
    let learned = &doc.knowledge[1];
    assert_eq!(learned.topic, None);
    assert_eq!(
        &text[learned.content[0]..learned.content[1]],
        "the lights had gone out"
    );
    for (subject, relation, object) in [
        ("ms_shaw", "speaks_to", "ms_reed"),
        ("ms_shaw", "told_about", "dev"),
        ("ms_shaw", "knows", "dev"),
        ("ms_reed", "learned_about", "dev"),
        ("ms_shaw", "knows_about", "dev"),
    ] {
        assert!(
            doc.analysis
                .candidates
                .iter()
                .any(|c| c.subject == subject && c.relation == relation && c.object == object),
            "{:?}",
            doc.analysis.candidates
        );
    }
    for text in [
        "Nora did not learn that Dev left.",
        "If Nora learned that Dev left, she would follow.",
        "Did Nora tell Dev about Cass?",
        "Nora told him about Dev.",
        "Everyone knows that Dev left.",
        "Ms. Reed waited. Ms. Shaw and Nora told her about Dev.",
    ] {
        let doc = analyze_document(text, &Options::default()).unwrap();
        assert!(doc.knowledge.is_empty(), "{text:?}: {:?}", doc.knowledge);
    }
    let claims = extract_candidate_triples("Nora knew Dev had left.", &Options::default()).unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].relation, "knows_about");
}

#[test]
fn revising_a_chapter_updates_only_its_evidence_and_keeps_series_ids() {
    let mut series = Series::new(Options {
        aliases: [
            ("Mr Darcy".to_string(), "darcy".to_string()),
            ("Fitzwilliam Darcy".to_string(), "darcy".to_string()),
        ]
        .into(),
        ..Options::default()
    });
    let first = ChapterPosition {
        book: 1,
        chapter: 5,
    };
    let next = ChapterPosition {
        book: 2,
        chapter: 1,
    };
    assert!(series
        .update_chapter(next, 1, "Fitzwilliam Darcy married Nora.")
        .unwrap());
    assert!(series
        .update_chapter(first, 1, "Mr. Darcy married Nora.")
        .unwrap());
    assert!(!series
        .update_chapter(first, 1, "Mr. Darcy married Nora.")
        .unwrap());
    let agg = series.aggregates();
    assert_eq!(agg.len(), 1);
    assert_eq!(agg[0].subject, "darcy");
    assert_eq!(
        agg[0].spans.iter().map(|s| s.position).collect::<Vec<_>>(),
        [first, next]
    );
    assert_eq!(
        series
            .characters()
            .iter()
            .find(|p| p.id == "darcy")
            .unwrap()
            .aliases,
        ["Fitzwilliam Darcy", "Mr. Darcy"]
    );

    assert!(series
        .update_chapter(first, 2, "Mr. Darcy was engaged to Nora.")
        .unwrap());
    assert_eq!(
        series
            .relationship_history("nora", "darcy")
            .map(|(chapter, fact)| (chapter, fact.state.unwrap()))
            .collect::<Vec<_>>(),
        [
            (first, RelationshipState::Engaged),
            (next, RelationshipState::Married)
        ]
    );
    let agg = series.aggregates();
    assert_eq!(
        agg.iter()
            .map(|a| (&*a.relation, a.spans[0].position))
            .collect::<Vec<_>>(),
        [("engaged_to", first), ("spouse_of", next)]
    );
    assert!(series.remove_chapter(next));
    assert!(!series.remove_chapter(next));
    assert_eq!(series.aggregates().len(), 1);
}
