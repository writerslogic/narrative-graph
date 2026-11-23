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
//! matches one is not a false positive. A side in brackets is a person the
//! sentence only describes ("[her mother = Mrs. Bennet]"); those labels are a
//! third tier, scored the same way, and measure what alias and coreference
//! work would add.
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

/// How a label is known, each scored on its own so that the inferred and the
/// merely described never lower the score of what the sentence states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Stated,
    /// Prefixed `implied`: inferred by a reader, not said.
    Implied,
    /// A side in brackets: a person the sentence only describes ("her
    /// mother"), matched by the name after `=` when the labeller gave one.
    Described,
}

#[derive(Debug)]
struct Claim {
    /// `None` for a description with no name: unmatchable until the library
    /// can make a node of a description.
    subject: Option<String>,
    relation: String,
    object: Option<String>,
    polarity: Polarity,
    tier: Tier,
}

/// A plain name, or `[description]` / `[description = Name]`.
fn side(raw: &str) -> (Option<String>, bool) {
    match raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        Some(inner) => (
            inner
                .split_once('=')
                .map(|(_, name)| name.trim().to_string())
                .filter(|name| !name.is_empty()),
            true,
        ),
        None => (Some(raw.to_string()), false),
    }
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
            let (subject, subject_described) = side(parts[0]);
            let (object, object_described) = side(parts[2]);
            let tier = if subject_described || object_described {
                Tier::Described
            } else if implied {
                Tier::Implied
            } else {
                Tier::Stated
            };
            claims.push(Claim {
                subject,
                relation: parts[1].to_string(),
                object,
                polarity,
                tier,
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

/// Which items a run scores, from `NG_HALF`: `even` to tune on, `odd` for the
/// held-out half at a milestone, both when unset.
#[derive(Clone, Copy, Debug)]
enum Half {
    All,
    Even,
    Odd,
}

impl Half {
    fn keeps(self, number: usize) -> bool {
        match self {
            Half::All => true,
            Half::Even => number % 2 == 0,
            Half::Odd => number % 2 == 1,
        }
    }
}

fn half() -> Half {
    match std::env::var("NG_HALF").as_deref() {
        Ok("even") => Half::Even,
        Ok("odd") => Half::Odd,
        Ok("") | Ok("all") | Err(_) => Half::All,
        Ok(other) => panic!("NG_HALF is `even`, `odd` or `all`, got {other:?}"),
    }
}

/// Another extractor's output, scored by the same matcher: one line per
/// claim, `item | subject | relation | object`, the subject prefixed `not `
/// for a denial. Lines for items outside the scored half are ignored.
fn parse_predictions(raw: &str, items: &[Item]) -> Vec<AggregateTriple> {
    let mut out = Vec::new();
    for line in raw.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        assert!(
            parts.len() == 4,
            "a prediction is `item | A | relation | B`, got {line:?}"
        );
        let number: usize = parts[0].parse().expect("prediction item number");
        let Some(item) = items.iter().find(|i| i.number == number) else {
            continue;
        };
        let (polarity, subject) = match parts[1].strip_prefix("not ") {
            Some(rest) => (Polarity::Denied, rest),
            None => (Polarity::Asserted, parts[1]),
        };
        out.push(AggregateTriple {
            subject: subject.to_string(),
            relation: parts[2].to_string(),
            object: parts[3].to_string(),
            polarity,
            confidence: 1.0,
            spans: vec![[item.start, item.start]],
            rules: vec!["prediction".to_string()],
        });
    }
    out
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
    let (Some(subject), Some(object)) = (&claim.subject, &claim.object) else {
        return false;
    };
    let forward = same_person(subject, &a.subject) && same_person(object, &a.object);
    let backward = MUTUAL.contains(&a.relation.as_str())
        && same_person(subject, &a.object)
        && same_person(object, &a.subject);
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

/// Scores `labels.txt` in `root` against the novels beside it, prints the
/// report, and returns `(labelled, precision, recall over relations with a
/// rule)`.
fn evaluate(root: &Path, predictions: Option<&Path>) -> (bool, f64, f64) {
    let labels = std::fs::read_to_string(root.join("labels.txt")).expect("labels.txt");
    let (labelled, mut items) = parse(&labels);
    let half = half();
    items.retain(|item| half.keeps(item.number));
    println!("half: {half:?}");
    let predicted = predictions.map(|path| {
        let raw = std::fs::read_to_string(path).expect("predictions file");
        parse_predictions(&raw, &items)
    });
    let known = known_relations();

    let mut books: BTreeMap<&str, (String, Vec<AggregateTriple>)> = BTreeMap::new();
    for item in &items {
        books.entry(&item.book).or_insert_with(|| {
            let text = std::fs::read_to_string(root.join(format!("{}.txt", item.book)))
                .expect("novel text");
            let aggregates = match &predicted {
                Some(predicted) => predicted
                    .iter()
                    .filter(|a| {
                        items
                            .iter()
                            .any(|i| i.book == item.book && i.start == a.spans[0][0])
                    })
                    .cloned()
                    .collect(),
                None => extract_aggregates(&text, &Options::default()).expect("extraction"),
            };
            (text, aggregates)
        });
    }

    let mut overall = Tally::default();
    let mut known_missed = 0;
    let mut known_gold = 0;
    let mut stated_gold = 0;
    let mut other_tiers: BTreeMap<Tier, (usize, usize)> = BTreeMap::new();
    let mut by_relation: BTreeMap<String, Tally> = BTreeMap::new();
    let mut report = Vec::new();
    // (labels, found, labels with a nameless side, which nothing can match)
    let mut pooled = (0, 0, 0);

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
            pooled.0 += 1;
            pooled.1 += usize::from(found);
            pooled.2 += usize::from(claim.subject.is_none() || claim.object.is_none());
            if claim.tier != Tier::Stated {
                let (gold, hit) = other_tiers.entry(claim.tier).or_default();
                *gold += 1;
                *hit += usize::from(found);
                if !found {
                    report.push(format!(
                        "  item {} missed ({:?}): {claim:?}",
                        item.number, claim.tier
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
    let recall_known = ratio(known_gold - known_missed, known_gold);

    println!(
        "{} items, {gold} stated labels ({known_gold} with a rule); tp {} fp {} missed {}",
        items.len(),
        overall.tp,
        overall.fp,
        overall.missed
    );
    println!(
        "precision {precision:.3}  recall (relations with a rule) {recall_known:.3}  recall (all) {recall_all:.3}"
    );
    println!(
        "all tiers: {} labels, recall {:.3}; {} have a nameless side, ceiling {:.3}",
        pooled.0,
        ratio(pooled.1, pooled.0),
        pooled.2,
        ratio(pooled.0 - pooled.2, pooled.0)
    );
    for (tier, (gold, hit)) in &other_tiers {
        println!("{tier:?}: {gold} labels, recall {:.3}", ratio(*hit, *gold));
    }
    for (relation, t) in &by_relation {
        println!(
            "  {relation:<20} tp {:>3} fp {:>3} missed {:>3}",
            t.tp, t.fp, t.missed
        );
    }
    for line in &report {
        println!("{line}");
    }

    (labelled, precision, recall_known)
}

#[test]
#[ignore = "slow in debug; run with --release -- --include-ignored"]
fn real_text_precision_and_recall() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let predictions = std::env::var_os("NG_PREDICTIONS");
    let (labelled, precision, recall_known) =
        evaluate(&root, predictions.as_deref().map(Path::new));
    if !labelled {
        println!("labels are a draft; not scored against a floor");
        return;
    }
    if predictions.is_some() {
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

/// The same scoring over a corpus that cannot be committed: contemporary
/// novels under copyright. `NG_PRIVATE_EVAL` names a directory holding
/// `labels.txt` and the `<book>.txt` files its items refer to. Reported, never
/// gated, because CI does not have it.
#[test]
#[ignore = "needs NG_PRIVATE_EVAL; run with --release -- --include-ignored"]
fn private_text_precision_and_recall() {
    let Some(root) = std::env::var_os("NG_PRIVATE_EVAL") else {
        println!("NG_PRIVATE_EVAL is not set; skipped");
        return;
    };
    let predictions = std::env::var_os("NG_PRIVATE_PREDICTIONS");
    evaluate(Path::new(&root), predictions.as_deref().map(Path::new));
}
