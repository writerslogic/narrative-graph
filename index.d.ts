import type {
  NapiChapterFactOccurrence,
  NapiCharacterProfile,
  NapiDocumentAnalysis,
  NapiOptions,
  NapiSeriesAggregate,
} from './native.js'

export {
  analyzeAsyncNapi,
  analyzeDocumentNapi,
  analyzeNapi,
  extractAggregatesNapi,
  extractCandidateTriplesNapi,
  findConflictsNapi,
} from './native.js'

export type {
  NapiAggregateTriple,
  NapiAnalysis,
  NapiChapterFactOccurrence,
  NapiChapterSpan,
  NapiCharacterMention,
  NapiCharacterProfile,
  NapiConflict,
  NapiDialogue,
  NapiDialogueLine,
  NapiDocumentAnalysis,
  NapiEventCandidate,
  NapiFactOccurrence,
  NapiFamilyLink,
  NapiKnowledgeFact,
  NapiOptions,
  NapiRejection,
  NapiSeriesAggregate,
  NapiTripleCandidate,
} from './native.js'

export declare class NarrativeSeries {
  #private
  constructor(opts?: NapiOptions | null)
  updateChapter(book: number, chapter: number, revision: number, text: string): boolean
  removeChapter(book: number, chapter: number): boolean
  chapter(book: number, chapter: number): NapiDocumentAnalysis | null
  aggregates(): NapiSeriesAggregate[]
  relationshipHistory(person: string, other: string): NapiChapterFactOccurrence[]
  characters(): NapiCharacterProfile[]
}
