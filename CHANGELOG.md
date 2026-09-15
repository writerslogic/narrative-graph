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
- Triple spans came from a first-occurrence substring search and were wrong
  when an entity repeated earlier in the sentence; they now come from the
  entity candidates that produced the relation.

## [0.1.0]

- Initial heuristic extraction pipeline: entity detection, relation labeling,
  confidence scoring, and span provenance, for Rust and Node.
