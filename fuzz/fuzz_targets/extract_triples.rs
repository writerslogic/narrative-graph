#![no_main]

use libfuzzer_sys::fuzz_target;
use narrative_graph::{
    extract_aggregates, extract_candidate_triples, find_conflicts, Language, Options,
};

const MAX_INPUT_BYTES: usize = 16 * 1024;

fuzz_target!(|data: &[u8]| {
    let bounded = &data[..data.len().min(MAX_INPUT_BYTES)];
    let text = String::from_utf8_lossy(bounded);
    // The Chinese reader gets a cast drawn from the input itself, so alias
    // matching is exercised on whatever the input holds.
    let mut chinese = Options {
        language: Language::Chinese,
        ..Options::default()
    };
    let chars: Vec<char> = text.chars().collect();
    for pair in chars.chunks(2).take(8) {
        let key: String = pair.iter().collect();
        chinese.aliases.insert(key.clone(), key);
    }
    for opts in [Options::default(), chinese] {
        let _ = extract_candidate_triples(&text, &opts);
        let _ = extract_aggregates(&text, &opts);
        let _ = find_conflicts(&text, &opts);
    }
});
