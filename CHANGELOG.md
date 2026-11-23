# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-11-23

### Added

- `analyze_document` in Rust and `analyzeDocumentNapi` in Node return
  character profiles, linked mentions, relationship history, events, and
  knowledge statements. The English reader resolves aliases, pronouns,
  and descriptions using names and family evidence across the document.
- `dialogue.lines` reports each recognized quoted utterance with its
  speaker and listener IDs when known, plus the original text ranges.
  `dialogue.unattributed` counts lines missing either ID.
- Relationship histories track observations in story order. Supported
  observations carry calibrated confidence estimates fitted on reviewed
  LitBank predictions. [Evaluation](./docs/EVALUATION.md)
- `Series` in Rust and `NarrativeSeries` in Node reuse chapter analyses
  by revision, merge evidence across books, and report chapter-relative spans.
  Caller aliases provide stable character IDs across chapters and books.
- The opt-in Rust `model` feature runs a local DeBERTa student checkpoint
  through `StudentModel` or the CLI's `--model` option.
- Chinese, traditional or simplified, through `Options::language`
  (`language: 'chinese'` in Node, `--language chinese` in the CLI). It has a
  reader of its own: names come from the caller's `aliases`, matched as
  literal character runs, and from a surname followed by a title (王夫人,
  邢氏). It reads kinship and service from a genitive (賈赦之妻邢氏) or a
  copula (李氏即賈珠之妻), marriage from 娶 and 嫁, and `speaks_to` from
  speech aimed at someone (寶玉便問襲人) or a quotation that opens on a name.
  Denials (並非) are kept as denials and conditionals (若是) assert nothing.

### Changed

- `Options` has a new public field, `language`, so a struct literal that
  names every field must add it (or use `..Default::default()`).

## [0.2.0] - 2026-11-23

### Added
- `speaks_to` from dialogue. A tagged speaker speaks to the name a turn
  addresses ("Cathy, are you busy?" asked Heathcliff), and each conversation
  is solved whole: once both sides of a two-person exchange are tagged,
  untagged turns alternate between them, each said to the other. A third
  voice, two tags on one turn, or a pronoun tag that fits only the other side
  stops the inference. Each claim spans every sentence of the turn.
- A first-person document without a caller-supplied narrator gets one when
  the turns beside "I said" turns address one name at least three times and
  twice as often as any other; the document is then read with "I" as that
  name.
- `tests/real_text_eval.rs` scores one half (`NG_HALF`), reports each tier's
  ceiling, and scores another extractor's output (`NG_PREDICTIONS`).
- `analyze` / `analyze_in` and `analyzeNapi`: candidates, aggregates and conflicts
  from one pipeline run. `analyzeAsyncNapi` runs it on the libuv worker pool.
- Exact-phrase relation rules for marriage, romance, friendship, acquaintance,
  work, study, residence, ownership, membership and contact; possessor-first
  kinship ("Bram's daughter Odile", "his sister Elena", "Cass, my sister");
  "x and y are <plural>"; a possessed noun with a prepositional tail
  ("Marco's student for two terms"); and more relational nouns. See
  `docs/ARCHITECTURE.md`.
- `heuristic::Context`, with `extract_aggregates_in`, `extract_candidate_triples_in`
  and `find_conflicts_in`: document-wide name evidence shared across calls, and an
  optional named first-person narrator. The original entry points are unchanged.
- `tests/synthetic_eval.rs`: a precision/recall score over hand-written fiction
  paragraphs. See `docs/EVALUATION.md`.

- A wholly lowercase `aliases` key is now a lexicon entry: the phrase is
  searched for in the text as a literal, word-bounded, case-insensitive match
  and becomes a mention of the mapped canonical name. This is what the option
  was documented for and could not do — an alias key was matched against the
  capitalized run, and a lowercase phrase such as "the detective" is never a
  run, so the key could never fire. The caller asserts the phrase names a
  person by putting it in the map, so nothing is inferred. Keys carrying an
  uppercase character keep matching the surface form exactly, longer keys are
  tried first, and a lexicon match overlapping a mention already detected is
  dropped, so a key can add a mention but never replace one. A lexicon mention
  is a referent a pronoun can take, winning where it sits nearer the pronoun
  than the capitalized word the backward walk finds and where that walk finds
  no capitalized word at all, so pronoun
  resolution is unchanged for a caller supplying no lowercase key; a pronoun
  covered by a lexicon mention is replaced by it.

### Changed
- Node spans are UTF-16 code unit offsets, which `String.prototype.slice`
  takes. They were UTF-8 byte offsets and selected the wrong text after any
  non-ASCII character. The Rust API still reports byte offsets.
- A suspender opening the subject's clause ("If Elena is Marco's sister, ...")
  now suspends every rule, so the positive conditional is no longer asserted.
  The scan stops at a semicolon, colon, dash or quotation mark.
- The one-subject check behind cardinality conflicts reads the rule's relation
  before the ontology renames it: renaming `mother_of` keeps the constraint,
  and mapping another relation onto that label does not acquire it.
- An invalid `min_confidence` is rejected even for empty text, by the CLI
  instead of being treated as no threshold, and in Node before narrowing to
  `f32`, with `InvalidArg`.
- The CLI rejects an unknown `--format`, buffers output, and exits cleanly on
  a closed pipe.
- Conflict detection groups aggregates by fact instead of comparing every
  pair, and extraction allocates less. Whole-book extraction is about a third
  faster with identical output.
- A relation is no longer read across a quotation mark, a pronoun inside speech
  is not resolved, and a pronoun outside speech does not take an antecedent from
  inside it. A sentence opener ("Then", "The") is no longer a pronoun antecedent.
- A comma now ends a capitalized run, so "Nora, Dev and Marco" is three names, not
  `nora_dev` and `marco`. The typographic apostrophe is read as the possessive
  clitic by the possessive rules.
- `extract_entities` results carry two additional fields, `pronoun` and `quoted`.

### Fixed
- Hard-wrapped speech no longer closes at every line break, only at a
  paragraph break, so a name inside a long speech is no longer read as a
  speaker or as narration.
- An apostrophe before a capital stays inside the name ("O’Leary", not
  `leary`).
- Words the document writes in lowercase as often as capitalized
  ("presently", "besides") and sentence openers ("Yes", "But") are no longer
  speakers or listeners.
- A title leading a name is no longer part of the entity's identity. A
  capitalized run swallowed every honorific in front of the name, so
  "the Right Honourable Lady Catherine de Bourgh" normalized to
  `right_honourable_lady_catherine_de_bourgh` and could never unify with
  "Lady Catherine" or "Catherine de Bourgh" elsewhere in the same novel. Titles
  are now dropped from the normalized form while `EntityCandidate.text` keeps
  the styled surface form, which is the form `aliases` is keyed on. The last
  title is kept when only one name word would remain, because that is the title
  doing the distinguishing work: stripping it collapses "Miss Darcy" and
  "Mr. Darcy" onto `darcy` and the relation between them is then dropped as a
  self-relation.

## [0.1.2] - 2026-09-16

### Added
- Three relation forms that real prose actually uses: the appositive
  (`X, Y's <noun>`), the reversed possessive (`Y's <noun> was X`), and the
  "of" genitive (`X, the <noun> of Y`). Measured over three public-domain
  novels, the form the possessive rule previously required (`X is Y's <noun>`)
  occurs zero times in 1.96 MB of narrative prose, and the "of" genitive
  nineteen; recall over that corpus goes from 1 candidate to 6, of which two
  independent judges who had not seen the patterns rated 5 correct.
- `examples/precision_sample.rs`, a seeded sampler that draws a reproducible
  corpus sample, extracts from it, and writes a separate adjudication sheet
  carrying only the passage and the claim, so precision can be judged by
  someone who did not write the rules. See `docs/EVALUATION.md`.

### Fixed
- A lowercase particle inside a name no longer truncates the mention. "Lady
  Catherine de Bourgh" normalized to `bourgh`, a fragment of the surname she
  shares with "Sir Lewis de Bourgh", so a relation between two members of one
  family could collapse into a claim about a single node. A closed list of
  particles now continues a capitalized run already open, and only when a
  capitalized word follows, so a trailing particle is not glued to the next
  clause (#7).

## [0.1.1] - 2026-09-15

First published release. 0.1.0 was tagged, reached neither crates.io nor npm,
and its tag name cannot be reused, so this is the initial release in practice.
The release workflow was what failed: it passed `--dir` to `napi artifacts`, an
option napi 3 removed, and generated no `npm/<platform>/package.json`, leaving
the four `optionalDependencies` the loader resolves at runtime with no packages
behind them.

### Added
- Initial heuristic extraction pipeline: entity detection, relation labeling,
  confidence scoring, and span provenance, for Rust and Node.
- Hand-rolled sentence segmenter handling honorifics, initials, ellipses,
  terminator runs, and quoted dialogue with attribution. Sentences are returned
  as borrowed slices, so segmentation allocates nothing per sentence.
- Opt-in Cargo features (`serde`, `json`, `bindings`, `node-api`, `cli`). The
  default build has no dependencies; CI fails if that regresses.
- Node runtime and TypeScript type tests, and `docs/ARCHITECTURE.md`,
  `docs/INTEGRATION.md`, `docs/EVALUATION.md`.

### Changed
- Confidence is now scored from rule strength and subject/object proximity
  instead of returning one of four hardcoded constants.

### Fixed
- Relation matching re-located each entity by searching a lowercased copy of
  the sentence instead of using the offsets the entity candidate already
  carried. Three consequences, all fixed: nested mentions (`Jane` inside
  `Mary Jane`) underflowed the subject/object gap and panicked; a surface form
  that also occurred inside an earlier word (`Dev` inside `Devon`) matched the
  wrong position and silently dropped the relation; and a character whose
  lowercase form has a different byte length (`İ`) shifted every offset, which
  dropped relations and could panic on a non-char-boundary slice.
- `EntityCandidate.end` pointed at the separator following the next lowercase
  word rather than at the end of the mention, so `TripleCandidate.span` ran
  past its object. Spans now bound the subject and object mentions, which
  changes the reported end offset of every candidate.
- Pronoun mentions were collected grouped by pronoun word, leaving the entity
  list out of text order even though relation extraction treats the earlier
  index as the subject. Entities are now ordered by position.
- Every relation rule matched a substring of the text between the two mentions
  and never checked the rest of it, so text that denied, hedged, attributed or
  suspended the relation produced the same triple at the same confidence as a
  plain statement: "Elena did not mentor Marco", "Elena refused to mentor
  Marco" and "Did Elena mentor Marco?" all asserted `mentors`. A negator, an
  open condition, a hedging modal, a suspended `to`-infinitive, or a sentence
  ending in `?` now blocks the relation outright.
- The possessive rule never inspected that text at all, firing on nothing but
  an `'s` after the object. "Elena visited Marco's sister" claimed
  `sister_of(elena, marco)` at the system's highest confidence, naming the
  subject as a third person's sister. The rule now requires a bare singular
  copula between the mentions, and requires the relational noun to head the
  possessed phrase, so "Marco's master key" and "Marco's friend's sister" no
  longer match either.
- A sentence-initial function word was folded into the mention that followed
  it, so "But Elena" normalized to `but_elena` and never unified with `elena`
  elsewhere in the text, splitting one character into two nodes. A closed list
  of openers is now dropped; it holds no word that can also be a given name,
  so "May Vance" and "Grace Vance" survive. The list also covers the pronouns
  the pronoun pass owns, which were otherwise emitted a second time as a
  capitalized mention normalized to `she`, a graph node naming nobody.
