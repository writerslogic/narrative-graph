'use strict'

const test = require('node:test')
const assert = require('node:assert/strict')
const {
  extractAggregatesNapi,
  extractCandidateTriplesNapi,
  findConflictsNapi,
} = require('../../index.js')

test('extracts a possessive-sister relation with a confident score', () => {
  const candidates = extractCandidateTriplesNapi("Elena is Marco's sister.")

  assert.equal(candidates.length, 1)
  const [candidate] = candidates
  assert.equal(candidate.subject, 'elena')
  assert.equal(candidate.relation, 'sister_of')
  assert.equal(candidate.object, 'marco')
  assert.equal(candidate.rule, 'possessive-sister-pattern')
  assert.ok(candidate.confidence > 0.7)
  assert.deepEqual(candidate.span, [0, 23])
  const text = "Elena is Marco's sister."
  assert.equal(text.slice(candidate.span[0], candidate.span[1]), "Elena is Marco's sister")
})

test('extracts a verb-mentor relation', () => {
  const candidates = extractCandidateTriplesNapi('Marco mentors Dev.')

  assert.equal(candidates.length, 1)
  assert.equal(candidates[0].subject, 'marco')
  assert.equal(candidates[0].relation, 'mentors')
  assert.equal(candidates[0].object, 'dev')
})

test('returns an empty array for text with no entities', () => {
  const candidates = extractCandidateTriplesNapi('the quiet room held nothing.')
  assert.deepEqual(candidates, [])
})

test('returns an empty array for empty input', () => {
  const candidates = extractCandidateTriplesNapi('')
  assert.deepEqual(candidates, [])
})

test('filters candidates below minConfidence', () => {
  const text = "Elena is Marco's sister. Marco mentors Dev."
  const all = extractCandidateTriplesNapi(text)
  assert.ok(all.length > 0)

  const filtered = extractCandidateTriplesNapi(text, { minConfidence: 0.8 })
  for (const candidate of filtered) {
    assert.ok(candidate.confidence >= 0.8)
  }
  assert.ok(filtered.length <= all.length)
})

test('resolves aliases to a canonical entity name', () => {
  const candidates = extractCandidateTriplesNapi("Elena is Marco's sister.", {
    aliases: { Marco: 'marco_reyes' },
  })

  assert.equal(candidates.length, 1)
  assert.equal(candidates[0].object, 'marco_reyes')
})

test('drops a leading title from the identity but keys aliases on the surface form', () => {
  const [styled] = extractCandidateTriplesNapi(
    'Lady Catherine de Bourgh, widow of Sir Lewis de Bourgh, said nothing.',
  )

  assert.equal(styled.subject, 'catherine_de_bourgh')
  assert.equal(styled.object, 'lewis_de_bourgh')

  // The alias key is the surface form, titles and all, with `.` dropped as a
  // separator. This is the form README and docs/INTEGRATION.md document.
  const [aliased] = extractCandidateTriplesNapi("Mr. Marcus Hale is Elena's brother.", {
    aliases: { 'Mr Marcus Hale': 'marcus_hale' },
  })

  assert.equal(aliased.subject, 'marcus_hale')
})

test('maps a recognized relation through a caller-supplied ontology', () => {
  const candidates = extractCandidateTriplesNapi('Marco mentors Dev.', {
    ontology: { mentors: 'mentorship' },
  })

  assert.equal(candidates.length, 1)
  assert.equal(candidates[0].relation, 'mentorship')
})

test('rejects an out-of-range minConfidence', () => {
  assert.throws(() => {
    extractCandidateTriplesNapi('Marco mentors Dev.', { minConfidence: 1.5 })
  }, /confidence threshold/i)
})

test('suppresses a rejected triple and leaves every other one', () => {
  const text = "Elena is Marco's sister. Marco mentors Dev."
  const all = extractCandidateTriplesNapi(text)
  assert.equal(all.length, 2)

  const kept = extractCandidateTriplesNapi(text, {
    rejections: [{ subject: 'elena', relation: 'sister_of', object: 'marco' }],
  })

  assert.equal(kept.length, 1)
  assert.equal(kept[0].relation, 'mentors')
})

test('aggregates one fact across a passage, in the order the story states it', () => {
  const text = "Elena is Marco's enemy. Dev works at the Archive. Elena is Marco's ally."
  const aggregates = extractAggregatesNapi(text)

  assert.deepEqual(
    aggregates.map((a) => a.relation),
    ['enemy_of', 'works_at', 'ally_of'],
  )

  const repeated = extractAggregatesNapi("Elena is Marco's sister. Elena is Marco's sister.")
  assert.equal(repeated.length, 1)
  assert.equal(repeated[0].spans.length, 2)
  assert.deepEqual(repeated[0].rules, ['possessive-sister-pattern'])
})

test('records a denial and flags it against the assertion of the same fact', () => {
  const text = "Elena is Marco's sister. Later, Elena is not Marco's sister."

  const aggregates = extractAggregatesNapi(text)
  assert.deepEqual(
    aggregates.map((a) => a.polarity),
    ['asserted', 'denied'],
  )

  const conflicts = findConflictsNapi(text)
  assert.equal(conflicts.length, 1)
  assert.equal(conflicts[0].kind, 'denial')
  assert.equal(conflicts[0].left.polarity, 'asserted')
  assert.equal(conflicts[0].right.polarity, 'denied')

  // A stance that changes over a story is a character arc, not a conflict.
  assert.equal(findConflictsNapi("Elena is Marco's enemy. Later, Elena is Marco's ally.").length, 0)
})

test('links a pronoun to a referent the previous sentence named', () => {
  const linked = extractCandidateTriplesNapi('Mrs. Bennet joined the Archive. She works at it.')

  assert.equal(linked.length, 1)
  assert.equal(linked[0].subject, 'mrs_bennet')
  assert.equal(linked[0].object, 'archive')

  // "He" contradicts what the document says about her, so nothing is claimed.
  const mismatched = extractCandidateTriplesNapi(
    'Mrs. Bennet joined the Archive. He works at it.',
  )
  assert.equal(mismatched.length, 0)
})
