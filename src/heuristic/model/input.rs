use crate::heuristic::{entities::EntityCandidate, extract_entities, pack::ENGLISH, segment};
use crate::Options;
use anyhow::{anyhow, ensure, Result};
use sentencepiece_rs::SentencePieceProcessor;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const CONTEXT_BYTES: usize = 6000;
const NEARBY_NAMES: usize = 12;
const CAST_NAMES: usize = 4;
pub(super) const MAX_NAMES: usize = NEARBY_NAMES + CAST_NAMES;
const MAX_NAME_TOKENS: usize = 8;
const MAX_SENTENCE_TOKENS: usize = 144;
pub(super) const MAX_TOKENS: usize = 320;

#[derive(serde::Serialize)]
pub struct Input {
    pub span: [usize; 2],
    pub ids: Vec<u32>,
    pub target: u32,
    pub names: Vec<String>,
    pub positions: Vec<u32>,
}

pub struct InputTokenizer {
    tokenizer: SentencePieceProcessor,
    cls: u32,
    sep: u32,
    pub(super) pad: u32,
}

impl InputTokenizer {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = super::read_artifact(path, 4 << 20)?;
        let tokenizer = SentencePieceProcessor::from_serialized_model(&bytes)?;
        ensure!(
            tokenizer.model().vocab_size() <= 128_100,
            "student vocabulary is too large"
        );
        let special = |token: &str| {
            tokenizer
                .model()
                .try_piece_to_id(token)
                .map(|id| id as u32)
                .ok_or_else(|| anyhow!("tokenizer is missing {token}"))
        };
        Ok(Self {
            cls: special("[CLS]")?,
            sep: special("[SEP]")?,
            pad: special("[PAD]")?,
            tokenizer,
        })
    }

    fn encode(&self, text: &str) -> Result<Vec<u32>> {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        Ok(self
            .tokenizer
            .encode_to_ids(&text)?
            .into_iter()
            .map(|id| id as u32)
            .collect())
    }

    pub fn prepare(&self, text: &str, spans: &[[usize; 2]], opts: &Options) -> Result<Vec<Input>> {
        let mut inputs = Vec::with_capacity(spans.len());
        self.for_batches(text, spans, opts, super::BATCH, |batch| {
            inputs.extend(batch);
            Ok(())
        })?;
        Ok(inputs)
    }

    pub(super) fn for_batches(
        &self,
        text: &str,
        spans: &[[usize; 2]],
        opts: &Options,
        batch_size: usize,
        mut visit: impl FnMut(Vec<Input>) -> Result<()>,
    ) -> Result<()> {
        if spans.is_empty() {
            return Ok(());
        }
        let names = DocumentNames::new(text, opts);
        for spans in spans.chunks(batch_size) {
            let batch = spans
                .iter()
                .map(|&span| self.prepare_sentence(text, span, &names))
                .collect::<Result<_>>()?;
            visit(batch)?;
        }
        Ok(())
    }

    fn prepare_sentence(
        &self,
        text: &str,
        span: [usize; 2],
        document: &DocumentNames,
    ) -> Result<Input> {
        let [start, end] = span;
        let sentence = text.get(start..end).filter(|s| !s.trim().is_empty());
        let Some(sentence) = sentence else {
            return Err(anyhow!("invalid sentence byte span {span:?}"));
        };
        let mut before = start.saturating_sub(CONTEXT_BYTES);
        while !text.is_char_boundary(before) {
            before += 1;
        }
        let names = document.candidates(before, span);
        let mut name_ids = Vec::new();
        for name in &names {
            let mut tokens = self.encode(&name.text)?;
            ensure!(!tokens.is_empty(), "empty spelling for a candidate name");
            tokens.truncate(MAX_NAME_TOKENS);
            name_ids.push(tokens);
        }
        let mut sentence_ids = self.encode(sentence)?;
        if sentence_ids.len() > MAX_SENTENCE_TOKENS {
            let trailing = sentence_ids.split_off(sentence_ids.len() - MAX_SENTENCE_TOKENS / 2);
            sentence_ids.truncate(MAX_SENTENCE_TOKENS / 2);
            sentence_ids.extend(trailing);
        }
        let context_ids = self.encode(&text[before..start])?;
        let context_limit = MAX_TOKENS
            - 3
            - sentence_ids.len()
            - name_ids
                .iter()
                .map(|tokens| tokens.len() + 1)
                .sum::<usize>();
        let context_start = context_ids.len().saturating_sub(context_limit);
        let mut ids = vec![self.cls];
        ids.extend_from_slice(&context_ids[context_start..]);
        let target = ids.len() as u32;
        ids.push(self.sep);
        ids.extend(sentence_ids);
        ids.push(self.sep);
        let mut positions = Vec::new();
        for tokens in name_ids {
            positions.push(ids.len() as u32);
            ids.extend(tokens);
            ids.push(self.sep);
        }
        Ok(Input {
            span,
            ids,
            target,
            positions,
            names: names
                .into_iter()
                .map(|name| name.normalized.clone())
                .collect(),
        })
    }
}

struct DocumentNames {
    mentions: Vec<EntityCandidate>,
    cast: Vec<usize>,
}

impl DocumentNames {
    fn new(text: &str, opts: &Options) -> Self {
        let mentions: Vec<_> = segment::split_sentences(text, &ENGLISH)
            .into_iter()
            .flat_map(|(sentence, offset)| {
                extract_entities(sentence, &opts.aliases)
                    .into_iter()
                    .filter(|entity| !entity.pronoun && entity.text.len() <= 100)
                    .map(move |mut entity| {
                        entity.start += offset;
                        entity.end += offset;
                        entity
                    })
            })
            .collect();
        let mut frequency = BTreeMap::new();
        for (index, name) in mentions.iter().enumerate() {
            let (count, _) = frequency.entry(&name.normalized).or_insert((0, index));
            *count += 1;
        }
        let mut frequent: Vec<_> = frequency
            .into_values()
            .filter(|(count, _)| *count >= 2)
            .collect();
        frequent.sort_unstable_by_key(|&(count, index)| (Reverse(count), index));
        let cast = frequent.into_iter().map(|(_, index)| index).collect();
        Self { mentions, cast }
    }

    fn candidates(&self, before: usize, [start, end]: [usize; 2]) -> Vec<&EntityCandidate> {
        let first = self.mentions.partition_point(|name| name.start < before);
        let current = self.mentions.partition_point(|name| name.start < start);
        let last = self.mentions.partition_point(|name| name.start < end);
        let mut seen = BTreeSet::new();
        let mut names: Vec<_> = self.mentions[current..last]
            .iter()
            .filter(|name| name.end <= end)
            .chain(
                self.mentions[first..current]
                    .iter()
                    .rev()
                    .filter(|name| name.end <= start),
            )
            .filter(|name| seen.insert(name.normalized.as_str()))
            .take(NEARBY_NAMES)
            .collect();
        names.extend(
            self.cast
                .iter()
                .map(|&index| &self.mentions[index])
                .filter(|name| seen.insert(name.normalized.as_str()))
                .take(CAST_NAMES),
        );
        names
    }
}
