# Architecture

The default extractor runs its rules in-process, sentence by sentence,
inside `extract_candidate_triples` (`src/heuristic/mod.rs`). The opt-in
`StudentModel` runs a local encoder alongside the rules. It builds pairs
from the names in each window and scores them with an ONNX checkpoint.

## Dependencies

The default build has **no dependencies at all** — `cargo tree` on default
features prints the crate and nothing else, and CI fails if that stops being
true. The extraction core compiles against the standard library alone:
pattern matching against a small closed vocabulary is hand-rolled byte and
token scanning, not a regex engine, so there is no DFA-compile cost and no
backtracking risk in the hot path.

Each dependency belongs to an opt-in feature.

| Feature | Pulls in | For |
|---|---|---|
| *(default)* | — | The heuristic pipeline |
| `serde` | `serde` | `Serialize`/`Deserialize` on the public types |
| `json` | `serde`, `serde_json` | JSON output |
| `bindings` | `serde`, `ts-rs` | TypeScript declarations generated from the Rust types |
| `node-api` | `napi`, `napi-derive`, `napi-build` (+ `bindings`, `json`) | The Node addon |
| `cli` | `clap`, `anyhow` (+ `json`) | The `narrative-graph` binary |
| `model` | Tract and SentencePiece (+ `json`) | Local DeBERTa inference |

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
character profiles          (src/heuristic/coreference.rs)
  |  named mentions, narrator vote, alias and family evidence
  v
for each sentence:
  resolve mentions
    |  links to character IDs with the original byte spans
    v
  extract relations and events
    |  pattern match using the resolved mentions
    v
  score_confidence          (src/heuristic/cooccurrence.rs)
    |  rule strength scaled by subject-object character distance
    v
  TripleCandidate { subject, relation, object, confidence, span, rule }
  |
  v
one of two collapses      (src/heuristic/mod.rs, src/heuristic/aggregate.rs)
  |  dedup_candidates: highest confidence per (subject, relation, object)
  |  aggregate:        every span per (subject, relation, object, polarity)
  v
Vec<TripleCandidate>  /  Vec<AggregateTriple>  ->  find_conflicts

analyze_document also exposes the profiles, linked mentions, individual facts,
events, and knowledge statements.
```

## Language pack (`src/heuristic/pack.rs`)

Every closed vocabulary the pipeline reads — sentence openers, name particles,
honorifics, pronouns, abbreviations, the relational-noun lexicon, copulas,
appositive modifiers, suspending words, negators, the single-filler relations,
and the verb rules — lives in one
`LanguagePack`, and `ENGLISH` is the only value of it today. Each stage takes
the pack as a parameter rather than reaching for a `const` of its own, so what
a second language must supply is exactly that struct and nothing else.

A pack is vocabulary, not strategy. Three assumptions stay in the code that
reads one and cannot be moved into it: capitalization is the entity signal, a
possessive is a clitic on the possessor read left to right, and a sentence-final
`.`/`!`/`?` terminates while a lowercase continuation does not. A second pack is
therefore enough only for a language sharing all three; deciding otherwise is
what the second pack is for. Selecting a pack is not yet caller-facing, since
there is nothing to select.

## The mention register (`src/heuristic/register.rs`)

Cross-sentence resolution needs facts a single sentence cannot supply: whether
a capitalized word is a name at all, whether the name is a person, and whether
that person is male or female. A pre-pass over the whole text records what the
document itself says, and nothing else:

- **Established.** Seen capitalized where no sentence boundary forced the
  capital, or seen carrying a title. This is what keeps `Behind`, `Penned` and
  `Imagine` from being referents — the sentence-opener list is closed and
  cannot hold every word that opens an English sentence.
- **Gender and personhood**, from titles: `Mr`/`Sir`/`Lord` against
  `Mrs`/`Miss`/`Ms`/`Lady`/`Dame`. `Dr` and `Captain` are titles and settle
  nothing.
- **Thinghood**, from a determiner in front of the name ("the Archive"), from a
  name of a time, and from a name the document only ever uses after a locative
  preposition and nowhere else.
- **What a subject pronoun says about the one name in front of it.** Most names
  in prose are bare most of the time, and "Elena said she would go" states
  Elena's gender as plainly as "Mrs." would. Only subject pronouns, only where
  exactly one name precedes them, and never where that name sits in a locative
  phrase — an object pronoun after a name usually names someone else, and "In
  London she saw them" opens on an adjunct rather than a subject.

Evidence is **counted, not latched**: a reading wins only by outweighing its
opposite two to one, and a title outweighs three bootstrapped sightings. One
sentence reading "Newland Archer looked at her" therefore cannot record him as
female for the whole document. A tie settles nothing, and an unsettled name is
one no pronoun may take — so ambiguity costs recall and never accuracy.

Cross-sentence pronouns use positive evidence from the register. A plural
pronoun is left unresolved because each mention carries a single character ID.

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

**Capitalized-token runs.** `extract_capitalized_entities` scans the
   sentence char by char, treats whitespace and `,.!?;:—'’"` as separators
   (both apostrophes, since narrative prose is typeset with the curly one),
   and joins consecutive capitalized words into one entity ("Marco",
   "the Archive" is *not* joined since "the" is lowercase, but "New York"
   would be). A lowercase word on the closed `name_particles` list ("de",
   "van", "von", …) continues a run that is already open, so "Lady Catherine
   de Bourgh" is one mention; it is held until a capitalized word follows, so
   a particle before a lowercase word or a comma is not folded in and never
   extends the mention's end offset. English "of" is deliberately not a
   particle: it would swallow "the Duchess of Devonshire" and it is the link
   text the "of" genitive rule matches.
Every mention is normalized (`normalize_entity`: lowercased, spaces replaced
with underscores) and, if the caller supplied an `aliases` map, remapped to
its canonical form. Normalization also drops the titles leading the run
(`honorifics`, plus `right`/`most`/`very` in front of another title), because
a title is capitalized and correctly part of the surface form but not part of
the identity: without this, "the Right Honourable Lady Catherine de Bourgh"
cannot unify with any shorter mention of her. The last title is kept when only
one name word would remain, since that title is what separates "Miss Darcy"
from "Mr. Darcy". `EntityCandidate.text` keeps the full styled surface form,
and that is the form an uppercase-bearing `aliases` key is matched against. A
wholly lowercase key is a lexicon entry instead (`lexicon_mentions`): the phrase
is searched for in the sentence as a literal, word-bounded, case-insensitive
match and becomes a mention of the mapped canonical name at those offsets.
Longer keys are tried first, and a match overlapping a mention
already produced is dropped, so a key can add a mention but never replace one.

## Character profiles and mention resolution

`coreference.rs` first inventories named mentions and scores compatible short
forms. An inferred alias needs a unique full or titled name. Distinct forms
appearing together are kept apart. A titled surname needs agreeing gender
evidence and a unique full form. Caller aliases supply canonical IDs directly.
The narrator can come from a caller-supplied ID, an explicit self-introduction,
or the dialogue vote.

The profile learns descriptions from appositions and direct identifications.
It uses asserted named family statements and direct inverse
links for possessive and genitive descriptions such as "her sister" and
"the sister of Nora's sister". Every step needs one referent. Identity learned
later in the document can resolve an earlier mention.

Outside speech, pronouns prefer agreeing names in the current sentence and
then in the previous three sentences. At a paragraph boundary, only the
immediately preceding sentence stays in scope. The resolver checks the
document's gender cues and declines equally plausible referents. A resolved
mention carries the canonical ID and its own offsets through all relation
rules and event extraction. The standalone sentence helper `extract_entities`
uses the nearest in-sentence mention.

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
| `<Subj>, <Obj>'s <noun>` | "Elena, Marco's sister, arrived." | `<noun>_of` | `possessive-<noun>-pattern` |
| `<Subj>'s <noun> is <Obj>` | "Mingott's father was Bob Spicer." | `<noun>_of` | `possessive-<noun>-pattern` |
| `<Subj>, the <noun> of <Obj>` | "Charlotte, the wife of Mr. Collins." | `<noun>_of` | `of-genitive-<noun>-pattern` |
| `<Subj> ... mentor...` | "Marco mentors Dev." | `mentors` | `verb-mentor-pattern` |
| `<Subj> ... work... at...` | "Dev works at the Archive." | `works_at` | `verb-works-at-pattern` |
| `<Subj> ... , who ... work... at...` | "Marco mentors Dev, who works at the Archive." | `works_at` | `relative-works-at-pattern` |
| `<Subj> ... , who ... mentor...` | | `mentors` | `relative-mentor-pattern` |

The possessive row is driven by `possessive_nouns` in
`src/heuristic/pack.rs`, a lexicon of relational nouns each yielding
`<noun>_of` under its own rule name. It carries two confidence tiers: kinship
and role nouns ("sister", "employer", "apprentice") state the relation
outright, while social-stance nouns ("friend", "enemy", "rival") use the same
grammar for a weaker claim — stance is routinely negated, hypothetical, or
narrated from a character's mistaken view — and score lower. Nouns are matched
whole-word, so "grandmother" is never read as "mother".

Every one of those four rows requires the text between the two mentions to be
the whole link and nothing else: a bare singular copula (`is`, `was`), or an
appositive comma followed only by pre-nominal modifiers from the closed
`APPOSITIVE_MODIFIERS` list. Without that, "Elena visited Marco's sister" reads
as `sister_of(elena, marco)` — a third person's relation claimed for the
subject, at the highest confidence in the system. Admitting a verb or a
relative pronoun after the comma would do the same to "Elena, who visited
Marco's sister"; admitting a conjunction would read "Dev, and Marco's sister"
as one person rather than two. The relational noun must also head the possessed
phrase, which is what rejects "Marco's master key" and "Marco's friend's
sister", and the object must not open a possessive of its own, which is what
rejects "Elena's mother was Marco's sister".

The plural copulas are absent for a separate reason: "Dev and Elena are
Marco's cousins" has two subjects where the pair loop sees one, so the relation
would be claimed for whichever mention the loop reached.

The three non-copula rows exist because the copula row alone reads almost
nothing. `docs/EVALUATION.md` measures this: across three novels the
`<Subj> is <Obj>'s <noun>` form occurs zero times, while the "of" genitive
occurs nineteen. Adding these forms took recall over that corpus from one
candidate to six.

Across every row, text between the mentions that denies or suspends the
relation is read as a stance on it rather than ignored. An open condition
(`if`, `unless`, `whether`), a hedging modal (`could`, `would`, `might`, `may`,
`should`), a `to`-infinitive suspended by a governing verb ("refused to
mentor"), or a sentence ending in `?` **suspends** the relation: the text
settles nothing and no candidate is emitted. A negator (`not`, `never`, `no`,
or an `n't` clitic) **denies** it, which is the opposite claim rather than a
weaker one, and is emitted as such by `extract_aggregates` and dropped by
`extract_candidate_triples`, whose type has nowhere to say so. `will` is absent
from both sets: a future tense asserts.

A suspender outranks a negator, and a denial is read from a wider window than
an assertion — the suspender in "If Elena is not Marco's sister" sits before
the subject, outside the text between the mentions. An assertion keeps the
narrow window deliberately: widening it would drop claims the extractor has
always made.

This is a fixed pattern list, not a parser — relations outside this table are
not extracted, regardless of how clearly a human reader would infer them.
`normalize_relation` then checks the caller-supplied `ontology` map and
remaps the relation if a canonical name was requested; otherwise the
pattern's default relation name passes through unchanged.

## Phrase rules, kin prefixes and the document context

Beyond the possessive, genitive and verb rules above, `relations.rs` reads four
more shapes. All four run after the older rules, so an existing claim keeps its
label and rule name.

- **Phrase rules** (`pack/phrases.rs`). The text between two mentions, once
  fillers ("had", "never", "finally") and trailing determiners are peeled
  off, must *equal* one listed phrase: `married`, `lives in`, `works for`,
  `joined`, `owns`. Equal, not contains, so "met her friend Marco" is not
  `meets`. Each rule constrains both mentions with a `Slot` (a person, a
  place, a venue). `kinds.rs` classifies a mention from its own words (a title,
  a gazetteer name, a head word such as "Diner" or "Club", a determiner in
  front of it) and from the document register, and only ever rules a mention
  out: an unclassified name is accepted where a person is wanted and refused
  where a venue is.
- **Kin prefix.** "Bram's daughter Odile" and "his sister Elena": the possessor
  comes first and the relation reads `daughter_of(odile, bram)`. A possessive
  pronoun is trusted only when it has one possible antecedent in the sentence
  and agrees in gender with what the register knows.
- **Pronoun possessor.** "Cass, my sister": the possessor is a pronoun after
  the subject.
- **Plural pair.** "Nora and Marco are siblings" and its negation.

New rules also decline a clause opened by "if", "whether" or a modal, read no
pair across a quotation mark (`quotes.rs`), and decline a pronoun that has a
rival referent in its sentence. Inside speech a pronoun is never resolved, and
outside it a pronoun never takes an antecedent from inside speech, so
`"Thanks, Cass," she says` does not make Cass the speaker.

`Context` (`context.rs`) is the optional, additive way to give extraction more
than one paragraph. `Context::learn` feeds the register every paragraph before
any is extracted, so a name's gender and type are settled by the whole book;
`Context::with_narrator` names the first person so "I" and "my" outside speech
resolve to a referent. `extract_aggregates_in`, `extract_candidate_triples_in`
and `find_conflicts_in` take it; the original entry points are unchanged and
use an empty context.

## Confidence scoring (`src/heuristic/cooccurrence.rs`)

`score_confidence(rule, gap)` starts from a base score keyed on which pattern
matched (possessive patterns score highest, then direct verb patterns, then
relative-clause patterns, with a `0.6` floor for anything else), then applies
a proximity penalty scaled by `gap` — the character distance between the
subject and object — up to a 15% reduction as the gap approaches the 30-char
cap enforced in `find_relation_pattern`. A closer pair is weaker evidence of
coincidence than a distant one, so it keeps more of the base score.

The document timeline also reports estimates fitted on adjudicated predictions
from public LitBank books. Rule families pool observations and use four
prior observations centered on the heuristic score. Families with fewer
than five reviewed fit predictions have no estimate. [Evaluation](./EVALUATION.md)

## Positions, events and chapters

Occurrences are ordered by their original spans. The timeline retains the
rule's relationship state before ontology mapping and carries explicit
state endings through the conflict check. Events use the same resolved
mentions and quote boundaries and keep a separate record for each occurrence.

Dialogue lines start from quote ranges. The English reader joins a quote
across sentence breaks or an interleaved tag and uses the conversation
solver for missing attribution. The Chinese reader uses its own quote map
and named tags. Both keep partially attributed lines in source order.

Knowledge statements keep actor, recipient, named topic when available, and
the byte span of the proposition or topic phrase. A reported topic produces a
knowledge relation using the same alias, ontology, rejection and confidence
machinery as other triples.

`Series` indexes cached chapter analyses by book and chapter. A new revision
replaces the affected chapter's entries in the fact index. Use caller aliases
for stable series identities across surface names. The index stores the
extracted evidence and keeps spans relative to each chapter.

## Span and provenance

Each `RelationCandidate` (`src/heuristic/relations.rs`) carries the byte span
`[subj.start, obj.end]` taken directly from the entity candidates that
produced it, and the `rule` name of the pattern that matched. `mod.rs`
offsets that span by the sentence's start position in the original text to
produce the final `TripleCandidate.span`. No candidate is emitted without a
`rule`; there is no generic co-occurrence fallback path in this pipeline —
every candidate came from one of the named patterns above.

## The two collapses

Everything above produces one candidate per sentence window. Two entry points
collapse that list, and they differ only in what they do with a repeat.

`extract_candidate_triples` calls `dedup_candidates`, which groups by
`(subject, relation, object)` and keeps the highest-confidence candidate per
key, so a fact restated with different phrasing surfaces once — and the spans
of every other statement of it are discarded.

`extract_aggregates` keeps them: one `AggregateTriple` per
`(subject, relation, object, polarity)` carrying every span and every rule that
produced one. Its confidence is the best of them and repetition does not raise
it, because in fiction a restatement is not independent evidence; `spans.len()`
is what reports corroboration. Aggregates are ordered by first span rather than
by triple, which is what lets two facts about one pair be read in the order the
story states them.

`find_conflicts` reads those aggregates for claims that cannot both be true: a
fact asserted and denied, or a relation whose object takes one subject
(`mother_of`, `father_of`) asserted of two. A relation that simply changes over
a story is not a conflict — `enemy_of` early and `ally_of` late is a character
arc, and reporting it would bury real continuity errors under every arc in the
book.

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
  part-of-speech or named-entity model backing this. `sentence_openers` in
  `src/heuristic/pack.rs` drops a closed class of function words when they
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
- **Coreference coverage** depends on nearby discourse for pronouns and on
  named evidence for descriptions. Nicknames with different spellings require
  a caller alias, and pronouns within dialogue remain unresolved.
- **Segmentation edge cases**: see the two documented limitations in the
  segmentation section above.
- **Assertion gating is lexical**: the negation, condition and modal checks
  read only the text between the two mentions, so a denial expressed outside
  that window ("It was a lie. Elena is Marco's sister.") is not caught.
