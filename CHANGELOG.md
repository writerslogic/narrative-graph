# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
  subject as a third person's sister. The rule now requires a bare copula
  between the mentions, and requires the relational noun to head the possessed
  phrase, so "Marco's master key" and "Marco's friend's sister" no longer
  match either.
- A sentence-initial function word was folded into the mention that followed
  it, so "But Elena" normalized to `but_elena` and never unified with `elena`
  elsewhere in the text, splitting one character into two nodes. A closed list
  of openers is now dropped; it holds no word that can also be a given name,
  so "May Vance" and "Grace Vance" survive.

## [0.1.0]

- Initial heuristic extraction pipeline: entity detection, relation labeling,
  confidence scoring, and span provenance, for Rust and Node.
