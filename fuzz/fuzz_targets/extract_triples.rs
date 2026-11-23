#![no_main]

use libfuzzer_sys::fuzz_target;
use narrative_graph::{analyze_document, Language, Options};

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
        let document = analyze_document(&text, &opts).expect("default options are valid");
        for [start, end] in document
            .timeline
            .iter()
            .map(|f| f.candidate.span)
            .chain(document.events.iter().map(|e| e.span))
            .chain(document.events.iter().map(|e| e.trigger))
            .chain(document.knowledge.iter().flat_map(|k| [k.span, k.content]))
            .chain(document.mentions.iter().map(|m| m.span))
            .chain(document.dialogue.lines.iter().flat_map(|line| {
                [line.span]
                    .into_iter()
                    .chain(line.spoken_spans.iter().copied())
            }))
        {
            assert!(text.get(start..end).is_some());
        }
    }
});
