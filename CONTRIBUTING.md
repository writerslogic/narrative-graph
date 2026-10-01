# Contributing to narrative-graph

Thanks for considering a contribution. narrative-graph turns prose into candidate relational facts with a rule-based, deterministic pipeline: no language model, no network call, no state between calls. Keep that scope in mind. Anything that needs a model, a store, or a project model belongs in a consuming application, not here.

## Reporting bugs

Use the [GitHub issue tracker](https://github.com/writerslogic/narrative-graph/issues). Include the input sentence or passage, the candidate you got, the candidate you expected, and the crate or package version. For security issues, follow [SECURITY.md](./SECURITY.md) instead of opening a public issue.

## Suggesting enhancements

Open an issue before writing code for a new relation form, a new language pack, or a change to the candidate shape. Each enhancement issue should say what prose pattern it covers and how the result will be measured; design discussion on those is as valuable as a patch.

## Development setup

```bash
git clone https://github.com/writerslogic/narrative-graph.git
cd narrative-graph
cargo test                          # segmentation + heuristic pipeline
cargo test --all-features           # adds the ts-rs binding-export tests
cargo tree                          # should print this crate and nothing else
npm install && npm test             # node --test tests/node
npm run test:types                  # strict TS check against generated bindings
```

Rust 1.77 or newer and Node.js 18 or newer. TypeScript definitions in `bindings/` are generated from the Rust types via `ts-rs`; do not hand-edit them.

## Before opening a PR

- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, `npm test`, and `npm run test:types` all pass. CI runs the same commands.
- New extraction rules or relation forms include a test with a realistic sentence and the expected candidate, written from the sentence alone rather than from the rule that will match it. If the change affects measured precision or recall, update `docs/EVALUATION.md` with the new numbers.
- Public API changes regenerate `bindings/` (`cargo test --features bindings`) and update `docs/ARCHITECTURE.md` if the pipeline stages change.
- Commit messages use `<type>: <description>` in the imperative (types: feat, fix, refactor, test, docs, perf, chore).

## Fuzzing

A cargo-fuzz target lives in `fuzz/`. Install `cargo-fuzz` and run `cargo fuzz run extract_triples` before changing the segmenter or any pattern that walks token spans.

## Code of Conduct

This project follows the [Contributor Covenant](./CODE_OF_CONDUCT.md).
