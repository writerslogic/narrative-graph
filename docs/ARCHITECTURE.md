# Architecture

narrative-graph is a pure-heuristic pipeline: no model, no network call, no
external process. Everything below runs in-process, sentence by sentence,
inside `extract_candidate_triples` (`src/heuristic/mod.rs`).

## Dependencies

The default build has **no dependencies at all** — `cargo tree` on default
features prints the crate and nothing else, and CI fails if that stops being
true. The extraction core compiles against the standard library alone:
pattern matching against a small closed vocabulary is hand-rolled byte and
token scanning, not a regex engine, so there is no DFA-compile cost and no
backtracking risk in the hot path.

Every dependency is opt-in and serves a boundary rather than the core:

| Feature | Pulls in | For |
|---|---|---|
| *(default)* | — | The heuristic pipeline |
| `serde` | `serde` | `Serialize`/`Deserialize` on the public types |
| `json` | `serde`, `serde_json` | JSON output |
| `bindings` | `serde`, `ts-rs` | TypeScript declarations generated from the Rust types |
| `node-api` | `napi`, `napi-derive`, `napi-build` (+ `bindings`, `json`) | The Node addon |
| `cli` | `clap`, `anyhow` (+ `json`) | The `narrative-graph` binary |

A Rust caller consuming `Vec<TripleCandidate>` directly never compiles any
of them.

## Pipeline

```
text
  |
  v
split_sentences            (src/heuristic/segment.rs)
  |  honorifics, initials, ellipses, quoted dialogue
  v
for each sentence:
  extract_entities          (src/heuristic/entities.rs)
    |  capitalized-token runs + pronoun-antecedent linking + alias resolution
    v
  extract_relations         (src/heuristic/relations.rs)
    |  pattern match on the text between each forward entity pair
    v
  score_confidence          (src/heuristic/cooccurrence.rs)
    |  rule strength scaled by subject-object character distance
    v
  TripleCandidate { subject, relation, object, confidence, span, rule }
  |
  v
dedup_candidates            (src/heuristic/mod.rs)
  |  keep the highest-confidence candidate per (subject, relation, object)
  v
Vec<TripleCandidate>
```

## Sentence segmentation (`src/heuristic/segment.rs`)

Segmentation runs before anything else, and it is the one part of this
codebase where a naive implementation directly undercuts what the crate is
for: splitting on every `.`/`!`/`?` misparses honorifics, initials, and
quoted dialogue — exactly the constructs narrative prose is made of.

`split_sentences` returns each sentence as a **borrowed slice** of the input
plus its byte offset, so segmentation allocates nothing per sentence and
candidate spans map straight back to the source text.

A terminator ends a sentence only when all of the following hold:

1. It is followed by whitespace or end-of-text. (`3.5`, `config.json`, and
   `narrative-graph.node` are therefore never split.)
2. A run of terminators (`?!`, `...`) is consumed as a single unit, so an
   ellipsis never splits at its first dot.
3. Trailing closing quotes and brackets are absorbed into the sentence, so
   `"Get out!" Elena ran.` breaks *after* the quote, not inside it.
4. The next word does not begin lowercase — this is what keeps dialogue
   attribution attached: `"Who are you?" she asked.` stays one sentence.
5. A lone period is not preceded by a known abbreviation (`Dr.`, `Mrs.`,
   `Lt.`, `etc.`, `Ave.`, …) or by a single capital letter, which covers
   initials: `J. R. R. Tolkien`, `U.S.`, `T. S. Eliot`.

Each of those rules has a test in `segment.rs`. Two known limitations, both
inherent to abbreviation handling rather than fixable by another rule:

- **Abbreviation-final sentences don't split.** "…on St. Marks Ave. Elena
  visits them." stays one sentence, because `Ave.` is indistinguishable from
  a mid-sentence abbreviation without parsing further.
- **A missing space after a period doesn't split** ("period.Another"), by
  design — rule 1 protects filenames, decimals, and URLs instead.

## Entity detection (`src/heuristic/entities.rs`)

Two independent sources of entity mentions, merged per sentence:

1. **Capitalized-token runs.** `extract_capitalized_entities` scans the
   sentence char by char, treats whitespace and `,.!?;:—'"` as separators,
   and joins consecutive capitalized words into one entity ("Marco",
   "the Archive" is *not* joined since "the" is lowercase, but "New York"
   would be).
2. **Pronoun-antecedent linking.** `extract_pronouns` finds word-bounded
   occurrences of `he/she/they/him/her/them/his/their/it`; for each one,
   `find_pronoun_antecedent` walks backward from the pronoun and picks the
   nearest preceding capitalized word as the antecedent. This is a
   same-sentence-only heuristic — it does not look at prior sentences.

Every mention is normalized (`normalize_entity`: lowercased, spaces replaced
with underscores) and, if the caller supplied an `aliases` map, remapped to
its canonical form. Mentions are then deduplicated, keeping the first
occurrence per normalized entity.

## Relation extraction (`src/heuristic/relations.rs`)

`extract_relations` only considers forward pairs — entity `i` before entity
`j` in text order — to avoid emitting both directions of the same pair.
For each pair, `find_relation_pattern` looks at the substring between the
end of the subject mention and the start of the object mention (capped at 30
characters; pairs farther apart than that are skipped) and checks for a
fixed set of surface patterns:

| Pattern | Example | Relation | Rule name |
|---|---|---|---|
| `<Subj> is <Obj>'s <noun>` | "Elena is Marco's sister." | `<noun>_of` | `possessive-<noun>-pattern` |
| `<Subj> ... mentor...` | "Marco mentors Dev." | `mentors` | `verb-mentor-pattern` |
| `<Subj> ... work... at...` | "Dev works at the Archive." | `works_at` | `verb-works-at-pattern` |
| `<Subj> ... , who ... work... at...` | "Marco mentors Dev, who works at the Archive." | `works_at` | `relative-works-at-pattern` |
| `<Subj> ... , who ... mentor...` | | `mentors` | `relative-mentor-pattern` |

The possessive row is driven by `POSSESSIVE_NOUNS` in
`src/heuristic/relations.rs`, a lexicon of relational nouns each yielding
`<noun>_of` under its own rule name. It carries two confidence tiers: kinship
and role nouns ("sister", "employer", "apprentice") state the relation
outright, while social-stance nouns ("friend", "enemy", "rival") use the same
grammar for a weaker claim — stance is routinely negated, hypothetical, or
narrated from a character's mistaken view — and score lower. Nouns are matched
whole-word, so "grandmother" is never read as "mother".

The possessive row requires a bare singular copula (`is`, `was`) as the
entire text between the two mentions, and requires the relational noun to head
the possessed phrase. Without both, "Elena visited Marco's sister" reads as
`sister_of(elena, marco)` — a third person's relation claimed for the subject,
at the highest confidence in the system — and so do "Marco's master key" and
"Marco's friend's sister". The appositive "Elena, Marco's sister, arrived"
states a true relation and is deliberately not extracted: it is a distinct
pattern, and widening the copula set to reach it also admits ", unlike". The
plural copulas are absent for a different reason: "Dev and Elena are Marco's
cousins" has two subjects where the pair loop sees one, so the relation would
be claimed for whichever mention the loop reached.

Across every row, text between the mentions that denies or suspends the
relation blocks it entirely: a negator (`not`, `never`, `no`, or an `n't`
clitic), an open condition (`if`, `unless`, `whether`), a hedging modal
(`could`, `would`, `might`, `may`, `should`), or a `to`-infinitive suspended
by a governing verb ("refused to mentor"). A sentence ending in `?` asks the
relation rather than stating it and is likewise skipped. These are the
opposite claim, not a weaker one, so no rule fires and no confidence tier
applies. `will` is absent from that set: a future tense asserts.

This is a fixed pattern list, not a parser — relations outside this table are
not extracted, regardless of how clearly a human reader would infer them.
`normalize_relation` then checks the caller-supplied `ontology` map and
remaps the relation if a canonical name was requested; otherwise the
pattern's default relation name passes through unchanged.

## Confidence scoring (`src/heuristic/cooccurrence.rs`)

`score_confidence(rule, gap)` starts from a base score keyed on which pattern
matched (possessive patterns score highest, then direct verb patterns, then
relative-clause patterns, with a `0.6` floor for anything else), then applies
a proximity penalty scaled by `gap` — the character distance between the
subject and object — up to a 15% reduction as the gap approaches the 30-char
cap enforced in `find_relation_pattern`. A closer pair is weaker evidence of
coincidence than a distant one, so it keeps more of the base score.

## Span and provenance

Each `RelationCandidate` (`src/heuristic/relations.rs`) carries the byte span
`[subj.start, obj.end]` taken directly from the entity candidates that
produced it, and the `rule` name of the pattern that matched. `mod.rs`
offsets that span by the sentence's start position in the original text to
produce the final `TripleCandidate.span`. No candidate is emitted without a
`rule`; there is no generic co-occurrence fallback path in this pipeline —
every candidate came from one of the named patterns above.

## Deduplication

`dedup_candidates` groups by `(subject, relation, object)` and keeps the
highest-confidence candidate per key, so the same fact restated with
different phrasing (or extracted redundantly by more than one sentence
window) surfaces once.

## NAPI bindings (`src/napi_bindings.rs`)

`extractCandidateTriplesNapi` is the sole exported Node function. It
converts `NapiOptions` (plain objects with `aliases`/`minConfidence`/
`ontology`) into the Rust `Options` type, calls
`extract_candidate_triples`, and maps each `TripleCandidate` into a
`NapiTripleCandidate` (widening `confidence` from `f32` to `f64` and `span`
from `usize` to `u32`, since N-API has no native `usize`/`f32`). Errors from
the extraction pipeline (currently only an out-of-range `min_confidence`)
surface as a rejected `napi::Error`.

## Known limitations

These are heuristic-coverage limits, not bugs — the pipeline behaves as
designed, but the design is narrow:

- **Entity precision**: any capitalized word not on the sentence-opener list
  is a candidate entity, including a title-cased common noun. There is no
  part-of-speech or named-entity model backing this. `SENTENCE_OPENERS` in
  `src/heuristic/entities.rs` drops a closed class of function words when they
  open a sentence, because a capitalized run becomes one mention and "But
  Elena" normalizes to `but_elena`, splitting a character into two graph
  nodes. It holds no word that can also be a given name — "May", "Will",
  "Grace", "June", "Faith", "Summer" — so a sentence opening "May Vance"
  keeps `may_vance`, while one opening "Suddenly Vance" yields `vance`. The
  pronouns the pronoun pass owns are on the list too, so a capitalized "She"
  does not also become a mention normalized to `she`.
- **Relation recall**: only the patterns listed above are recognized, and the
  possessive lexicon covers a fraction of the relational nouns English uses.
  Any other phrasing of the same relationship is invisible to the pipeline.
- **Pronoun resolution**: "nearest preceding capitalized word," with no
  gender or number agreement — a multi-entity sentence can link a pronoun to
  the wrong antecedent.
- **No cross-sentence coreference**: entities and pronouns are resolved
  within a single sentence only; a pronoun in one sentence never resolves to
  an entity introduced in a previous one.
- **Segmentation edge cases**: see the two documented limitations in the
  segmentation section above.
- **Assertion gating is lexical**: the negation, condition and modal checks
  read only the text between the two mentions, so a denial expressed outside
  that window ("It was a lie. Elena is Marco's sister.") is not caught.
