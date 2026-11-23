# Evaluation

## What is measured today

There is no labeled precision/recall corpus for this project yet. What
exists is behavioral test coverage over the pattern set documented in
[ARCHITECTURE.md](./ARCHITECTURE.md):

| Suite | Count | What it checks |
|---|---|---|
| `src/heuristic/segment.rs` (`cargo test`) | 14 | Sentence segmentation: honorifics, initials, ellipses, decimals, em-dashes, quoted dialogue with attribution, terminator runs, multibyte offsets, empty input, and a 20k-case property test asserting no panic and valid char-boundary offsets |
| `tests/heuristic.rs` (`cargo test`) | 84 | Each relation pattern fires on a canonical example, confidence ordering, span correctness, `min_confidence` filtering, rule attribution, and multi-relation sentences; plus nested mentions (`Jane` inside `Mary Jane`), a surface form recurring inside an earlier word (`Dev` inside `Devon`), an entity whose lowercase form changes byte length (`İ`), a possessive bounded to its noun phrase, passive-voice role orientation, spans covering the token that licensed the relation, a referent mentioned twice yielding both relations, no self-relations, whole-word lexicon matching (`grandmother` is not `mother`), stance nouns scoring below stated kinship, one label shared between a noun rule and a verb rule, and a 20k-case property test over the whole pipeline asserting no panic and valid char-boundary spans. Ten of them came out of the near-miss probing below. Seven assert that nothing is emitted: a possessive without a copula, a reported possessive, a non-head relational noun, a denied or suspended relation, a question, a plural copula, and a capitalized pronoun inside dialogue. Three guard the other direction, so the gates cannot be tightened into silence: a sentence opener dropped without losing the mention after it, a given name that reads as a function word, and each copula the possessive still accepts. Entity boundaries have their own group: a name particle inside a run, a trailing particle that must not be folded in, a leading title dropped from the identity, a title in front of one name word kept because it is doing the distinguishing work, and a style qualifier that is only a title in front of another title. Ten more cover the alias lexicon: a lowercase key becoming a mention, case-insensitive matching alongside an exact uppercase key, an overlapping key that must not replace the run it spans, longest key first, no match inside a longer word, offsets that address the source text past a multi-byte character, a lexicon mention resolving a pronoun, a nearer capitalized run keeping that pronoun instead, a key covering a pronoun replacing it, and a lexicon mention taken outright where no capitalized word precedes the pronoun. Document-level behaviour has its own group: one fact stated three ways yielding one aggregate with three spans, repetition not raising an aggregate's confidence, aggregates ordered by where the story states them while the per-sentence output keeps its own order, a rejection suppressing only the triple it names and keying on the relation the caller saw, a denial recorded as the opposite claim in three phrasings, a conditional denial settling nothing, an assertion and its denial flagged with both spans, two compatible facts about one pair flagged as nothing, the cardinality constraint binding the object rather than the subject, a pronoun taking the previous sentence's referent once the document has said who she is, a chain stopping at a paragraph break, two pronouns in one sentence never sharing a referent, a cross-sentence link agreeing in gender, a personal pronoun refusing a thing and `it` refusing a person, an unestablished capital refused as a referent, a plural pronoun never linking at all, a capitalized pronoun never becoming a graph node, and the typographic apostrophe separating a mention as the straight one does |
| `types.rs` binding-export tests (`cargo test --features bindings`) | 8 | `ts-rs` regenerates `bindings/*.ts` from `Options`, `TripleCandidate`, `SpannedTriple`, `Rejection`, `AggregateTriple`, `Polarity`, `Conflict` and `ConflictKind` without drift |
| `tests/node/api.test.cjs` (`npm test`) | 13 | The N-API surface: extraction, empty input, `minConfidence` filtering, `aliases` (including a titled surface form as the key), `ontology`, the title rule reaching the binding, the out-of-range-confidence error, a suppressed rejection, aggregation with its order and spans, a denial flagged against its assertion, and cross-sentence pronoun linking with its agreement check |
| `tests/node/types.test.mts` (`npm run test:types`) | — | `index.d.ts` accepts valid `NapiOptions`/results and rejects invalid ones (`tsc --strict`) |

All pass as of this writing (98 under default features; the 8 binding-export
tests require `--features bindings`, which CI covers via `--all-features`).
Reproduce with:

```bash
cargo test                      # 98: segmentation + pipeline
cargo test --all-features       # 106: adds the binding-export tests
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

## Recall on real prose

Measured, and the result is bad enough to state plainly: **on 1.96 MB of
public-domain narrative prose the pipeline emitted one candidate, and that
candidate is wrong.**

Method. Three novels from Project Gutenberg — *The Age of Innocence* (541),
*Pride and Prejudice* (1342), *Howards End* (2891) — with the licence header
and footer stripped, split on blank lines and rewrapped, keeping every block of
at least 80 bytes. The only filter is the length floor, so the sample is not
shaped by a guess about which prose the extractor handles well.
`examples/precision_sample.rs` does this and is the reproduction:

```
cargo run --release --features json --example precision_sample -- \
    --out DIR --sample 100000 --seed 1 541.txt 1342.txt 2891.txt
```

That corpus is 5,014 paragraphs, which is what the command reports back.

At the commit where this was first measured, the pipeline emitted **one**
candidate over all of it, and that candidate was wrong. The same harness
against `ad9aa4e`, the commit before the precision work described above,
emits 24, of which **17 name a subject that is not an entity at all** — a
sentence-opening function word (`but`, `if`, `this`), a pronoun (`it`, `they`,
`his`, `he`), or a place standing in for a person (`new_york`). So the
precision work removed a large body of nonsense, and what the measurement then
showed is that almost nothing correct was there to keep.

### Cross-sentence pronoun linking, measured against the same corpus

Resolving a pronoun past its own sentence was first built ungated, and this
corpus rejected it: it added exactly one triple over the 5,014 paragraphs, and
that triple was wrong — "He had wanted work keenly at Oniton" resolved "He" to
the woman named in the sentence before, where the man it refers to was named
three sentences earlier.

What fixed it was not a wider window. No one-sentence lookback could have
reached the right antecedent, and a wider one would only have had more wrong
candidates to choose between. What fixed it was requiring evidence: the
register described in [ARCHITECTURE.md](./ARCHITECTURE.md) reads gender,
personhood and thinghood off the document itself, and a link is made only where
that evidence positively agrees with the pronoun.

Measured after that change, on the same corpus and with linking on
unconditionally: **the output is byte-identical to the run before the feature
existed** — the same six triples, no additions and no losses. The false
positive is gone and nothing correct was traded for it. `precision_sample`
reproduces this; the four files it writes diff clean.

That is the bar the feature had to clear to be on by default, and it is a
modest one: it says the mechanism does no harm here, not that it does good
here. It cannot do visible good on this corpus, because the relation rules fire
six times in 5,014 paragraphs and a resolved pronoun only matters in a sentence
where a rule also fires. Its accuracy is now bounded by entity precision rather
than by pronoun agreement — the links it still gets wrong are links to things
that were never names, which is the limitation the next section is about.

### Why: the rules matched a form prose does not use

Counting surface forms across the same three novels, against the same
relational-noun lexicon:

| Form | Occurrences |
| --- | ---: |
| `X is Y's <noun>` — the only form the possessive rule matched | **0** |
| `X's <noun> was Y` | 0 |
| `X, Y's <noun>` | 1 |
| `the <noun> of Y` | 13 |
| `X, the <noun> of Y` | 6 |

The rule set was built entirely around the Saxon possessive and a copula. That
exact construction does not occur once in 1.96 MB of narrative prose. English
prefers the "of" genitive, which was not recognized at all.

Three rules now cover the forms that do occur: the appositive
(`X, Y's <noun>`), the reversed possessive (`Y's <noun> was X`), and the "of"
genitive (`X, the <noun> of Y`, with or without a copula in place of the
comma). Each admits only a closed class of pre-nominal modifiers between the
mentions, so a relative clause or a conjunction is not read as an apposition.

### Precision, independently adjudicated

With those rules the same 5,014 paragraphs yield 6 candidates. Each was put to
two judges who had not seen the patterns, the rule names, or the confidence
scores — only the paragraph and the claim in plain English. Both returned the
same verdict on all six:

| | |
| --- | ---: |
| Candidates | 6 |
| Judged correct | 5 |
| Judged wrong | 1 |
| Inter-judge agreement | 6/6 |

**Precision 5/6.** The sample is far too small for that ratio to mean much —
the 95% interval on 6 trials runs from roughly 44% to 97% — and it is reported
as a count for that reason. The single wrong candidate is
`mentors(shapely, mentor)`, which reads a role noun as a person; it is the one
surviving candidate from before this work and neither new rule produced it.

One judge flagged a defect the rule changes did not address, in
`widow_of(bourgh, sir_lewis)` extracted from "Lady Catherine de Bourgh, widow
of Sir Lewis de Bourgh": the lowercase "de" broke the capitalized run, so both
slots normalized to a fragment of the surname the two people share, and the
claim was only distinguishable by taking the reading that makes them different
people. A closed list of name particles now continues a run already open, and
that candidate reads
`widow_of(catherine_de_bourgh, lewis_de_bourgh)`. The other five candidates
carry the same relation, direction and referents, so the counts above still
stand. The changed candidate was not re-adjudicated: both judges ruled on the
relation and its direction, which the fix does not touch, and the entity
boundary they objected to is what it repairs.

The honorific the subject then carried — the same class of boundary error one
level out, a capitalized run taking in every title before the name — was
tracked as issue #8 and is fixed too. Re-running the sample is how both were
found and how both were confirmed; the run is unchanged at 6 candidates and
three of them changed a mention:

| Before | After |
| --- | --- |
| `widow_of(right_honourable_lady_catherine_de_bourgh, sir_lewis_de_bourgh)` | `widow_of(catherine_de_bourgh, lewis_de_bourgh)` |
| `uncle_of(mr_percy_cahill, mrs_charles)` | `uncle_of(percy_cahill, mrs_charles)` |
| `father_of(bob_spicer, mrs_manson_mingott)` | `father_of(bob_spicer, manson_mingott)` |

`mrs_charles` keeps its title on purpose: a title in front of a single name
word is the only thing distinguishing the people who share that word. The same
rule is why `daughter_of(miss_darcy, mr_darcy)` is untouched — stripping
unconditionally would make both slots `darcy`, which `extract_relations` then
drops as a self-relation, costing a candidate the judges ruled correct.

### What the number is worth

Recall went from 1 candidate to 6. That is a real improvement and it is still
6 facts from three novels. The ceiling table above is the honest bound: with
this lexicon, roughly forty relational statements exist to be found in 1.96 MB
of prose, and the pipeline currently reaches a sixth of them. Nothing here
contradicts the test suite — every test still passes, because every test uses
the phrasing its rule was written for. That is exactly the limit of what a
suite written by the pattern author can tell you, and it is why this section
exists.

## Synthetic fiction set

`tests/synthetic_eval.rs` scores extraction against paragraphs written for the
file, none quoted from a manuscript. A case lists the claims its paragraph
states; a claim counts only if (subject, relation, object, polarity) matches, so
a wrong label is both a miss and a false positive. Negatives (no gold) are
dialogue fragments crossing an attribution, a third party's possessive, common
nouns, pronouns alone, modal and conditional phrasing, desire, questions, and
verbs with other content between the mentions.

Three sets run: *development* (written beside the rules and used to tune them),
*held-out* (longer, dialogue-heavy paragraphs written afterwards; the first run
exposed misses that were then fixed, so it is no longer untouched), and
*narrator-context* (a shared `Context` with a named first-person narrator).
`cargo test --test synthetic_eval -- --nocapture` prints every miss and false
positive.

| Set | Paragraphs | Gold | Before (precision / recall) | After (precision / recall) |
|---|---|---|---|---|
| development | 84 | 58 | 0.750 / 0.103 | 1.000 / 1.000 |
| held-out | 30 | 21 | 1.000 / 0.048 | 1.000 / 0.905 |
| narrator-context | 13 | 10 | n/a / 0.000 | 1.000 / 1.000 |
| all | 127 | 89 | 0.778 / 0.079 | 1.000 / 0.978 |

"Before" is the engine at `73a1969` over the same cases. The remaining misses
are a subject elided across a coordinated verb ("divorced Bram ... and moved
back to Tulsa") and a subject separated from its verb by a relative clause
("Elena, who works for Marco, has never met Dev"). A set this size, written by
the author of the rules, bounds nothing about real prose; its use is to catch
regressions and to document which phrasings are read and which are not. The
precision floor asserted in CI is 0.95 and the recall floor 0.85.

## What is not measured

- **Precision at a sample size worth quoting.** Six adjudicated candidates is
  a count, not a rate. Getting to a number with a usable interval needs the
  recall work to continue until the corpus yields candidates in the hundreds,
  which is a lexicon and pattern problem rather than a measurement one.
  No labeled corpus is checked in. The measurement above is reproduced from
  the seed and the command, not from a fixture, so the repository carries no
  copy of the prose and nothing here goes stale against one.

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

## Hand-labelled real text

`tests/real_text_eval.rs` scores whole-book extraction against hand labels on
sentences sampled by fixed seed (`tests/corpus/tools/sample_public.py`): 150
from the six public-domain novels in `tests/corpus/`, and a private set
(`NG_PRIVATE_EVAL`, never committed) of 173 from contemporary novels and the
author's manuscript excerpts. Labels are tiered: stated, `implied`, and
described (a bracketed side such as `[her mother = Mrs. Bennet]`).
`tests/corpus.rs` snapshots every whole-book aggregate, so a rule change shows
as the claims it adds and loses.

Baseline (2026-11-22): public, 44 stated labels, none found; private, 31
stated labels, one found and one false positive. 438 of 548 labels need a
person the sentence only describes, mostly narrators and unnamed speakers,
and `speaks_to` is the most frequent relation.

### Plan and settled decisions

- Inference stays local and the default build dependency-free. No remote
  model at run time or in tests.
- Odd-numbered items are the held-out half: tune on even items, read the odd
  half only at milestones.
- Order: the apostrophe fix (`O’Leary` is cut to `leary`); dialogue speaker
  attribution emitting `speaks_to`; narrator identity; nodes for described
  people; then a learned pair classifier.
- The classifier: a teacher (a local model through ollama) is first scored on
  the even half with the same matcher; only if it does well is it used to
  label training text, with every evaluated window excluded. The student is a
  small fine-tuned encoder (DeBERTa-v3-small or ModernBERT-base class) run in
  pure Rust through `candle`, behind an opt-in `model` feature; it scores
  candidate pairs and resolves described mentions. A linear model cannot reach
  the target: most labels need coreference or a speaker carried across turns.
  The rule pipeline stays the dependency-free default and supplies features. Training text for shipped weights is public domain plus the
  author's own writing, not the commercial novels.
- Target: recall of at least 0.7 over all tiers on the held-out half, with
  precision of at least 0.8 once extractions are numerous enough to measure.
  1.0 is not a meaningful goal: annotators disagree on fiction relations, and
  half the items changed under the gpt-6.2-astra review.
- Labels were reviewed by gpt-6.2-astra (public: 78 of 150 items changed;
  private: 115 of 173). Removals mostly enforce the rule that only the target
  sentence is labelled. After the review and the apostrophe fix: public 0/58
  stated, private 2/32 stated with no false positives.

### Ceiling and teacher (2026-11-22)

`NG_HALF=even|odd` scores one half. `NG_PREDICTIONS` (public) and
`NG_PRIVATE_PREDICTIONS` (private) score a file of `item | A | relation | B`
lines with the same matcher instead of the library;
`tests/corpus/tools/teacher.py` writes one from a local ollama model. Builds
use `CARGO_TARGET_DIR=/Volumes/C/rust-target-narrative-graph`, because other
sessions hold the lock on the shared target directory for hours.

The report gives each tier a ceiling: the share of labels that are
matchable, with both sides named and a relation from the guide (its lists,
the relations its rules name in backticks such as `speaks_to`, and every
relation the library has a rule for). The matcher needs the relation string
exactly, and the labels invent relations freely (`limits_criticism_of`,
`wants_to_visit`), most of them used once.

| set, half | labels | ceiling all tiers | stated | implied | described | invented names, used once |
|---|---|---|---|---|---|---|
| public even | 150 | 0.593 | 0.421 | 0.700 | 0.612 | 38, 29 |
| public odd | 170 | 0.576 | 0.538 | 0.682 | 0.569 | 52, 45 |
| private even | 121 | 0.331 | 0.438 | 0.692 | 0.261 | 45, 38 |
| private odd | 126 | 0.341 | 0.500 | 0.875 | 0.275 | 49, 41 |

So recall of 0.7 over all tiers is out of reach for any extractor, not only a
vocabulary-bound one, while the labels stand: a model cannot learn names used
once. Private described labels are mostly nameless (`[the man]`).

- Library, recall over all tiers: public even 0.020, odd 0.000; private even
  0.000, odd 0.016 (stated 2/16, no false positives).
- Teacher, qwen3:14b without thinking, 3000 characters of preceding text, a
  cast of frequent names and the label guide as the prompt, public even:
  recall 0.093 over all tiers and 0.135 over the matchable labels, precision
  0.136 over 103 extractions. Most false positives are invented relations or
  common nouns as sides. Not good enough to label training text.
