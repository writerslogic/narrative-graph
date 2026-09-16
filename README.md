<img src="https://raw.githubusercontent.com/writerslogic/narrative-graph/main/assets/logo-black.svg" alt="narrative-graph logo" width="120" align="left">

<h3>narrative-graph</h3>

<p><strong>Turn prose into candidate relational facts, entirely local. No LLM, no network. Entities, relations, confidence scores and spans, for Rust and Node.</strong></p>

<br clear="left">

[![CI](https://img.shields.io/github/actions/workflow/status/writerslogic/narrative-graph/ci.yml?style=flat-square&labelColor=20232a&branch=main&label=CI)](https://github.com/writerslogic/narrative-graph/actions/workflows/ci.yml) [![CodeQL](https://img.shields.io/github/actions/workflow/status/writerslogic/narrative-graph/codeql.yml?style=flat-square&labelColor=20232a&branch=main&label=CodeQL)](https://github.com/writerslogic/narrative-graph/actions/workflows/codeql.yml) [![OpenSSF Scorecard](https://img.shields.io/ossf-scorecard/github.com/writerslogic/narrative-graph?style=flat-square&labelColor=20232a&label=OpenSSF)](https://securityscorecards.dev/viewer/?uri=github.com/writerslogic/narrative-graph) [![License](https://img.shields.io/github/license/writerslogic/narrative-graph?style=flat-square&labelColor=20232a&color=007ec6&label=license)](https://github.com/writerslogic/narrative-graph/blob/main/LICENSE) [![Code of Conduct](https://img.shields.io/badge/code%20of%20conduct-Contributor%20Covenant%202.1-6a4c93?style=flat-square&labelColor=20232a)](https://github.com/writerslogic/narrative-graph/blob/main/CODE_OF_CONDUCT.md)

<a href="https://www.npmjs.com/package/narrative-graph">
    <img src="https://img.shields.io/npm/v/narrative-graph.svg?style=flat-square&labelColor=20232a&color=007ec6" alt="npm version"/>
  </a>
  <img src="https://img.shields.io/npm/dm/narrative-graph.svg?style=flat-square&labelColor=20232a&color=007ec6" alt="npm downloads"/>
  <a href="https://crates.io/crates/narrative-graph">
    <img src="https://img.shields.io/crates/v/narrative-graph.svg?style=flat-square&labelColor=20232a&color=007ec6" alt="crates.io version"/>
  </a>
  <a href="https://docs.rs/narrative-graph">
    <img src="https://img.shields.io/docsrs/narrative-graph?style=flat-square&labelColor=20232a&color=007ec6" alt="docs.rs"/>
  </a>
  <a href="https://github.com/writerslogic/narrative-graph/blob/main/LICENSE">
    <img src="https://img.shields.io/github/license/writerslogic/narrative-graph?style=flat-square&labelColor=20232a&color=007ec6" alt="license"/>
  </a>
  <a href="https://github.com/writerslogic/narrative-graph">
    <img src="https://img.shields.io/github/stars/writerslogic/narrative-graph?style=flat-square&labelColor=20232a&color=6a4c93" alt="stars"/>
  </a>
</p>

<p align="center">
  <a href="#install">Install</a> &middot;
  <a href="#what-it-extracts">What It Extracts</a> &middot;
  <a href="#api">API</a> &middot;
  <a href="#guides">Guides</a> &middot;
  <a href="#contributing">Contributing</a>
</p>

---

narrative-graph reads prose and returns candidate `(subject, relation, object)` facts with a confidence score and the exact source span each one came from. No API key, no network call, no model download. It stores nothing and queries nothing — it's a producer you point at any store, or at no store at all.

**The default build has zero dependencies.** `cargo tree` prints the crate and nothing else; the extraction core compiles against the standard library alone, and CI fails if that ever stops being true. Serde, TypeScript bindings, the Node addon, and the CLI are each opt-in features that serve a boundary, never the core.

> **Input:**
> ```
> Elena is Marco's sister. Marco mentors Dev, who works at the Archive.
> ```
>
> **Output** (measured, `cargo run --features cli -- extract -`):
> ```
> dev --works_at--> archive 0.75 [39..68]
> elena --sister_of--> marco 0.85 [0..14]
> marco --mentors--> dev 0.78 [25..42]
> ```

Works with Rust 1.77+ and Node.js 18+, on macOS, Linux, and Windows. Pairs naturally with [`holographic-memory`](https://github.com/writerslogic/holographic-memory)'s Meaning Memory — `memorizeTriplet` / `relate_phase` expect exactly this shape — but has no dependency on it and no opinion on what you do with a candidate.

## Install

### npm

```bash
npm install narrative-graph
```

### Cargo

```toml
[dependencies]
narrative-graph = "0.1"
```

### GitHub

```bash
npm install writerslogic/narrative-graph
```

### Cargo features

| Feature | Adds | Dependencies |
|---|---|---|
| *(default)* | The heuristic extraction pipeline | none |
| `serde` | `Serialize`/`Deserialize` on the public types | `serde` |
| `json` | JSON output | `serde`, `serde_json` |
| `bindings` | TypeScript declarations via `ts-rs` | `serde`, `ts-rs` |
| `node-api` | The Node addon | `napi`, `napi-derive`, `napi-build` |
| `cli` | The `narrative-graph` binary | `clap`, `anyhow` |

## What It Extracts

### Sentences

Segmentation is hand-rolled rather than a naive split on `.`/`!`/`?`, because narrative prose is made of exactly the constructs a naive splitter breaks on: honorifics (`Dr. Smith went home.`), initials (`J. R. R. Tolkien`), ellipses (`She paused... then left.`), and quoted dialogue with attribution (`"Who are you?" she asked.` is one sentence, not two). Sentences are returned as borrowed slices of the input, so segmentation allocates nothing per sentence.

### Entities

Capitalized-token detection, pronoun-antecedent linking within a sentence, and an optional caller-supplied alias list so "Marcus" and "the detective" resolve to the same entity when you already know that.

> **You supply:** `{ aliases: { "the detective": "marcus" } }`
> **Result:** mentions of "the detective" resolve to `marcus` in every candidate.

### Relations

Verb-phrase heuristics between two entity candidates in the same clause — familial, social, professional, and spatial relations ("sister of," "mentors," "works at," "located in"). Surface forms normalize to a small controlled vocabulary (`sister_of`, `mentors`, `works_at`, ...) rather than passing through raw verb phrases, so "trains," "is the mentor of," and "mentors" all collapse to the same relation type. Supply your own ontology map to override or extend the defaults.

### Explainability

Every candidate reports the rule that produced it, not just a score — `possessive-sister-pattern`, `verb-mentor-pattern`, `verb-works-at-pattern`, `relative-mentor-pattern`, `relative-works-at-pattern`. A confidence number alone tells you nothing about *why* the pipeline believes something; the rule name does.

### Confidence and Span

Every candidate carries a confidence score from the extraction rule's pattern strength and how close the subject and object are in the source text, plus the byte offset of its extracted span — a range covering the subject, the object, and the token that licensed the relation. Gate on a threshold, route low-confidence candidates to human review, or fetch source context for display.

## API

<details>
<summary><strong>Rust</strong> -- core extraction and types</summary>

| Item | What it does |
|------|-------------|
| `extract_candidate_triples(text, &Options)` | Run the heuristic pipeline over a passage |
| `Options { aliases, min_confidence, ontology }` | Alias map, confidence floor, relation-vocabulary overrides |
| `TripleCandidate { subject, relation, object, confidence, span, rule }` | One candidate fact; `span` is a byte range into the input, `rule` names the pattern that produced it |

</details>

<details>
<summary><strong>CLI</strong> -- extract from a file without embedding the library</summary>

```bash
narrative-graph extract chapter.txt --min-confidence 0.6 --format json
```

Installed alongside the crate (`cargo install narrative-graph --features cli` provides the `narrative-graph` binary), the same way `holographic-memory` ships `hms-admin` and `hms-eval` as bins in its own crate rather than separate packages. Useful for shell pipelines, CI checks, and inspecting output before wiring the library into an application.

</details>

<details>
<summary><strong>Node.js</strong> -- extraction and types</summary>

| Function | What it does |
|----------|-------------|
| `extractCandidateTriplesNapi(text, opts?)` | Run the heuristic pipeline over a passage |

`opts` accepts `aliases`, `minConfidence`, and `ontology`. Full TypeScript definitions ship in the package (`index.d.ts`), generated from the Rust types via `ts-rs` — treat them as the source of truth over this table.

</details>

## Guides

- **[Architecture](./docs/ARCHITECTURE.md)** -- the extraction pipeline and confidence scoring
- **[Integration with holographic-memory](./docs/INTEGRATION.md)** -- extracting, thresholding, and feeding a Meaning Memory store
- **[Evaluation](./docs/EVALUATION.md)** -- methodology and results against the fixtures in `tests/fixtures/`
- **[Contributing](./CONTRIBUTING.md)** -- development setup and conventions

## Requirements

- **Rust 1.77+** or **Node.js 18+**
- No API key, no network access, no external service of any kind

## Development

```bash
git clone https://github.com/writerslogic/narrative-graph.git
cd narrative-graph
cargo test                          # segmentation + heuristic pipeline
cargo test --all-features           # adds the ts-rs binding-export tests
cargo tree                          # should print this crate and nothing else
npm install && npm test             # node --test tests/node
npm run test:types                  # strict TS check against generated bindings
```

TypeScript definitions in `bindings/` are generated from the Rust types via `ts-rs` — don't hand-edit them.

## Why narrative-graph

| | Hand-written regex | General-purpose NLP toolkit | LLM-based extraction | **narrative-graph** |
|---|:-:|:-:|:-:|:-:|
| Runs offline, no API key | yes | yes | no | **yes** |
| Tuned for narrative prose (dialogue attribution, familial/social relations) | you build it | no | depends on prompt | **yes** |
| Confidence score per candidate | no | model-dependent | inconsistent across calls | **yes** |
| Source span on every candidate | you build it | sometimes | rarely | **yes** |
| Cost per extraction | free, but brittle | free, general-purpose | API cost + latency | **free, microsecond-scale** |
| Output shape ready for a relational store | you build it | you build it | you parse it | **yes -- matches `memorizeTriplet`/`relate_phase`** |

narrative-graph isn't trying to out-perform an LLM at open-domain relation extraction — it's trying to be the thing you reach for when a manuscript's characters and relationships need to become structured facts, offline, cheaply, and repeatably, with enough provenance on each candidate that a human or a downstream store can decide what to trust.

## Contributing

We welcome contributions of all sizes. Check the [issue tracker](https://github.com/writerslogic/narrative-graph/issues) for `good first issue` labels, or see [CONTRIBUTING.md](./CONTRIBUTING.md) for development setup.

The sentence-level pipeline is complete and useful on its own. Where the project goes next is tracked as [enhancement issues](https://github.com/writerslogic/narrative-graph/issues?q=is%3Aissue+is%3Aopen+label%3Aenhancement) — [whole-document aggregation](https://github.com/writerslogic/narrative-graph/issues/1) and [contradiction detection](https://github.com/writerslogic/narrative-graph/issues/3) are the highest-leverage next steps, and each issue states what needs deciding before any code. Design discussion on those is as valuable as a patch.

**Areas where help is especially welcome:**
- Additional relation-verb patterns for non-English narrative conventions
- Integration examples for stores other than `holographic-memory`

## Security

Found a vulnerability? Please report it privately — see [SECURITY.md](./SECURITY.md).

## License

Apache-2.0 &copy; [WritersLogic, Inc.](https://github.com/writerslogic)

<p align="center">
  <a href="https://github.com/writerslogic/narrative-graph">GitHub</a> &middot;
  <a href="https://www.npmjs.com/package/narrative-graph">npm</a> &middot;
  <a href="https://crates.io/crates/narrative-graph">crates.io</a> &middot;
  <a href="https://github.com/writerslogic/narrative-graph/issues">Issues</a> &middot;
  <a href="./CHANGELOG.md">Changelog</a>
</p>
