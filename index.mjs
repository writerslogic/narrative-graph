import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const binding = require('./index.js')

export const extractCandidateTriplesNapi = binding.extractCandidateTriplesNapi
export const extractAggregatesNapi = binding.extractAggregatesNapi
export const findConflictsNapi = binding.findConflictsNapi
export const analyzeNapi = binding.analyzeNapi
export const analyzeAsyncNapi = binding.analyzeAsyncNapi
export const analyzeDocumentNapi = binding.analyzeDocumentNapi
export const NarrativeSeries = binding.NarrativeSeries
export default binding
