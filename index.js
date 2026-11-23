'use strict'

const binding = require('./native.js')

class NarrativeSeries {
  #handle

  constructor(opts) {
    this.#handle = binding.createSeries(opts)
  }

  updateChapter(book, chapter, revision, text) {
    return binding.updateSeriesChapter(this.#handle, book, chapter, revision, text)
  }

  removeChapter(book, chapter) {
    return binding.removeSeriesChapter(this.#handle, book, chapter)
  }

  chapter(book, chapter) {
    return binding.seriesChapter(this.#handle, book, chapter)
  }

  aggregates() {
    return binding.seriesAggregates(this.#handle)
  }

  relationshipHistory(person, other) {
    return binding.seriesRelationshipHistory(this.#handle, person, other)
  }

  characters() {
    return binding.seriesCharacters(this.#handle)
  }
}

module.exports.NarrativeSeries = NarrativeSeries
module.exports.analyzeAsyncNapi = binding.analyzeAsyncNapi
module.exports.analyzeDocumentNapi = binding.analyzeDocumentNapi
module.exports.analyzeNapi = binding.analyzeNapi
module.exports.extractAggregatesNapi = binding.extractAggregatesNapi
module.exports.extractCandidateTriplesNapi = binding.extractCandidateTriplesNapi
module.exports.findConflictsNapi = binding.findConflictsNapi
