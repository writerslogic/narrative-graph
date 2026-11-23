use crate::heuristic::{analyze_document_in, Context};
use crate::types::{
    CharacterProfile, DocumentAnalysis, FactOccurrence, FamilyLink, Gender, Options, Polarity,
};
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};

/// The order of a chapter in a series. Keep this key when editing the chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct ChapterPosition {
    pub book: u32,
    pub chapter: u32,
}

/// Evidence positions are relative to each chapter's own input text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct ChapterSpan {
    pub position: ChapterPosition,
    pub span: [usize; 2],
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS), ts(export))]
pub struct SeriesAggregate {
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub polarity: Polarity,
    pub confidence: f32,
    pub spans: Vec<ChapterSpan>,
    pub rules: Vec<String>,
}

type Fact = (String, String, String, Polarity);

/// An incremental graph over independently extracted chapters. Set stable
/// canonical IDs in [`Options::aliases`] for characters known by multiple names.
#[derive(Debug, Default)]
pub struct Series {
    options: Options,
    chapters: BTreeMap<ChapterPosition, (u64, DocumentAnalysis)>,
    facts: BTreeMap<Fact, BTreeMap<ChapterPosition, usize>>,
}

impl Series {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }

    /// Insert or replace a chapter. Pass the same revision for identical
    /// inputs to reuse the cached analysis. Returns whether extraction ran.
    pub fn update_chapter(
        &mut self,
        position: ChapterPosition,
        revision: u64,
        text: &str,
    ) -> Result<bool> {
        self.update_chapter_in(position, revision, text, &Context::default())
    }

    /// The revision identifies the chapter text and its supplied context.
    pub fn update_chapter_in(
        &mut self,
        position: ChapterPosition,
        revision: u64,
        text: &str,
        context: &Context,
    ) -> Result<bool> {
        if self.chapter_revision(position) == Some(revision) {
            return Ok(false);
        }
        let next = analyze_document_in(text, &self.options, context)?;
        self.remove_chapter(position);
        for (index, fact) in next.analysis.aggregates.iter().enumerate() {
            let key = (
                fact.subject.clone(),
                fact.relation.clone(),
                fact.object.clone(),
                fact.polarity,
            );
            self.facts.entry(key).or_default().insert(position, index);
        }
        self.chapters.insert(position, (revision, next));
        Ok(true)
    }

    pub fn remove_chapter(&mut self, position: ChapterPosition) -> bool {
        let Some((_, chapter)) = self.chapters.remove(&position) else {
            return false;
        };
        for fact in chapter.analysis.aggregates {
            let key = (fact.subject, fact.relation, fact.object, fact.polarity);
            let references = self
                .facts
                .get_mut(&key)
                .expect("chapter fact missing from series index");
            references.remove(&position);
            if references.is_empty() {
                self.facts.remove(&key);
            }
        }
        true
    }

    pub fn chapter(&self, position: ChapterPosition) -> Option<&DocumentAnalysis> {
        self.chapters.get(&position).map(|(_, analysis)| analysis)
    }

    pub fn chapter_revision(&self, position: ChapterPosition) -> Option<u64> {
        self.chapters.get(&position).map(|(revision, _)| *revision)
    }

    pub fn chapters(&self) -> impl Iterator<Item = (ChapterPosition, &DocumentAnalysis)> {
        self.chapters
            .iter()
            .map(|(&position, (_, analysis))| (position, analysis))
    }

    pub fn relationship_history<'a>(
        &'a self,
        person: &'a str,
        other: &'a str,
    ) -> impl Iterator<Item = (ChapterPosition, &'a FactOccurrence)> {
        self.chapters().flat_map(move |(position, chapter)| {
            chapter
                .relationship_history(person, other)
                .map(move |fact| (position, fact))
        })
    }

    /// Facts across all books, ordered by the position of their first evidence.
    pub fn aggregates(&self) -> Vec<SeriesAggregate> {
        let mut result = Vec::with_capacity(self.facts.len());
        for ((subject, relation, object, polarity), chapters) in &self.facts {
            let mut aggregate = SeriesAggregate {
                subject: subject.clone(),
                relation: relation.clone(),
                object: object.clone(),
                polarity: *polarity,
                confidence: 0.0,
                spans: Vec::new(),
                rules: Vec::new(),
            };
            for (&position, &index) in chapters {
                let evidence = &self.chapters[&position].1.analysis.aggregates[index];
                aggregate.confidence = aggregate.confidence.max(evidence.confidence);
                aggregate.spans.extend(
                    evidence
                        .spans
                        .iter()
                        .map(|&span| ChapterSpan { position, span }),
                );
                for rule in &evidence.rules {
                    if !aggregate.rules.contains(rule) {
                        aggregate.rules.push(rule.clone());
                    }
                }
            }
            result.push(aggregate);
        }
        result.sort_by_key(|a| a.spans[0]);
        result
    }

    pub fn characters(&self) -> Vec<CharacterProfile> {
        #[derive(Default)]
        struct Evidence<'a> {
            aliases: BTreeSet<&'a str>,
            genders: BTreeSet<Gender>,
            family: BTreeSet<(&'a str, &'a str)>,
        }
        let mut seen: BTreeMap<&str, Evidence<'_>> = BTreeMap::new();
        for (_, chapter) in self.chapters() {
            for character in &chapter.characters {
                let evidence = seen.entry(&character.id).or_default();
                evidence
                    .aliases
                    .extend(character.aliases.iter().map(String::as_str));
                evidence.genders.extend(character.gender);
                evidence.family.extend(
                    character
                        .family
                        .iter()
                        .map(|link| (link.role.as_str(), link.relative.as_str())),
                );
            }
        }
        seen.into_iter()
            .map(|(id, evidence)| CharacterProfile {
                id: id.to_string(),
                aliases: evidence.aliases.into_iter().map(str::to_string).collect(),
                gender: (evidence.genders.len() == 1).then(|| *evidence.genders.first().unwrap()),
                family: evidence
                    .family
                    .into_iter()
                    .map(|(role, relative)| FamilyLink {
                        role: role.to_string(),
                        relative: relative.to_string(),
                    })
                    .collect(),
            })
            .collect()
    }
}
