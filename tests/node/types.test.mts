import { extractCandidateTriplesNapi, NapiOptions, NapiTripleCandidate } from '../../index.js'

// Minimal call must type-check with no options.
const bare: NapiTripleCandidate[] = extractCandidateTriplesNapi('Elena is Marco\'s sister.')
void bare

// Full options object must be assignable to NapiOptions.
const opts: NapiOptions = {
  aliases: { Marco: 'marco_reyes' },
  minConfidence: 0.5,
  ontology: { mentors: 'mentorship' },
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
