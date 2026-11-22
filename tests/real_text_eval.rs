//! Precision and recall against hand labels on sentences from real novels.
//!
//! `tests/corpus/labels.txt` holds 150 sentences drawn from the six novels by
//! a fixed seed, before the library was run on them: 7 per book containing a
//! relation word (in the library's vocabulary or not), 12 naming two or more
//! people, and 6 at random. Each lists the relations it states.
//!
//! Each novel is extracted whole, the way a caller reads a manuscript, and an
//! aggregate belongs to a labelled sentence when one of its spans starts
//! inside it. Names match after lowercasing, when one name's words begin or end
//! the other's ("Jane" and "Jane Bennet"), and only when their titles agree or
//! one has none, so "Mr. Bennet" never matches "Mrs. Bennet". A character
//! named two unrelated ways ("Lizzy", "Elizabeth") counts as a miss: alias
//! resolution is not built yet.
//!
//! Recall over stated labels is reported twice: over labels whose relation the
//! library has a rule for, and over all of them. The gap between them is
//! missing vocabulary; the first is what the rules get wrong. Labels marked
//! `implied` are what a reader infers rather than what the sentence says; they
//! get their own recall and never lower the other two, and an extraction that
//! matches one is not a false positive.
//!
//! Run `cargo test --release --test real_text_eval -- --include-ignored
//! --nocapture` for the numbers, per relation and per miss.

use narrative_graph::heuristic::pack::ENGLISH;
use narrative_graph::{extract_aggregates, AggregateTriple, Options, Polarity};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Set once the labels land; until then a lower score cannot fail the build.
const FLOOR: Option<(f64, f64)> = None;

const MUTUAL: &[&str] = &[
    "acquaintance_of",
    "ally_of",
    "argues_with",
    "broke_up_with",
    "classmate_of",
    "colleague_of",
    "companion_of",
    "cousin_of",
    "coworker_of",
    "dating",
    "divorced",
    "enemy_of",
    "engaged_to",
    "friend_of",
    "knows",
    "lives_with",
    "lover_of",
    "meets",
    "near",
    "neighbor_of",
    "partner_of",
    "rival_of",
    "roommate_of",
    "sibling_of",
    "spouse_of",
    "twin_of",
    "works_with",
];

#[derive(Debug)]
struct Claim {
    subject: String,
    relation: String,
    object: String,
    polarity: Polarity,
    implied: bool,
}

struct Item {
    number: usize,
    book: String,
    start: usize,
    sentence: String,
    claims: Vec<Claim>,
}

fn parse(labels: &str) -> (bool, Vec<Item>) {
    let mut labelled = None;
    let mut items: Vec<Item> = Vec::new();
    let mut lines = labels.lines().peekable();

    while let Some(line) = lines.next() {
        if let Some(status) = line.strip_prefix("status:") {
            labelled = Some(status.trim() == "labelled");
            continue;
        }
        let Some(header) = line.strip_prefix("@ ") else {
            assert!(
                line.starts_with('#') || line.trim().is_empty(),
                "unexpected line outside an item: {line:?}"
            );
            continue;
        };
        let fields: Vec<&str> = header.split_whitespace().collect();
        let number = fields[0].parse().expect("item number");
        let context = lines.next().expect("context line");
        assert!(context.starts_with("~ "), "item {number}: missing `~` line");
        let sentence = lines.next().expect("sentence line").to_string();
        assert_eq!(lines.next(), Some("="), "item {number}: missing `=` line");

        let mut claims = Vec::new();
        while let Some(line) = lines.next_if(|l| !l.starts_with("@ ")) {
            let line = line.trim().trim_end_matches('?').trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (implied, line) = match line.strip_prefix("implied ") {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            let (polarity, rest) = match line.strip_prefix("not ") {
                Some(rest) => (Polarity::Denied, rest),
                None => (Polarity::Asserted, line),
            };
            let parts: Vec<&str> = rest.split('|').map(str::trim).collect();
            assert!(
                parts.len() == 3 && parts.iter().all(|p| !p.is_empty()),
                "item {number}: a claim is `A | relation | B`, got {line:?}"
            );
            claims.push(Claim {
                subject: parts[0].to_string(),
                relation: parts[1].to_string(),
                object: parts[2].to_string(),
                polarity,
                implied,
            });
        }

        items.push(Item {
            number,
            book: fields[1].to_string(),
            start: fields[2].parse().expect("byte offset"),
            sentence,
            claims,
        });
    }

    (labelled.expect("labels have a `status:` line"), items)
}

/// The byte range in `text` that the whitespace-collapsed `sentence` covers,
/// starting at `start`.
fn locate(text: &str, start: usize, sentence: &str) -> Option<usize> {
    let mut want = sentence.chars().peekable();
    let mut in_space = false;
    for (offset, c) in text[start..].char_indices() {
        if want.peek().is_none() {
            return Some(start + offset);
        }
        if c.is_whitespace() {
            if !in_space {
                in_space = true;
                if want.next() != Some(' ') {
                    return None;
                }
            }
            continue;
        }
        in_space = false;
        if want.next() != Some(c) {
            return None;
        }
    }
    want.peek().is_none().then_some(text.len())
}

struct Name {
    title: Option<String>,
    words: Vec<String>,
}

fn name(raw: &str) -> Name {
    let mut words: Vec<String> = raw
        .to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '_' || c == '.')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect();
    let title = (words.len() > 1 && ENGLISH.honorifics.contains(&words[0].as_str()))
        .then(|| words.remove(0));
    Name { title, words }
}

fn same_person(gold: &str, extracted: &str) -> bool {
    let (a, b) = (name(gold), name(extracted));
    if let (Some(x), Some(y)) = (&a.title, &b.title) {
        if x != y {
            return false;
        }
    }
    let (short, long) = if a.words.len() <= b.words.len() {
        (&a.words, &b.words)
    } else {
        (&b.words, &a.words)
    };
    !short.is_empty() && (long.starts_with(short) || long.ends_with(short))
}

fn matches(claim: &Claim, a: &AggregateTriple) -> bool {
    if claim.relation != a.relation || claim.polarity != a.polarity {
        return false;
    }
    let forward = same_person(&claim.subject, &a.subject) && same_person(&claim.object, &a.object);
    let backward = MUTUAL.contains(&a.relation.as_str())
        && same_person(&claim.subject, &a.object)
        && same_person(&claim.object, &a.subject);
    forward || backward
}

fn known_relations() -> BTreeSet<&'static str> {
    let pack = &ENGLISH;
    pack.possessive_nouns
        .iter()
        .chain(pack.plural_nouns)
        .map(|n| n.1)
        .chain(pack.verb_rules.iter().map(|r| r.relation))
        .chain(pack.relative_rules.iter().map(|r| r.relation))
        .chain(pack.phrase_rules.iter().map(|r| r.relation))
        .collect()
}

#[derive(Default)]
struct Tally {
    tp: usize,
    fp: usize,
    missed: usize,
}

fn ratio(n: usize, d: usize) -> f64 {
    if d == 0 {
        1.0
    } else {
        n as f64 / d as f64
    }
}

#[test]
#[ignore = "slow in debug; run with --release -- --include-ignored"]
fn real_text_precision_and_recall() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let labels = std::fs::read_to_string(root.join("labels.txt")).expect("labels.txt");
    let (labelled, items) = parse(&labels);
    let known = known_relations();

    let mut books: BTreeMap<&str, (String, Vec<AggregateTriple>)> = BTreeMap::new();
    for item in &items {
        books.entry(&item.book).or_insert_with(|| {
            let text = std::fs::read_to_string(root.join(format!("{}.txt", item.book)))
                .expect("novel text");
            let aggregates = extract_aggregates(&text, &Options::default()).expect("extraction");
            (text, aggregates)
        });
    }

    let mut overall = Tally::default();
    let mut known_missed = 0;
    let mut known_gold = 0;
    let mut stated_gold = 0;
    let mut implied_gold = 0;
    let mut implied_found = 0;
    let mut by_relation: BTreeMap<String, Tally> = BTreeMap::new();
    let mut report = Vec::new();

    for item in &items {
        let (text, aggregates) = &books[item.book.as_str()];
        let end = locate(text, item.start, &item.sentence).unwrap_or_else(|| {
            panic!(
                "item {}: the sentence no longer matches {}.txt at byte {}",
                item.number, item.book, item.start
            )
        });
        let extracted: Vec<&AggregateTriple> = aggregates
            .iter()
            .filter(|a| a.spans.iter().any(|s| (item.start..end).contains(&s[0])))
            .collect();

        for a in &extracted {
            let tally = by_relation.entry(a.relation.clone()).or_default();
            if item.claims.iter().any(|c| matches(c, a)) {
                overall.tp += 1;
                tally.tp += 1;
            } else {
                overall.fp += 1;
                tally.fp += 1;
                report.push(format!(
                    "  item {} extra: {} {} {} {:?}",
                    item.number, a.subject, a.relation, a.object, a.polarity
                ));
            }
        }
        for claim in &item.claims {
            let found = extracted.iter().any(|a| matches(claim, a));
            if claim.implied {
                implied_gold += 1;
                implied_found += usize::from(found);
                if !found {
                    report.push(format!(
                        "  item {} missed (implied): {claim:?}",
                        item.number
                    ));
                }
                continue;
            }
            let is_known = known.contains(claim.relation.as_str());
            known_gold += usize::from(is_known);
            stated_gold += 1;
            if !found {
                overall.missed += 1;
                known_missed += usize::from(is_known);
                by_relation
                    .entry(claim.relation.clone())
                    .or_default()
                    .missed += 1;
                report.push(format!(
                    "  item {} missed{}: {claim:?}",
                    item.number,
                    if is_known { "" } else { " (no rule)" }
                ));
            }
        }
    }

    let precision = ratio(overall.tp, overall.tp + overall.fp);
    let gold = stated_gold;
    let recall_all = ratio(gold - overall.missed, gold);
    let recall_implied = ratio(implied_found, implied_gold);
    let recall_known = ratio(known_gold - known_missed, known_gold);

    println!(
        "{} items, {gold} stated labels ({known_gold} with a rule), {implied_gold} implied; tp {} fp {} missed {}",
        items.len(),
        overall.tp,
        overall.fp,
        overall.missed
    );
    println!(
        "precision {precision:.3}  recall (relations with a rule) {recall_known:.3}  recall (all) {recall_all:.3}  recall (implied) {recall_implied:.3}"
    );
    for (relation, t) in &by_relation {
        println!(
            "  {relation:<20} tp {:>3} fp {:>3} missed {:>3}",
            t.tp, t.fp, t.missed
        );
    }
    for line in &report {
        println!("{line}");
    }

    if !labelled {
        println!("labels are a draft; not scored against a floor");
        return;
    }
    if let Some((min_precision, min_recall)) = FLOOR {
        assert!(precision >= min_precision, "precision {precision:.3}");
        assert!(recall_known >= min_recall, "recall {recall_known:.3}");
    }
}

#[test]
fn names_match_across_titles_and_partial_names_but_not_across_sexes() {
    assert!(same_person("Jane", "jane_bennet"));
    assert!(same_person("Mr. Darcy", "darcy"));
    assert!(same_person("Elizabeth Bennet", "elizabeth"));
    assert!(!same_person("Mr. Bennet", "mrs_bennet"));
    assert!(!same_person("Lizzy", "elizabeth"));
}

#[test]
fn a_collapsed_sentence_is_located_across_line_breaks() {
    let text = "Before. Elena is\nMarco's   sister. After.";
    let start = text.find("Elena").unwrap();
    let end = locate(text, start, "Elena is Marco's sister.").unwrap();
    assert_eq!(&text[start..end], "Elena is\nMarco's   sister.");
    assert_eq!(locate(text, start, "Elena is Marco's brother."), None);
}
