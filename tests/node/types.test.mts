import { analyzeDocumentNapi, extractCandidateTriplesNapi, NarrativeSeries, NapiOptions, NapiTripleCandidate } from '../../index.js'

// Minimal call must type-check with no options.
const bare: NapiTripleCandidate[] = extractCandidateTriplesNapi('Elena is Marco\'s sister.')
void bare

// Full options object must be assignable to NapiOptions.
const opts: NapiOptions = {
  aliases: { Marco: 'marco_reyes' },
  minConfidence: 0.5,
  ontology: { mentors: 'mentorship' },
  language: 'english',
}
const withOpts: NapiTripleCandidate[] = extractCandidateTriplesNapi('Marco mentors Dev.', opts)
void withOpts

// Options is optional and may be omitted, undefined, or null.
extractCandidateTriplesNapi('text', undefined)
extractCandidateTriplesNapi('text', null)

// Each field of NapiTripleCandidate has its declared shape.
const [candidate] = bare
if (candidate) {
  const subject: string = candidate.subject
  const relation: string = candidate.relation
  const object: string = candidate.object
  const confidence: number = candidate.confidence
  const span: number[] = candidate.span
  const rule: string = candidate.rule
  void [subject, relation, object, confidence, span, rule]
}

// @ts-expect-error text is required, not optional.
extractCandidateTriplesNapi()

// @ts-expect-error minConfidence must be a number, not a string.
const badOpts: NapiOptions = { minConfidence: 'high' }
void badOpts

const document = analyzeDocumentNapi('Cass met Nora.')
const confidence: number | undefined = document.timeline[0]?.calibratedConfidence
const unattributed: number = document.dialogue.unattributed
const speaker: string | undefined = document.dialogue.lines[0]?.speaker
const spokenSpans: number[][] | undefined = document.dialogue.lines[0]?.spokenSpans
const series = new NarrativeSeries(opts)
const updated: boolean = series.updateChapter(1, 1, 1, 'Cass met Nora.')
const span: number[] | undefined = series.aggregates()[0]?.spans[0]?.span
const historySpan: number[] | undefined = series.relationshipHistory('cass', 'nora')[0]?.fact.candidate.span
void [confidence, unattributed, speaker, spokenSpans, updated, span, historySpan]
