# Fuzzing

Install `cargo-fuzz`, then run:

```sh
cargo fuzz run extract_triples
```

Targets must remain deterministic and bound the input they hand to the extractor.
