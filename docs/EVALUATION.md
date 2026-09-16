# Evaluation

## What is measured today

There is no labeled precision/recall corpus for this project yet. What
exists is behavioral test coverage over the pattern set documented in
[ARCHITECTURE.md](./ARCHITECTURE.md):

| Suite | Count | What it checks |
|---|---|---|
| `src/heuristic/segment.rs` (`cargo test`) | 14 | Sentence segmentation: honorifics, initials, ellipses, decimals, em-dashes, quoted dialogue with attribution, terminator runs, multibyte offsets, empty input, and a 20k-case property test asserting no panic and valid char-boundary offsets |
| `tests/heuristic.rs` (`cargo test`) | 38 | Each relation pattern fires on a canonical example, confidence ordering, span correctness, `min_confidence` filtering, rule attribution, and multi-relation sentences; plus nested mentions (`Jane` inside `Mary Jane`), a surface form recurring inside an earlier word (`Dev` inside `Devon`), an entity whose lowercase form changes byte length (`İ`), a possessive bounded to its noun phrase, passive-voice role orientation, spans covering the token that licensed the relation, a referent mentioned twice yielding both relations, no self-relations, whole-word lexicon matching (`grandmother` is not `mother`), stance nouns scoring below stated kinship, one label shared between a noun rule and a verb rule, and a 20k-case property test over the whole pipeline asserting no panic and valid char-boundary spans. Ten of them came out of the near-miss probing below. Seven assert that nothing is emitted: a possessive without a copula, a reported possessive, a non-head relational noun, a denied or suspended relation, a question, a plural copula, and a capitalized pronoun inside dialogue. Three guard the other direction, so the gates cannot be tightened into silence: a sentence opener dropped without losing the mention after it, a given name that reads as a function word, and each copula the possessive still accepts |
| `types.rs` binding-export tests (`cargo test --features bindings`) | 3 | `ts-rs` regenerates `bindings/*.ts` from `Options`, `TripleCandidate`, `SpannedTriple` without drift |
| `tests/node/api.test.cjs` (`npm test`) | 8 | The N-API surface: extraction, empty input, `minConfidence` filtering, `aliases`, `ontology`, and the out-of-range-confidence error |
| `tests/node/types.test.mts` (`npm run test:types`) | — | `index.d.ts` accepts valid `NapiOptions`/results and rejects invalid ones (`tsc --strict`) |

All pass as of this writing (52 under default features; the 3 binding-export
tests require `--features bindings`, which CI covers via `--all-features`).
Reproduce with:

```bash
cargo test                      # 52: segmentation + pipeline
cargo test --all-features       # 55: adds the binding-export tests
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
  { "subject": "dev", "relation": "works_at", "object": "archive",
    "confidence": 0.7488, "span": [39, 68], "rule": "verb-works-at-pattern" },
  { "subject": "elena", "relation": "sister_of", "object": "marco",
    "confidence": 0.85, "span": [0, 23], "rule": "possessive-sister-pattern" },
  { "subject": "marco", "relation": "mentors", "object": "dev",
    "confidence": 0.78, "span": [25, 42], "rule": "verb-mentor-pattern" }
]
```

Confidence is `score_confidence(base, gap)` (`src/heuristic/cooccurrence.rs`):
a base score carried by the pattern that matched — stated kinship and role
highest, then direct verb patterns, then social stance and relative-clause
patterns — reduced by up to 15% as the character gap between subject and
object approaches the 30-character cutoff enforced in
`find_relation_pattern`. It is not derived from a trained model or a
corpus-wide statistic — it is a fixed formula over a per-pattern constant and
a distance measurement, so identical inputs always produce identical scores.

## Precision on near-miss phrasing

The open question this replaces was per-noun precision for the possessive
lexicon: whether "sister" is a safer trigger than "friend". Probing it found
the variance is not per-noun at all. Three batches of sentences built around
the documented shapes — the same nouns and verbs, varied only in what joined
the two mentions or opened the sentence — produced 13 false positives out of
15 possessive probes, 8 out of 13 verb probes, and 6 out of 9 sentence-opener
probes. Every one fired at the pattern's full confidence, and the noun was
never the reason:

| Probe | Emitted before | Why it was wrong |
|---|---|---|
| "Elena visited Marco's sister." | `sister_of(elena, marco)` @ 0.85 | Names a third person |
| "Elena is not Marco's sister." | `sister_of(elena, marco)` @ 0.85 | Denies the relation |
| "Elena could be Marco's sister." | `sister_of(elena, marco)` @ 0.85 | Hedges it |
| "Dev believed Elena was Marco's sister." | `sister_of(dev, marco)` @ 0.81 | Claims it of the reporter |
| "Elena is Marco's friend's sister." | `sister_of(elena, marco)` @ 0.85 | Sister of the friend |
| "Elena is Marco's master key." | `master_of(elena, marco)` @ 0.85 | Names a thing |
| "Elena did not mentor Marco." | `mentors(elena, marco)` @ 0.77 | Denies the relation |
| "Elena refused to mentor Marco." | `mentors(elena, marco)` @ 0.75 | Suspends the event |
| "But Elena mentors Marco." | `mentors(but_elena, marco)` @ 0.78 | Splits the character |

One root cause covered most of it: each rule matched a substring of the text
between the mentions and never checked the rest of it. Every probe in the
table is now an assertion in `tests/heuristic.rs`, as are the true positives
of each batch, which pin the gates open; the gating is documented in
[ARCHITECTURE.md](./ARCHITECTURE.md).

This is adversarial probing by the pattern author, which is weaker evidence
than an independent judgment on real prose. It bounds nothing about behavior
on phrasing nobody thought to probe.

## What is not measured

- **Precision and recall against real narrative prose.** There is no
  labeled dataset of (passage, expected triples) pairs. `tests/fixtures/narrative-passages.json`
  exists as a placeholder (currently empty) for exactly this purpose.
  Building one requires prose with ground-truth relations labeled by
  someone other than the pattern author — the test suite above confirms the
  code does what it was written to do, not that what it was written to do
  is correct on prose it wasn't designed around.

  No such corpus appears to be published. A survey of the citation cluster in
  [Artificial Relationships in Fiction](https://aclanthology.org/2025.latechclfl-1.13.pdf)
  (LaTeCH-CLfL 2025), the most recent work to need one, turns up only speaker
  identification, character detection, spatial annotation, and unsupervised
  work with no gold release. [LitBank](https://github.com/dbamman/litbank) is
  the right domain under a usable license but annotates entities, events,
  coreference and quotations, with no relation layer.
  [Massey et al. 2015](https://arxiv.org/abs/1512.00728) has the right
  relation inventory but annotates third-party plot summaries rather than the
  prose, and ships no license. ARF itself is GPT-4o output that its authors
  state is unvalidated. Precision is the cheaper half to establish and needs
  no corpus: sample real prose, extract, and have someone who did not write
  the patterns judge each emitted triple against its sentence. The probe
  above is not that — it is the author testing his own guesses about where
  the patterns break.
- **Coverage of relation phrasing outside the documented patterns.**
  Every current test exercises the exact phrasing each pattern was built
  for; none establish a recall floor on the broader space of ways the same
  relations get expressed in real writing.
- **Cross-sentence behavior.** Every test operates within a single sentence
  or a small fixed passage; there is no evaluation of pronoun or entity
  resolution across a full chapter or document.

A precision/recall benchmark against a genuinely independent labeled corpus
remains future work; see "What is not measured" above for why no
redistributable corpus has been adopted.
