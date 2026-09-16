//! Draws a reproducible sample of real prose, extracts from it, and writes two
//! files: the full record for analysis, and an adjudication sheet carrying only
//! what a judge needs to rule on each triple.
//!
//! The split matters. `docs/EVALUATION.md` requires that whoever judges a
//! triple has not seen the patterns that produced it, so the adjudication sheet
//! omits the confidence score, the rule name, and everything else that would
//! reveal what the extractor expected to find.
//!
//! Usage:
//!   cargo run --release --features json --example precision_sample -- \
//!       --out DIR --sample N [--seed S] CORPUS.txt [CORPUS.txt ...]

use std::fs;
use std::path::PathBuf;

use narrative_graph::{extract_candidate_triples, Options};

const MIN_PARAGRAPH_BYTES: usize = 80;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = PathBuf::from("precision-sample");
    let mut sample = 200usize;
    let mut seed = 0x5eed_1e55_u64;
    let mut corpora: Vec<PathBuf> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                out = PathBuf::from(&args[i]);
            }
            "--sample" => {
                i += 1;
                sample = args[i].parse().expect("--sample takes a number");
            }
            "--seed" => {
                i += 1;
                seed = args[i].parse().expect("--seed takes a number");
            }
            other => corpora.push(PathBuf::from(other)),
        }
        i += 1;
    }
    assert!(!corpora.is_empty(), "no corpus files given");

    let mut units: Vec<(String, usize, String)> = Vec::new();
    for path in &corpora {
        let raw = fs::read_to_string(path).expect("corpus unreadable");
        let book = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        for (index, para) in paragraphs(strip_boilerplate(&raw)).into_iter().enumerate() {
            units.push((book.clone(), index, para));
        }
    }

    let picked = take_sample(units.len(), sample, seed);
    fs::create_dir_all(&out).expect("cannot create output dir");

    let mut sampled = Vec::new();
    let mut records = Vec::new();
    let mut sheet = String::new();
    let mut id = 0usize;
    let opts = Options::default();

    for unit in &picked {
        let (book, index, text) = &units[*unit];
        sampled.push(serde_json::json!({
            "book": book, "paragraph": index, "unit": unit, "text": text,
        }));

        let candidates = extract_candidate_triples(text, &opts).expect("extraction failed");
        for c in candidates {
            let span = &text[c.span[0]..c.span[1]];
            records.push(serde_json::json!({
                "id": id, "book": book, "paragraph": index,
                "subject": c.subject, "relation": c.relation, "object": c.object,
                "confidence": c.confidence, "rule": c.rule, "span": c.span, "span_text": span,
            }));
            sheet.push_str(
                &serde_json::json!({
                    "id": id, "sentence": span,
                    "claim": format!("{} {} {}", c.subject, c.relation, c.object),
                })
                .to_string(),
            );
            sheet.push('\n');
            id += 1;
        }
    }

    let summary = serde_json::json!({
        "corpora": corpora.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "paragraphs_available": units.len(),
        "paragraphs_sampled": picked.len(),
        "triples_emitted": id,
        "seed": seed,
        "min_paragraph_bytes": MIN_PARAGRAPH_BYTES,
    });

    write(
        &out.join("sampled.json"),
        &serde_json::to_string_pretty(&sampled).unwrap(),
    );
    write(
        &out.join("candidates.json"),
        &serde_json::to_string_pretty(&records).unwrap(),
    );
    write(&out.join("adjudication.jsonl"), &sheet);
    write(
        &out.join("summary.json"),
        &serde_json::to_string_pretty(&summary).unwrap(),
    );

    println!(
        "{} paragraphs sampled from {} available; {} triples emitted",
        picked.len(),
        units.len(),
        id
    );
}

fn write(path: &PathBuf, body: &str) {
    fs::write(path, body).expect("cannot write output");
}

/// Project Gutenberg wraps each text in a licence header and footer. Sampling
/// those would put the extractor to work on legal boilerplate.
fn strip_boilerplate(raw: &str) -> &str {
    let start = raw
        .find("*** START OF THE PROJECT GUTENBERG EBOOK")
        .and_then(|i| raw[i..].find('\n').map(|j| i + j + 1))
        .unwrap_or(0);
    let end = raw[start..]
        .find("*** END OF THE PROJECT GUTENBERG EBOOK")
        .map(|i| start + i)
        .unwrap_or(raw.len());
    &raw[start..end]
}

/// Blank-line separated blocks, rewrapped to one line each. The only filter is
/// a length floor, so that the sample is not shaped by a guess about which
/// prose the extractor handles well.
fn paragraphs(body: &str) -> Vec<String> {
    body.split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| p.len() >= MIN_PARAGRAPH_BYTES)
        .collect()
}

/// Seeded Fisher-Yates over the index range, so a given seed and corpus always
/// select the same units and a measurement can be re-derived after a rule change.
fn take_sample(len: usize, want: usize, seed: u64) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..len).collect();
    let mut state = seed | 1;
    for i in (1..len).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        idx.swap(i, (state % (i as u64 + 1)) as usize);
    }
    idx.truncate(want.min(len));
    idx.sort_unstable();
    idx
}
