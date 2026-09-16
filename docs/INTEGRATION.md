# Integration

## Installing

```bash
npm install narrative-graph
```

The package ships a single native binding built via N-API 3 (`napi` /
`napi-derive` in `Cargo.toml`), a CommonJS entry point, an ESM entry point,
and hand-written TypeScript declarations regenerated from the Rust types via
`ts-rs` (`bindings/*.ts`, folded into `index.d.ts`).

```json
{
  "main": "index.js",
  "exports": {
    ".": {
      "types": "./index.d.ts",
      "import": "./index.mjs",
      "require": "./index.js"
    }
  }
}
```

## Node (CommonJS)

```js
const { extractCandidateTriplesNapi } = require('narrative-graph')

const candidates = extractCandidateTriplesNapi("Elena is Marco's sister.")
// [{ subject: 'elena', relation: 'sister_of', object: 'marco',
//    confidence: 0.85, span: [0, 23], rule: 'possessive-sister-pattern' }]
```

## Node (ESM)

```js
import { extractCandidateTriplesNapi } from 'narrative-graph'

const candidates = extractCandidateTriplesNapi('Marco mentors Dev.')
```

## TypeScript

`index.d.ts` declares the single export and its option/result types:

```ts
import { extractCandidateTriplesNapi, NapiOptions, NapiTripleCandidate } from 'narrative-graph'

const opts: NapiOptions = {
  aliases: { Marcus: 'marcus_hale', 'Mr Marcus Hale': 'marcus_hale' },
  minConfidence: 0.7,
  ontology: { mentors: 'mentorship' },
}

const candidates: NapiTripleCandidate[] = extractCandidateTriplesNapi(text, opts)
```

`opts` is optional — omit it, or pass `undefined`/`null`, to run with
defaults (no aliases, no confidence floor, no ontology remapping).

### Options

| Field | Type | Default | Effect |
|---|---|---|---|
| `aliases` | `Record<string, string>` | `{}` | Surface form → canonical entity name. Any mention matching a key resolves to the mapped value. A key carrying an uppercase character matches the mention's surface form exactly — the capitalized run with `.` dropped as a separator, so `Mr Marcus Hale`. A wholly lowercase key is a lexicon entry: the phrase is searched for as a literal, word-bounded, case-insensitive match and becomes a mention of the mapped name (`the detective` → `marcus`), unless it overlaps a mention already detected, in which case it is dropped — so a lexicon key is for a phrase carrying no capitalized name, and `ms. chen` never fires because the run `Ms Chen` is already there. A pronoun can take a lexicon mention as its referent, and a pronoun the mention covers is replaced by it. |
| `minConfidence` | `number` (0.0–1.0) | `0.0` (all candidates) | Candidates below this score are filtered out. Passing a value outside `[0.0, 1.0]` throws. |
| `crossSentencePronouns` | `boolean` | `false` | Let a pronoun with no antecedent in its own sentence take one from the sentence before it, within the same paragraph. Off by default because it is measurably lossy: the pipeline has no gender or number agreement, so the fallback picks by position alone. Over three novels (5014 paragraphs) turning it on adds exactly one triple, and that one is wrong. Worth turning on for prose with few characters per scene, or where recall matters more than precision and you filter afterwards. |
| `rejections` | `Array<{ subject, relation, object }>` | `[]` | Triples the caller has already judged wrong; a candidate matching one is not emitted. The three values are the normalized ones as they appeared on the candidate, so `relation` is the name *after* any `ontology` remapping. Keying on the triple rather than the span means a rejection survives an edit to the manuscript, at the cost of applying document-wide. The crate persists nothing: the set is yours to store and hand back on the next run. A rejection suppresses and does nothing else — it reweights no other candidate. |
| `ontology` | `Record<string, string>` | `{}` | Recognized relation name → caller-supplied vocabulary. Relations not present in the map pass through with their heuristic default name. |

### Result shape

Each `NapiTripleCandidate`:

| Field | Type | Meaning |
|---|---|---|
| `subject` | `string` | Normalized subject entity (lowercase, spaces → underscores) |
| `relation` | `string` | Relation type, after ontology remapping if applicable |
| `object` | `string` | Normalized object entity |
| `confidence` | `number` | 0.0–1.0 |
| `span` | `number[]` | `[start, end)` byte range in the input text |
| `rule` | `string` | Name of the extraction rule that produced this candidate |

### Aggregating a whole document

`extractCandidateTriplesNapi` works sentence by sentence and keeps the
highest-confidence candidate per triple, so the spans of every other statement
of the same fact are discarded. `extractAggregatesNapi(text, opts?)` runs the
same rules and keeps them:

```ts
const aggregates = extractAggregatesNapi(
  "Elena is Marco's enemy. Dev works at the Archive. Elena is Marco's ally.",
)
// → enemy_of, works_at, ally_of — in the order the passage states them
```

Each `NapiAggregateTriple` carries `subject`, `relation`, `object`, plus:

| Field | Type | Meaning |
|---|---|---|
| `polarity` | `"asserted" \| "denied"` | Whether the passage states the fact or states that it does not hold. Text that settles neither — a conditional, a modal, a question, a complement its main verb holds open — produces no entry at all. |
| `confidence` | `number` | The best of its supporting candidates. Repetition does not raise it: a restatement in fiction is not independent evidence, so `spans.length` is what reports corroboration and the score deliberately ignores it. |
| `spans` | `number[][]` | Every `[start, end)` that stated the fact, in document order |
| `rules` | `string[]` | The rules that produced them, first occurrence first, without repeats |

The array is ordered by each entry's first span, so two facts about the same
pair can be read in the order the story states them — which is what makes
`enemy_of` in chapter 2 and `ally_of` in chapter 20 a character arc rather
than a contradiction. `extractCandidateTriplesNapi` keeps its own order, by
triple, and is unaffected.

### Finding contradictions

`findConflictsNapi(text, opts?)` returns the claims a passage makes that cannot
both be true. Each `NapiConflict` carries `kind` plus `left` and `right`, two
`NapiAggregateTriple`s with their own spans:

| `kind` | Means |
|---|---|
| `"denial"` | The passage asserts a fact and denies the same fact. |
| `"cardinality"` | The relation admits one subject per object — `mother_of`, `father_of` — and two subjects are asserted over one object. |

`left` is the claim the passage makes first. Neither side is marked true:
deciding that needs the manuscript, so both carry their evidence and the
decision stays with you.

A relation that simply changes over a story is never a conflict. `enemy_of`
early and `ally_of` late is what a narrative does, and reporting it would bury
real continuity errors under every character arc in the book.

Two limits worth knowing. A denial has to be phrasable as a triple, so "Elena
is not Marco's sister" is representable and "Elena was an only child" is not —
the latter names no second entity, so no rule fires on it either way. And a
denial expressed outside the text between the two mentions is invisible: "It
was a lie. Elena is Marco's sister." reads as a plain assertion.

### Error handling

The only error the current pipeline raises is an out-of-range
`minConfidence`. It surfaces as a thrown `Error` (Rust's
`napi::Status::GenericFailure`) with a message containing "confidence
threshold":

```js
try {
  extractCandidateTriplesNapi(text, { minConfidence: 1.5 })
} catch (err) {
  // err.message: "Confidence threshold must be between 0.0 and 1.0, got 1.5"
}
```

## Rust

```toml
[dependencies]
narrative-graph = "0.1"
```

```rust
use narrative_graph::{extract_candidate_triples, Options};

let candidates = extract_candidate_triples(
    "Elena is Marco's sister.",
    &Options::default(),
)?;
```

## CLI

```bash
cargo install narrative-graph --features cli
narrative-graph extract chapter.txt --min-confidence 0.6 --format json
```

Reads a file (or `-` for stdin) and writes candidates as `text` (default) or
`json` to stdout — useful for a shell pipeline or a CI check without
embedding the library.

## Feeding a downstream store

`TripleCandidate` / `NapiTripleCandidate` are plain `(subject, relation,
object)` facts with no opinion on storage. The shape lines up directly with
a relational-fact store's write API — for example
[`holographic-memory`](https://github.com/writerslogic/holographic-memory)'s
`memorizeTriplet(id, subject, relation, object)`:

```js
const { extractCandidateTriplesNapi } = require('narrative-graph')
const { HolographicMemorySystem } = require('holographic-memory')

const hms = new HolographicMemorySystem(16384, './storage', { meaningEnabled: true })

const candidates = extractCandidateTriplesNapi(chapterText, { minConfidence: 0.7 })
for (const [i, c] of candidates.entries()) {
  await hms.memorizeTriplet(`fact-${i}`, c.subject, c.relation, c.object)
}
```

narrative-graph does not depend on `holographic-memory`, or on any store —
this is one integration pattern, not a requirement. Filter on `confidence`
before writing, and keep `span`/`rule` alongside the fact in your own store
if you need to show provenance or support later correction.
