use narrative_graph::{
    analyze_document, rule_calibration, CalibrationGroup, EventKind, MentionKind, Options,
    Polarity, RuleCalibration,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

type Span = [usize; 2];

#[derive(Default)]
struct Count {
    correct: usize,
    predicted: usize,
    gold: usize,
}

fn token_spans(original: &str, tokenized: &str) -> Vec<Vec<Span>> {
    let mut end = 0;
    tokenized
        .lines()
        .map(|line| {
            line.split_whitespace()
                .map(|token| {
                    let start = original.len() - original[end..].trim_start().len();
                    assert!(
                        original[start..].starts_with(token),
                        "unaligned token {token:?} at {start}"
                    );
                    end = start + token.len();
                    [start, end]
                })
                .collect()
        })
        .collect()
}

fn gold_people<'a>(
    annotations: &'a str,
    tokens: &[Vec<Span>],
) -> BTreeMap<(Span, MentionKind), &'a str> {
    let mut mentions = BTreeMap::new();
    let mut chains = BTreeMap::new();
    let mut descriptions = Vec::new();
    for line in annotations.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if let ["MENTION", id, sentence, first, end_sentence, last, _, "PER", kind] =
            fields.as_slice()
        {
            let index = |n: &str| n.parse::<usize>().expect("LitBank index");
            let span = [
                tokens[index(sentence)][index(first)][0],
                tokens[index(end_sentence)][index(last)][1],
            ];
            let kind = match *kind {
                "PROP" => MentionKind::Name,
                "PRON" => MentionKind::Pronoun,
                "NOM" => MentionKind::Description,
                _ => continue,
            };
            mentions.insert(*id, (span, kind));
        } else if let ["COREF", id, chain] = fields.as_slice() {
            chains.insert(*id, *chain);
        } else if let ["APPOS" | "COP", first, second] = fields.as_slice() {
            descriptions.push((*first, *second));
        }
    }
    for (a, b) in descriptions {
        for (name, description) in [(a, b), (b, a)] {
            if matches!(mentions.get(name), Some((_, MentionKind::Name)))
                && matches!(
                    mentions.get(description),
                    Some((_, MentionKind::Description))
                )
            {
                if let Some(&chain) = chains.get(name) {
                    chains.insert(description, chain);
                }
            }
        }
    }
    mentions
        .into_iter()
        .filter_map(|(id, mention)| Some((mention, *chains.get(id)?)))
        .collect()
}

#[test]
#[ignore = "requires the public LitBank repository, set NG_LITBANK_ROOT"]
fn named_character_links_and_event_triggers() {
    let Some(root) = std::env::var_os("NG_LITBANK_ROOT") else {
        println!("NG_LITBANK_ROOT is unset");
        return;
    };
    let root = Path::new(&root);
    let annotations_root = root.join("coref/tsv");
    let paths = std::fs::read_dir(&annotations_root).expect("LitBank coref/tsv");
    let mut totals = BTreeMap::<MentionKind, Count>::new();
    let mut events = BTreeMap::<&str, Count>::new();
    let mut books = 0;
    for path in paths {
        let path = path.expect("LitBank file").path();
        if path.extension().and_then(|e| e.to_str()) != Some("ann") {
            continue;
        }
        books += 1;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("LitBank filename");
        let name = stem.strip_suffix("_brat").expect("LitBank filename");
        let raw = std::fs::read_to_string(root.join("original").join(format!("{name}.txt")))
            .expect("original novel");
        let tokenized =
            std::fs::read_to_string(path.with_extension("txt")).expect("tokenized novel");
        let tokens = token_spans(&raw, &tokenized);
        let excerpt_end = tokens.iter().flatten().last().expect("nonempty excerpt")[1];
        let annotations = std::fs::read_to_string(&path).expect("LitBank annotations");
        let gold = gold_people(&annotations, &tokens);
        let named: BTreeSet<_> = gold
            .iter()
            .filter_map(|((_, kind), &chain)| (*kind == MentionKind::Name).then_some(chain))
            .collect();
        let analysis = analyze_document(&raw[..excerpt_end], &Options::default())
            .expect("extract LitBank excerpt");
        let mut ids = BTreeMap::<&str, BTreeSet<&str>>::new();
        for mention in analysis
            .mentions
            .iter()
            .filter(|m| m.kind == MentionKind::Name)
        {
            if let Some(chain) = gold.get(&(mention.span, MentionKind::Name)) {
                ids.entry(&mention.id).or_default().insert(chain);
            }
        }
        for ((_, kind), chain) in &gold {
            if *kind != MentionKind::Name && named.contains(chain) {
                totals.entry(*kind).or_default().gold += 1;
            }
        }
        for mention in analysis
            .mentions
            .iter()
            .filter(|m| m.kind != MentionKind::Name)
        {
            let count = totals.entry(mention.kind).or_default();
            count.predicted += 1;
            if let (Some(expected), Some(chains)) = (
                gold.get(&(mention.span, mention.kind)),
                ids.get(mention.id.as_str()),
            ) {
                count.correct += usize::from(chains.len() == 1 && chains.contains(expected));
            }
        }
        let event_rows =
            std::fs::read_to_string(root.join("events/tsv").join(format!("{stem}.tsv")))
                .expect("LitBank events");
        let labelled: Vec<_> = event_rows
            .lines()
            .filter_map(|line| line.rsplit_once('\t'))
            .collect();
        assert_eq!(labelled.len(), tokens.iter().flatten().count(), "{stem}");
        let gold_triggers: Vec<_> = labelled
            .iter()
            .zip(tokens.iter().flatten())
            .filter_map(|((_, label), &span)| (*label == "EVENT").then_some(span))
            .collect();
        for event in analysis.events {
            let kind = match event.kind {
                EventKind::Duel => "duel",
                EventKind::Proposal => "proposal",
                EventKind::Journey => "journey",
                EventKind::Death => "death",
            };
            let count = events.entry(kind).or_default();
            count.predicted += 1;
            count.correct += usize::from(
                gold_triggers
                    .iter()
                    .any(|&[start, end]| start < event.trigger[1] && event.trigger[0] < end),
            );
        }
    }
    assert_eq!(books, 100, "expected the complete LitBank corpus");
    println!("LitBank {books} excerpts");
    for (kind, count) in totals {
        println!(
            "{kind:?} links: {}/{} predicted, {} gold with a named anchor",
            count.correct, count.predicted, count.gold
        );
    }
    for (kind, count) in events {
        println!(
            "{kind} trigger support: {}/{}",
            count.correct, count.predicted
        );
    }
}

#[test]
#[ignore = "requires the public LitBank repository, set NG_LITBANK_ROOT"]
fn relation_calibration_matches_adjudications() {
    let Some(root) = std::env::var_os("NG_LITBANK_ROOT") else {
        return;
    };
    let original = Path::new(&root).join("original");
    let labels = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/litbank-calibration.tsv");
    let annotations = std::fs::read_to_string(labels).expect("relation adjudications");
    let mut books = BTreeMap::new();
    let mut counts = BTreeMap::<(&str, CalibrationGroup), (&str, u32, u32)>::new();
    let mut squared_errors = (0usize, 0., 0.);
    let mut unscored = 0;
    let mut changed = Vec::new();
    for line in annotations
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let columns: Vec<_> = line.split('\t').collect();
        let [split, book, start, end, subject, relation, object, polarity, rule, verdict] =
            columns.as_slice()
        else {
            panic!("malformed calibration row {line:?}");
        };
        let position = |s: &str| s.parse::<usize>().expect("byte position");
        let span = [position(start), position(end)];
        let expected_polarity = match *polarity {
            "A" => Polarity::Asserted,
            "D" => Polarity::Denied,
            _ => panic!("unexpected polarity"),
        };
        let document = books.entry(*book).or_insert_with(|| {
            let text = std::fs::read_to_string(original.join(format!("{book}.txt")))
                .expect("LitBank novel");
            analyze_document(&text, &Options::default()).expect("extract complete LitBank novel")
        });
        let Some(fact) = document.timeline.iter().find(|fact| {
            let c = &fact.candidate;
            c.span == span
                && c.subject == *subject
                && c.relation == *relation
                && c.object == *object
                && c.rule == *rule
                && fact.polarity == expected_polarity
        }) else {
            changed.push(format!("{book}:{start} {subject} {relation} {object}"));
            continue;
        };
        let correct = match *verdict {
            "yes" => true,
            "no" => false,
            "unclear" => {
                unscored += 1;
                continue;
            }
            _ => panic!("unexpected adjudication {verdict:?}"),
        };
        let group = CalibrationGroup::for_rule(rule).expect("recognized calibration group");
        let bucket = counts.entry((split, group)).or_insert((rule, 0, 0));
        bucket.1 += u32::from(correct);
        bucket.2 += 1;
        if let Some(estimate) = fact.calibrated_confidence.filter(|_| *split == "check") {
            let outcome = f64::from(u32::from(correct));
            squared_errors.0 += 1;
            squared_errors.1 += (f64::from(fact.candidate.confidence) - outcome).powi(2);
            squared_errors.2 += (f64::from(estimate) - outcome).powi(2);
        }
    }
    assert!(
        changed.is_empty(),
        "predictions changed:\n{}",
        changed.join("\n")
    );
    for ((split, group), (rule, correct, observations)) in counts {
        println!("LitBank calibration {split} {group:?}: {correct}/{observations} correct");
        if split == "fit" && observations >= 5 {
            assert_eq!(
                rule_calibration(rule),
                Some(RuleCalibration {
                    correct,
                    observations
                })
            );
        }
    }
    let (n, raw, calibrated) = squared_errors;
    assert!(
        n > 0,
        "expected held-out observations for calibrated groups"
    );
    println!("LitBank calibration held-out Brier over {n} predictions: raw {:.4}, calibrated {:.4}; {unscored} unclear across both splits",
        raw / n as f64, calibrated / n as f64);
}
