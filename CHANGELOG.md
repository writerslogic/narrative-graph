# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
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
