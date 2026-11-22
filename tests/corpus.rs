//! Whole-book output snapshots over the novels in `tests/corpus/`.
//!
//! Every aggregate and conflict each novel yields is written one per line to
//! `tests/corpus/snapshots/<book>.tsv`, so a rule change shows up in review as
//! the claims it added and the claims it lost. The test fails on any
//! difference; after checking the reported lines, regenerate with
//! `NG_BLESS=1 cargo test --release --test corpus -- --ignored`.
//!
//! Ignored by default because a debug build takes minutes over the six books;
//! CI runs it in release.

use narrative_graph::{analyze, AggregateTriple, Analysis, Options};
use std::collections::BTreeSet;
use std::path::Path;

const BOOKS: &[&str] = &["1342", "1260", "98", "2701", "768", "158"];
const SHOWN: usize = 50;

fn triple(a: &AggregateTriple) -> String {
    format!(
        "{}\t{}\t{}\t{:?}",
        a.subject, a.relation, a.object, a.polarity
    )
}

fn render(analysis: &Analysis) -> String {
    let mut out = String::new();
    for a in &analysis.aggregates {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            triple(a),
            a.spans.len(),
            a.spans[0][0],
            a.rules.join(",")
        ));
    }
    for c in &analysis.conflicts {
        out.push_str(&format!(
            "conflict\t{:?}\t{}\t{}\n",
            c.kind,
            triple(&c.left),
            triple(&c.right)
        ));
    }
    out
}

fn difference(expected: &str, actual: &str) -> String {
    let old: BTreeSet<&str> = expected.lines().collect();
    let new: BTreeSet<&str> = actual.lines().collect();
    let removed: Vec<_> = old.difference(&new).collect();
    let added: Vec<_> = new.difference(&old).collect();
    let mut report = format!("{} removed, {} added\n", removed.len(), added.len());
    for line in removed.iter().take(SHOWN) {
        report.push_str(&format!("- {line}\n"));
    }
    for line in added.iter().take(SHOWN) {
        report.push_str(&format!("+ {line}\n"));
    }
    if removed.is_empty() && added.is_empty() {
        report.push_str("same lines in a different order\n");
    }
    report
}

#[test]
#[ignore = "slow in debug; run with --release -- --ignored"]
fn whole_book_output_matches_the_snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let bless = std::env::var_os("NG_BLESS").is_some();
    let mut failures = Vec::new();

    for book in BOOKS {
        let text = std::fs::read_to_string(root.join(format!("{book}.txt")))
            .unwrap_or_else(|e| panic!("reading {book}.txt: {e}"));
        let actual = render(&analyze(&text, &Options::default()).expect("analysis failed"));
        let path = root.join(format!("snapshots/{book}.tsv"));

        if bless {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_default();
        if expected != actual {
            failures.push(format!("{book}: {}", difference(&expected, &actual)));
        }
    }

    assert!(
        failures.is_empty(),
        "output changed; if intended, rerun with NG_BLESS=1\n{}",
        failures.join("\n")
    );
}
