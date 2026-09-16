'use strict'

const test = require('node:test')
const assert = require('node:assert/strict')
const { extractCandidateTriplesNapi } = require('../../index.js')

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
