#![no_main]

use libfuzzer_sys::fuzz_target;
use narrative_graph::{extract_aggregates, extract_candidate_triples, find_conflicts, Options};

const MAX_INPUT_BYTES: usize = 16 * 1024;

fuzz_target!(|data: &[u8]| {
    let bounded = &data[..data.len().min(MAX_INPUT_BYTES)];
    let text = String::from_utf8_lossy(bounded);
    let opts = Options::default();
    let _ = extract_candidate_triples(&text, &opts);
    let _ = extract_aggregates(&text, &opts);
    let _ = find_conflicts(&text, &opts);
});
