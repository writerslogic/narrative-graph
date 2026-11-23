//! Who speaks to whom in a paragraph of dialogue.
//!
//! A dialogue tag outside the quotation marks names the speaker ("said Diana",
//! "Heathcliff asked"), and a name set off by punctuation inside the speech
//! names the listener ("Cathy, are you busy?"). By convention a paragraph is
//! one speaker's turn, so the two are joined across the paragraph's sentences
//! and only when the paragraph has exactly one tagged speaker.

use super::entities::EntityCandidate;
use super::pack::LanguagePack;
use super::quotes::{is_quote_mark, QuoteMap};
use super::register::Register;
use super::relations::{normalize_relation, RelationCandidate, Stance};
use std::collections::{BTreeMap, BTreeSet};

/// Base confidence of a speaker and a vocative in the same paragraph.
const BASE: f32 = 0.7;

pub const RULE: &str = "dialogue-vocative";

#[derive(Default)]
pub struct Turn {
    speakers: Vec<String>,
    /// Vocatives with their absolute byte span.
    vocatives: Vec<(String, usize, usize)>,
    /// How often each name is tagged as a speaker in the document.
    voices: BTreeMap<String, usize>,
}

impl Turn {
    /// Read one sentence of the paragraph; `start` is its byte offset.
    pub fn observe(
        &mut self,
        sentence: &str,
        start: usize,
        mentions: &[EntityCandidate],
        quotes: &QuoteMap,
        pack: &LanguagePack,
    ) {
        if !sentence.chars().any(is_quote_mark) {
            return;
        }
        for m in mentions {
            if m.quoted {
                if !m.pronoun
                    && !pack.honorifics.contains(&m.normalized.as_str())
                    && is_vocative(sentence, m)
                    && !interrupted(sentence, m, quotes)
                {
                    self.vocatives
                        .push((m.normalized.clone(), start + m.start, start + m.end));
                }
            } else if tags_speech(sentence, m, quotes, pack)
                && !self.speakers.contains(&m.normalized)
            {
                self.speakers.push(m.normalized.clone());
                *self.voices.entry(m.normalized.clone()).or_default() += 1;
            }
        }
    }

    /// End the paragraph: one candidate per listener of its single speaker,
    /// with an absolute span on the vocative.
    pub fn finish(&mut self, ontology: &BTreeMap<String, String>) -> Vec<RelationCandidate> {
        let speakers = std::mem::take(&mut self.speakers);
        let vocatives = std::mem::take(&mut self.vocatives);
        let [speaker] = speakers.as_slice() else {
            return vec![];
        };
        let mut out: Vec<RelationCandidate> = Vec::new();
        for (name, start, end) in vocatives {
            if &name == speaker || out.iter().any(|r| r.object == name) {
                continue;
            }
            out.push(RelationCandidate {
                subject: speaker.clone(),
                relation: normalize_relation("speaks_to", ontology),
                object: name,
                rule: RULE.into(),
                span: [start, end],
                base: BASE,
                gap: 0,
                stance: Stance::Asserted,
                single_filler: false,
            });
        }
        out
    }
}

/// Whether a speaker or listener is known to be a person: the register says
/// so, or the name is tagged as a speaker at least `tags` times. A capitalized
/// word set off in speech is as often an interjection ("Come,", "La!") as a
/// name, and a capital beside a speech verb is sometimes an adverb
/// ("Presently said ..."), so a speaker needs a second tag and a listener one.
pub fn is_voice(name: &str, tags: usize, turn: &Turn, register: &Register) -> bool {
    !register.is_thing(name)
        && (register.is_person(name) || turn.voices.get(name).is_some_and(|&n| n >= tags))
}

/// The single-word names among `names` that `text` writes in lowercase at
/// least as often as capitalized: words like "presently", "besides" or "yes"
/// that a capital at the start of speech or a sentence made look like names.
/// A name that is also a word ("Rose", "Will") is capitalized far more often
/// in a book that has the character.
pub fn common_words<'a>(text: &str, names: impl Iterator<Item = &'a str>) -> BTreeSet<String> {
    let mut counts: BTreeMap<String, (usize, usize)> = names
        .filter(|n| n.chars().all(char::is_alphabetic))
        .map(|n| (n.to_string(), (0, 0)))
        .collect();
    if counts.is_empty() {
        return BTreeSet::new();
    }
    for word in text.split(|c: char| !c.is_alphabetic()) {
        let Some(first) = word.chars().next() else {
            continue;
        };
        if let Some((lower, capital)) = counts.get_mut(word.to_lowercase().as_str()) {
            if first.is_lowercase() {
                *lower += 1;
            } else {
                *capital += 1;
            }
        }
    }
    counts
        .into_iter()
        .filter(|(_, (lower, capital))| lower >= capital)
        .map(|(name, _)| name)
        .collect()
}

/// A name in speech set off on both sides: by the opening mark or a comma
/// before it, and by a comma, a mark of address or the closing mark after.
fn is_vocative(sentence: &str, m: &EntityCandidate) -> bool {
    let before = sentence[..m.start].trim_end().chars().next_back();
    let after = sentence[m.end..].trim_start().chars().next();
    let opens = match before {
        None => true,
        Some(c) => c == ',' || is_quote_mark(c),
    };
    let closes = after.is_some_and(|c| matches!(c, ',' | '!' | '?' | ';') || is_quote_mark(c));
    opens && closes
}

/// A mention outside speech beside a speech verb: "said Diana", "Diana said".
fn tags_speech(
    sentence: &str,
    m: &EntityCandidate,
    quotes: &QuoteMap,
    pack: &LanguagePack,
) -> bool {
    let is_verb = |w: &str| pack.speech_verbs.contains(&w.to_lowercase().as_str());
    let head = sentence[..m.start].trim_end();
    let previous = head
        .rsplit(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or("");
    if !previous.is_empty()
        && head.len() == sentence[..m.start].trim_end().len()
        && is_verb(previous)
    {
        let at = head.len() - previous.len();
        if !quotes.inside(at) {
            return true;
        }
    }
    let tail = &sentence[m.end..];
    let offset = m.end + (tail.len() - tail.trim_start().len());
    let next = tail
        .trim_start()
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or("");
    !next.is_empty() && is_verb(next) && !quotes.inside(offset)
}

/// A name that is the whole of a quoted fragment, with the speech resuming in
/// lowercase after the tag: `"Marco," Elena said, "married Nora."` is one
/// sentence split by its tag, and Marco is its subject, not its listener.
fn interrupted(sentence: &str, m: &EntityCandidate, quotes: &QuoteMap) -> bool {
    let Some((open, close)) = quotes.range_at(m.start) else {
        return false;
    };
    let alone = |s: &str| {
        s.chars()
            .all(|c| c.is_whitespace() || c == ',' || is_quote_mark(c))
    };
    if !alone(&sentence[open..m.start]) || !alone(&sentence[m.end..close]) {
        return false;
    }
    let rest = &sentence[close..];
    let Some(next) = rest.find(is_quote_mark) else {
        return false;
    };
    let resumed = rest[next..].chars().skip(1).find(|c| !c.is_whitespace());
    resumed.is_some_and(char::is_lowercase)
}

#[cfg(test)]
mod tests {
    use super::common_words;

    #[test]
    fn a_word_written_lowercase_as_often_as_capitalized_is_not_a_name() {
        let text =
            "Presently Rose said no. Rose came presently. Rose left; a rose fell. Presently.";
        let common = common_words(text, ["presently", "rose", "miss_abbot"].into_iter());
        // presently: 1 lowercase, 2 capitalized; one more lowercase use ties it.
        assert!(common.is_empty(), "{common:?}");
        let common = common_words(
            &format!("{text} It ended presently."),
            ["presently", "rose"].into_iter(),
        );
        assert_eq!(common.into_iter().collect::<Vec<_>>(), ["presently"]);
    }
}
