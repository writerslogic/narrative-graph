# Evaluation

## What is measured today

There is no labeled precision/recall corpus for this project yet. What
exists is behavioral test coverage over the pattern set documented in
[ARCHITECTURE.md](./ARCHITECTURE.md):

| Suite | Count | What it checks |
|---|---|---|
| `src/heuristic/segment.rs` (`cargo test`) | 13 | Sentence segmentation: honorifics, initials, ellipses, decimals, em-dashes, quoted dialogue with attribution, terminator runs, multibyte offsets, and empty input |
| `tests/heuristic.rs` (`cargo test`) | 11 | Each relation pattern fires on a canonical example, confidence ordering, span correctness, `min_confidence` filtering, rule attribution, and multi-relation sentences |
| `types.rs` binding-export tests (`cargo test --features bindings`) | 3 | `ts-rs` regenerates `bindings/*.ts` from `Options`, `TripleCandidate`, `SpannedTriple` without drift |
| `tests/node/api.test.cjs` (`npm test`) | 8 | The N-API surface: extraction, empty input, `minConfidence` filtering, `aliases`, `ontology`, and the out-of-range-confidence error |
| `tests/node/types.test.mts` (`npm run test:types`) | — | `index.d.ts` accepts valid `NapiOptions`/results and rejects invalid ones (`tsc --strict`) |

All pass as of this writing (24 under default features; the 3 binding-export
tests require `--features bindings`, which CI covers via `--all-features`).
Reproduce with:

```bash
cargo test                      # 24: segmentation + pipeline
cargo test --all-features       # 27: adds the binding-export tests
npm install && npm test
npm run test:types
```

CI additionally enforces `cargo fmt --check`, `cargo clippy --all-targets
--all-features -D warnings`, and a gate asserting the default build has zero
dependencies.

## Measured example output

Confidence and span values quoted in the README and `docs/INTEGRATION.md`
are captured directly from a run, not invented:

```bash
$ echo "Elena is Marco's sister. Marco mentors Dev, who works at the Archive." \
  | cargo run --features cli -- extract - --format json
```

```json
[
  { "subject": "elena", "relation": "sister_of", "object": "marco",
    "confidence": 0.85, "span": [0, 16], "rule": "possessive-sister-pattern" },
  { "subject": "marco", "relation": "mentors", "object": "dev",
    "confidence": 0.78, "span": [25, 47], "rule": "verb-mentor-pattern" },
  { "subject": "dev", "relation": "works_at", "object": "archive",
    "confidence": 0.75, "span": [39, 69], "rule": "verb-works-at-pattern" }
]
```

Confidence is `score_confidence(rule, gap)` (`src/heuristic/cooccurrence.rs`):
a base score per rule family (possessive highest, then direct verb patterns,
then relative-clause patterns), reduced by up to 15% as the character gap
between subject and object approaches the 30-character cutoff enforced in
`find_relation_pattern`. It is not derived from a trained model or a
corpus-wide statistic — it is a fixed formula over the rule name and a
distance measurement, so identical inputs always produce identical scores.

## What is not measured

- **Precision and recall against real narrative prose.** There is no
  labeled dataset of (passage, expected triples) pairs. `tests/fixtures/narrative-passages.json`
  exists as a placeholder (currently empty) for exactly this purpose.
  Building one requires prose with ground-truth relations labeled by
  someone other than the pattern author — the test suite above confirms the
  code does what it was written to do, not that what it was written to do
  is correct on prose it wasn't designed around.
- **Coverage of relation phrasing outside the six documented patterns.**
  Every current test exercises the exact phrasing each pattern was built
  for; none establish a recall floor on the broader space of ways the same
  relations get expressed in real writing.
- **Cross-sentence behavior.** Every test operates within a single sentence
  or a small fixed passage; there is no evaluation of pronoun or entity
  resolution across a full chapter or document.

A precision/recall benchmark against a genuinely independent labeled corpus
is tracked as future work — see the Roadmap in [README.md](../README.md).
