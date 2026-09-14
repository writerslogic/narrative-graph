<img src="https://raw.githubusercontent.com/writerslogic/narrative-graph/main/assets/logo-black.svg" alt="narrative-graph logo" width="120" align="left">

<h1>narrative-graph</h1>

<p><strong>Turn prose into candidate relational facts — entirely local, no LLM, no network. Entity detection, relation labeling, confidence scoring, and span provenance on every candidate, for Rust and Node.</strong></p>

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
  <a href="#roadmap">Roadmap</a> &middot;
  <a href="#contributing">Contributing</a>
</p>

---

narrative-graph reads prose and returns candidate `(subject, relation, object)` facts with a confidence score and the exact source span each one came from. No API key, no network call, no model download by default. It stores nothing and queries nothing — it's a producer you point at any store, or at no store at all.

> **Input:**
> ```
> Elena is Marco's sister. Marco mentors Dev, who works at the Archive.
> ```
>
> **Output:**
> ```
> elena  --sister_of--> marco       0.81   [0..24]
> marco  --mentors-->   dev         0.74   [25..46]
> dev    --works_at-->  the_archive 0.69   [46..70]
> ```

Works with Rust 1.75+ and Node.js 18+, on macOS, Linux, and Windows. Pairs naturally with [`holographic-memory`](https://github.com/writerslogic/holographic-memory)'s Meaning Memory — `memorizeTriplet` / `relate_phase` expect exactly this shape — but has no dependency on it and no opinion on what you do with a candidate.

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

<details>
<summary><strong>Optional: local ONNX extraction model (Node.js only)</strong></summary>

The default pipeline is pure heuristics and needs nothing extra. For higher recall on prose that doesn't fit a recognizable verb-phrase pattern, install `@huggingface/transformers` and point narrative-graph at a local model directory (Node.js only):

```javascript
const { createLocalExtractor, extractCandidateTriples } = require('narrative-graph');

const extractor = await createLocalExtractor({
  modelPath: process.env.NG_MODEL_DIR,
  modelId: 'Xenova/bert-base-NER',
  revision: process.env.NG_MODEL_REVISION,
  dtype: 'q8',
});
const candidates = extractCandidateTriples(text, { extractor });
await extractor.dispose();
```

Nothing is downloaded by this factory — supply an existing model directory and an explicit revision. The heuristic pipeline runs unconditionally as a baseline; the model, when supplied, sharpens entity boundaries and relation labels on harder sentences.

The Rust core stays heuristics-only by design to keep the default build lightweight and dependency-free — this matches the pattern of the sibling [`holographic-memory`](https://github.com/writerslogic/holographic-memory) project, where local embedding models are similarly Node-only via `@huggingface/transformers`.

</details>

## What It Extracts

### Entities

Capitalized-token detection, pronoun-antecedent linking within a sentence, and an optional caller-supplied alias list so "Marcus" and "the detective" resolve to the same entity when you already know that.

> **You supply:** `{ aliases: { "the detective": "marcus" } }`
> **Result:** mentions of "the detective" resolve to `marcus` in every candidate.

### Relations

Verb-phrase heuristics between two entity candidates in the same clause — familial, social, professional, and spatial relations ("sister of," "mentors," "works at," "located in"). Surface forms normalize to a small controlled vocabulary (`sister_of`, `mentors`, `works_at`, ...) rather than passing through raw verb phrases, so "trains," "is the mentor of," and "mentors" all collapse to the same relation type. Supply your own ontology map to override or extend the defaults.

### Explainability

Every candidate reports the rule that produced it, not just a score — `possessive-sister-pattern`, `verb-mentor-pattern`, `cooccurrence-fallback`. A confidence number alone tells you nothing about *why* the pipeline believes something; the rule name does.

### Confidence and Span

Every candidate carries a confidence score from sentence-window co-occurrence strength and pattern specificity, plus the exact byte offset it was extracted from — so a caller can show the source sentence, gate on a threshold, or route low-confidence candidates to human review instead of a store.

> **You:** Extract relations from Chapter 3 above confidence 0.7.
>
> **narrative-graph:** 12 candidates found, 8 above threshold. Filtered: "elena --knows--> the stranger" (0.41) — the co-occurrence window spans a paragraph break, weak evidence for a direct relation.

### Local ONNX Mode

Swap the heuristic entity/relation steps for a local ONNX model when default recall isn't enough — same "bring your own model, no download" contract as `holographic-memory`'s local embedder. See [Install](#install).

## API

Two surfaces, one behavior. To keep the default build light, the ONNX path is feature-gated in Rust and an optional peer dependency in Node.

<details>
<summary><strong>Rust</strong> -- core extraction, types, feature flags</summary>

| Item | What it does |
|------|-------------|
| `extract_candidate_triples(text, &Options)` | Run the heuristic pipeline (and the ONNX path, if configured) over a passage |
| `Options { aliases, min_confidence, extractor, ontology }` | Alias map, confidence floor, optional local extractor handle, relation-vocabulary overrides |
| `TripleCandidate { subject, relation, object, confidence, span, rule }` | One candidate fact; `span` is a byte range into the input, `rule` names the pattern that produced it |
| `create_local_extractor(config)` | *(feature `onnx-ner`)* Load a local ONNX model by path and revision; no download |

</details>

<details>
<summary><strong>CLI</strong> -- extract from a file without embedding the library</summary>

```bash
narrative-graph extract chapter.txt --min-confidence 0.6 --format json
```

Installed alongside the crate (`cargo install narrative-graph` provides the `narrative-graph` binary), the same way `holographic-memory` ships `hms-admin` and `hms-eval` as bins in its own crate rather than separate packages. Useful for shell pipelines, CI checks, and inspecting output before wiring the library into an application.

</details>

<details>
<summary><strong>Node.js</strong> -- extraction, local models, types</summary>

| Function | What it does |
|----------|-------------|
| `extractCandidateTriples(text, options?)` | Run the heuristic pipeline (and the ONNX path, if an extractor is passed) |
| `createLocalExtractor(config)` | Load a local ONNX model by path and revision; requires `@huggingface/transformers` |

`options` accepts `aliases`, `minConfidence`, and `extractor`. Full TypeScript definitions ship in the package (`index.d.ts`), generated from the Rust types via `ts-rs` — treat them as the source of truth over this table.

</details>

## Guides

- **[Architecture](./docs/ARCHITECTURE.md)** -- the extraction pipeline, confidence scoring, and the ONNX path
- **[Integration with holographic-memory](./docs/INTEGRATION.md)** -- extracting, thresholding, and feeding a Meaning Memory store
- **[Evaluation](./docs/EVALUATION.md)** -- methodology and results against the fixtures in `tests/fixtures/`
- **[Contributing](./CONTRIBUTING.md)** -- development setup and conventions

## Requirements

- **Rust 1.75+** or **Node.js 18+**
- Optional: `@huggingface/transformers` (Node) or the `onnx-ner` feature (Rust) for local-model extraction
- No API key, no network access, no external service of any kind

## Development

```bash
git clone https://github.com/writerslogic/narrative-graph.git
cd narrative-graph
cargo test                          # heuristic pipeline, Rust side
npm install && npm test             # node --test tests/node
npm run test:types                  # strict TS check against generated bindings
cargo test --features onnx-ner      # local-model path
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

## Roadmap

The current pipeline works sentence-by-sentence. The next stage of this project is document-level: taking a whole manuscript's worth of sentence-level candidates and turning them into a coherent, trustworthy picture of the story world.

- **Whole-document aggregation** -- merge and deduplicate candidates across a chapter or manuscript instead of emitting a fresh candidate every time a fact is restated; accumulate every supporting span and mention under one fact.
- **Contradiction detection** -- once candidates accumulate across a document, flag conflicts a single sentence can't see: "only child" in Chapter 1 against "my sister" in Chapter 9.
- **Cross-sentence coreference** -- extend pronoun linking past the sentence boundary to follow pronoun chains across paragraphs, the way real prose actually reads.
- **Temporal and event extraction** -- relations that change over the course of a story (enemies becoming allies) need a "when," not just a static fact, to support a timeline rather than a single snapshot graph.
- **Correction feedback** -- let a caller reject a wrong candidate once and suppress that false positive on the next extraction pass over the same manuscript.
- **Multi-language pattern packs** -- the heuristic pipeline is English-only today; additional verb-phrase and capitalization pattern sets per language are the path to broader use.

None of the above is required to use narrative-graph today — the sentence-level pipeline is complete and useful on its own. This is where contributions and design discussion are most valuable.

## Contributing

We welcome contributions of all sizes. Check the [issue tracker](https://github.com/writerslogic/narrative-graph/issues) for `good first issue` labels, or see [CONTRIBUTING.md](./CONTRIBUTING.md) for development setup.

**Areas where help is especially welcome:**
- Any [Roadmap](#roadmap) item above -- document-level aggregation and contradiction detection are the highest-leverage next steps
- Additional relation-verb patterns for non-English narrative conventions
- ONNX model recommendations and evaluation against `tests/fixtures/`
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
