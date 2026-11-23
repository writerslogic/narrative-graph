//! Who speaks to whom in a conversation.
//!
//! A dialogue tag outside the quotation marks names the speaker ("said Diana",
//! "Heathcliff asked"), and a name set off by punctuation inside the speech
//! names the listener ("Cathy, are you busy?"). By convention a paragraph is
//! one speaker's turn, so a turn's tag and address are joined across its
//! sentences.
//!
//! Turns also carry a conversation. Between two people the turns alternate,
//! so an untagged turn belongs to whoever spoke the turn before last, a turn
//! tagged only "she said" to whichever of the two the pronoun fits, and
//! either one is said to the other. A turn whose speaker stays unknown, or
//! two paragraphs without speech, end the conversation.

use super::entities::EntityCandidate;
use super::pack::LanguagePack;
use super::quotes::{is_quote_mark, QuoteMap};
use super::register::Register;
use super::relations::{normalize_relation, RelationCandidate, Stance};
use std::collections::{BTreeMap, BTreeSet};

/// Base confidence of a tagged speaker and a vocative in the same paragraph.
const BASE: f32 = 0.7;
/// Base confidence when the speaker or the listener comes from the turns
/// around it rather than from the paragraph itself.
const INFERRED: f32 = 0.5;

pub const RULE: &str = "dialogue-vocative";
pub const TURN_RULE: &str = "dialogue-turn";

/// Paragraphs without speech after which a conversation is over.
const QUIET: usize = 2;

#[derive(Default)]
pub struct Turn {
    speakers: Vec<String>,
    /// Pronouns tagging speech in this paragraph ("she said"), lowercase.
    pronoun_tags: Vec<String>,
    /// Vocatives with their absolute byte span.
    vocatives: Vec<(String, usize, usize)>,
    /// Absolute byte ranges of the paragraph's sentences that hold speech.
    speech: Vec<[usize; 2]>,
    /// How often each name is tagged as a speaker in the document.
    voices: BTreeMap<String, usize>,
    /// Speakers of the conversation so far, oldest first, no two adjacent
    /// alike.
    exchange: Vec<String>,
    quiet: usize,
    /// Whether the paragraph's speech is still open at its end.
    open_end: bool,
    /// Whether the previous paragraph's speech was left open, so that this
    /// one continues the same speaker's turn.
    continued: bool,
    /// The name the previous turn addressed, who is the likely next speaker.
    addressed: Option<String>,
    /// Whether this paragraph's speech is tagged "I said".
    first_person: bool,
    /// Whether a tag outside the speech carries on the last turn ("continued").
    resumed: bool,
    /// The previous turn: whether it was the narrator's, and whom it addressed.
    last_turn: Option<(bool, Vec<String>)>,
    /// Names addressed in a turn next to one of the narrator's, by count.
    narrator_votes: BTreeMap<String, usize>,
}

impl Turn {
    /// Read one sentence of the paragraph; `start` is its byte offset and
    /// `open` whether a quotation was open when it began.
    pub fn observe(
        &mut self,
        sentence: &str,
        start: usize,
        mentions: &[EntityCandidate],
        quotes: &QuoteMap,
        open: bool,
        pack: &LanguagePack,
    ) {
        if !open && !sentence.chars().any(is_quote_mark) {
            return;
        }
        self.speech.push([start, start + sentence.len()]);
        self.open_end = quotes.open_at_end;
        for m in mentions {
            let first_person = pack
                .first_person_pronouns
                .iter()
                .any(|p| m.text.eq_ignore_ascii_case(p));
            if m.quoted {
                if !m.pronoun
                    && !pack.honorifics.contains(&m.normalized.as_str())
                    && !pack.sentence_openers.contains(&m.normalized.as_str())
                    && is_vocative(sentence, m)
                    && !interrupted(sentence, m, quotes)
                {
                    self.vocatives
                        .push((m.normalized.clone(), start + m.start, start + m.end));
                }
            } else if m.pronoun && !first_person {
                // A resolved "she" outside speech was resolved against the
                // previous sentence, which in dialogue is usually the other
                // speaker's tag; the conversation decides who it is instead.
                if tags_speech(sentence, m, quotes, pack) {
                    self.pronoun_tags.push(m.text.to_lowercase());
                }
            } else if tags_speech(sentence, m, quotes, pack)
                && !self.speakers.contains(&m.normalized)
            {
                self.speakers.push(m.normalized.clone());
                *self.voices.entry(m.normalized.clone()).or_default() += 1;
            }
        }
        for (word, at) in words(sentence) {
            let is_pronoun = |list: &[&str]| list.iter().any(|p| word.eq_ignore_ascii_case(p));
            let covered = mentions.iter().any(|m| m.start == at);
            let probe = |word: &str| EntityCandidate {
                text: word.to_string(),
                normalized: word.to_lowercase(),
                start: at,
                end: at + word.len(),
                pronoun: true,
                quoted: false,
            };
            if word == "I"
                && !quotes.inside(at)
                && tags_speech(sentence, &probe(word), quotes, pack)
            {
                self.first_person = true;
            }
            if !covered
                && !quotes.inside(at)
                && (is_pronoun(pack.masculine_pronouns) || is_pronoun(pack.feminine_pronouns))
                && is_pronoun(pack.subject_pronouns)
                && tags_speech(sentence, &probe(word), quotes, pack)
            {
                self.pronoun_tags.push(word.to_lowercase());
            }
        }
    }

    /// End the paragraph: its speaker `speaks_to` each listener, with a span
    /// on each vocative and on each sentence of the speech.
    pub fn finish(
        &mut self,
        ontology: &BTreeMap<String, String>,
        register: &Register,
        pack: &LanguagePack,
    ) -> Vec<RelationCandidate> {
        let speakers = std::mem::take(&mut self.speakers);
        let pronoun_tags = std::mem::take(&mut self.pronoun_tags);
        let vocatives = std::mem::take(&mut self.vocatives);
        let speech = std::mem::take(&mut self.speech);
        let continued = std::mem::replace(&mut self.continued, false);
        let addressed = self.addressed.take();
        let first_person = std::mem::take(&mut self.first_person);
        let resumed = std::mem::take(&mut self.resumed);
        if speech.is_empty() {
            self.quiet += 1;
            if self.quiet >= QUIET {
                self.exchange.clear();
                self.last_turn = None;
            }
            return vec![];
        }
        self.quiet = 0;
        // The narrator is whoever the turns beside the narrator's address.
        let names: Vec<String> = vocatives.iter().map(|(n, _, _)| n.clone()).collect();
        if let Some((last_first_person, last_names)) = self.last_turn.take() {
            let votes = match (last_first_person, first_person) {
                (false, true) => last_names,
                (true, false) => names.clone(),
                _ => vec![],
            };
            for name in votes {
                *self.narrator_votes.entry(name).or_default() += 1;
            }
        }
        self.last_turn = Some((first_person, names));
        self.continued = std::mem::take(&mut self.open_end);

        // The two people the conversation is between, most recent last.
        // Only a conversation between two people alternates: with a third
        // voice in it, the next turn may be anyone's.
        let two = |exchange: &[String]| exchange.iter().collect::<BTreeSet<_>>().len() == 2;
        let pair: Option<[String; 2]> = match self.exchange.as_slice() {
            e @ [.., a, b] if two(e) => Some([a.clone(), b.clone()]),
            _ => None,
        };
        let previous = self.exchange.last().cloned();
        let fits = |pronoun: &str, n: &str| register.allows(pronoun, n, pack);
        let (speaker, tagged) = match (speakers.as_slice(), pronoun_tags.as_slice()) {
            ([one], _) => (Some(one.clone()), true),
            // Speech left open at the end of a paragraph resumes in the next.
            ([], []) if continued || resumed => (previous.clone(), false),
            // Whoever the last turn addressed answers it.
            ([], tags)
                if tags.len() <= 1
                    && addressed.as_ref().is_some_and(|a| {
                        Some(a) != previous.as_ref() && tags.iter().all(|t| fits(t, a))
                    }) =>
            {
                (addressed.clone(), false)
            }
            ([], [pronoun]) => (
                pair.as_ref()
                    .and_then(|[a, b]| match (fits(pronoun, a), fits(pronoun, b)) {
                        (true, false) => Some(a.clone()),
                        (false, true) => Some(b.clone()),
                        // Both fit: the turns alternate.
                        (true, true) => Some(a.clone()),
                        (false, false) => None,
                    }),
                false,
            ),
            ([], []) => (pair.as_ref().map(|[a, _]| a.clone()), false),
            _ => (None, false),
        };
        let Some(speaker) = speaker else {
            self.exchange.clear();
            self.addressed = vocatives.first().map(|(n, _, _)| n.clone());
            return vec![];
        };
        if self.exchange.last() != Some(&speaker) {
            self.exchange.push(speaker.clone());
        }
        self.addressed = vocatives
            .iter()
            .map(|(n, _, _)| n)
            .find(|n| **n != speaker)
            .cloned();

        let mut out: Vec<RelationCandidate> = Vec::new();
        let mut emit = |listener: &str, span: [usize; 2], rule: &str, base: f32| {
            out.push(RelationCandidate {
                subject: speaker.clone(),
                relation: normalize_relation("speaks_to", ontology),
                object: listener.to_string(),
                rule: rule.into(),
                span,
                base,
                gap: 0,
                stance: Stance::Asserted,
                single_filler: false,
            });
        };
        let mut listeners: Vec<String> = Vec::new();
        for (name, start, end) in vocatives {
            if name == speaker || listeners.contains(&name) {
                continue;
            }
            let (rule, base) = if tagged {
                (RULE, BASE)
            } else {
                (TURN_RULE, INFERRED)
            };
            emit(&name, [start, end], rule, base);
            listeners.push(name);
        }
        if listeners.is_empty() {
            // Between two people, each turn is said to the other.
            // A turn answers the one before it: said to its speaker.
            let other = match self.exchange.as_slice() {
                [.., a, b] if *b == speaker && (tagged || two(&self.exchange)) => Some(a.clone()),
                _ => None,
            };
            listeners.extend(other);
        }
        for listener in &listeners {
            for &span in &speech {
                emit(listener, span, TURN_RULE, INFERRED);
            }
        }
        out
    }
}

impl Turn {
    /// The name the narrator goes by, when the document is told in the first
    /// person and the turns beside the narrator's address one name at least
    /// three times and twice as often as any other.
    pub fn narrator(&self) -> Option<String> {
        let mut ranked: Vec<(&String, &usize)> = self.narrator_votes.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1));
        match ranked.as_slice() {
            [(name, &n), rest @ ..] if n >= 3 && rest.first().map_or(true, |r| n >= r.1 * 2) => {
                Some((*name).clone())
            }
            _ => None,
        }
    }
}

/// Whole words of `text` with their byte offsets.
fn words(text: &str) -> impl Iterator<Item = (&str, usize)> {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(move |w| (w, w.as_ptr() as usize - text.as_ptr() as usize))
}

/// Whether a candidate came from this module.
pub fn is_rule(rule: &str) -> bool {
    rule == RULE || rule == TURN_RULE
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
    use super::{common_words, Turn};

    #[test]
    fn a_narrator_needs_three_votes_and_twice_the_runner_up() {
        let narrator = |votes: &[(&str, usize)]| {
            Turn {
                narrator_votes: votes.iter().map(|(n, c)| (n.to_string(), *c)).collect(),
                ..Turn::default()
            }
            .narrator()
        };
        assert_eq!(narrator(&[("jane", 2)]), None);
        assert_eq!(narrator(&[("jane", 3)]).as_deref(), Some("jane"));
        assert_eq!(
            narrator(&[("jane", 4), ("rose", 2)]).as_deref(),
            Some("jane")
        );
        assert_eq!(narrator(&[("jane", 4), ("rose", 3)]), None);
    }

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
