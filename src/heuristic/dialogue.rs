//! Who speaks to whom in a conversation.
//!
//! A dialogue tag outside the quotation marks names the speaker ("said Diana",
//! "Heathcliff asked"), and a name set off by punctuation inside the speech
//! names the listener ("Cathy, are you busy?"). By convention a paragraph is
//! one speaker's turn, so a turn's tag and address are joined across its
//! sentences.
//!
//! Turns also make a conversation, solved whole once two paragraphs pass
//! without speech: between two people the turns alternate, so once each side
//! of a stretch is tagged, every turn in it belongs to its side and is said
//! to the other person. See [`solve`].

use super::coreference::Sentence;
use super::entities::EntityCandidate;
use super::pack::LanguagePack;
use super::quotes::{is_quote_mark, QuoteMap};
use super::register::Register;
use super::relations::{normalize_relation, RelationCandidate, Stance};
use crate::types::{Dialogue, DialogueLine};
use std::collections::{BTreeMap, BTreeSet};

mod lines;

/// Base confidence of a tagged speaker and a vocative in the same paragraph.
const BASE: f32 = 0.7;
/// Base confidence when the speaker or the listener comes from the turns
/// around it rather than from the paragraph itself.
const INFERRED: f32 = 0.5;

pub const RULE: &str = "dialogue-vocative";
pub const TURN_RULE: &str = "dialogue-turn";

/// Paragraphs without speech after which a conversation is over.
const QUIET: usize = 2;

/// One paragraph's speech, as read.
#[derive(Default, Clone)]
struct Para {
    /// Names tagged as speaking it.
    speakers: Vec<String>,
    /// Pronouns tagging it ("she said"), lowercase.
    pronoun_tags: Vec<String>,
    /// Vocatives with their absolute byte span.
    vocatives: Vec<(String, usize, usize)>,
    /// Absolute byte ranges of the sentences that hold speech.
    speech: Vec<[usize; 2]>,
    lines: Vec<lines::Line>,
    /// The speech is still open at the paragraph's end.
    open_end: bool,
    /// A tag outside the speech carries on the last turn ("continued").
    resumed: bool,
    /// Tagged "I said".
    first_person: bool,
    /// The paragraph opens with a quotation mark, as a speech carried over
    /// from the paragraph before does.
    opens_quoted: bool,
}

#[derive(Default)]
pub struct Turn {
    current: Para,
    /// The conversation so far: its turns, a paragraph left open merged into
    /// the next.
    conversation: Vec<Para>,
    quiet: usize,
    /// How often each name is tagged as a speaker in the document.
    voices: BTreeMap<String, usize>,
    /// The previous turn: whether it was the narrator's, and whom it addressed.
    last_turn: Option<(bool, Vec<String>)>,
    /// Names addressed in a turn next to one of the narrator's, by count.
    narrator_votes: BTreeMap<String, usize>,
    lines: Option<Vec<DialogueLine>>,
}

impl Turn {
    pub(super) fn new(record_lines: bool) -> Self {
        Self {
            lines: record_lines.then(Vec::new),
            ..Self::default()
        }
    }

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
        let para = &mut self.current;
        if para.speech.is_empty() {
            para.opens_quoted = sentence.trim_start().starts_with(is_quote_mark);
        }
        para.speech.push([start, start + sentence.len()]);
        para.open_end = quotes.open_at_end;
        if self.lines.is_some() {
            lines::observe(
                &mut para.lines,
                sentence,
                start,
                mentions,
                quotes,
                open,
                pack,
            );
        }
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
                    para.vocatives
                        .push((m.normalized.clone(), start + m.start, start + m.end));
                }
            } else if m.pronoun && !first_person {
                // A resolved "she" outside speech was resolved against the
                // previous sentence, which in dialogue is usually the other
                // speaker's tag; the conversation decides who it is instead.
                if tags_speech(sentence, m, quotes, pack) {
                    para.pronoun_tags.push(m.text.to_lowercase());
                }
            } else if tags_speech(sentence, m, quotes, pack)
                && !para.speakers.contains(&m.normalized)
            {
                para.speakers.push(m.normalized.clone());
                *self.voices.entry(m.normalized.clone()).or_default() += 1;
            }
        }
        for (word, at) in words(sentence) {
            if quotes.inside(at) {
                continue;
            }
            let is_pronoun = |list: &[&str]| list.iter().any(|p| word.eq_ignore_ascii_case(p));
            let probe = EntityCandidate {
                text: word.to_string(),
                normalized: word.to_lowercase(),
                start: at,
                end: at + word.len(),
                pronoun: true,
                quoted: false,
            };
            if pack.resuming_verbs.contains(&word) {
                para.resumed = true;
            }
            if word == "I" && tags_speech(sentence, &probe, quotes, pack) {
                para.first_person = true;
            }
            if !mentions.iter().any(|m| m.start == at)
                && (is_pronoun(pack.masculine_pronouns) || is_pronoun(pack.feminine_pronouns))
                && is_pronoun(pack.subject_pronouns)
                && tags_speech(sentence, &probe, quotes, pack)
            {
                para.pronoun_tags.push(word.to_lowercase());
            }
        }
    }

    /// End the paragraph. A conversation is solved whole once it is over:
    /// after two paragraphs without speech, or with `last`, at the end of the
    /// document.
    pub fn finish(
        &mut self,
        ontology: &BTreeMap<String, String>,
        register: &Register,
        pack: &LanguagePack,
        last: bool,
    ) -> Vec<RelationCandidate> {
        let Some(turns) = self.close_paragraph(last) else {
            return Vec::new();
        };
        let mut output = ConversationOutput {
            relations: Vec::new(),
            lines: self.lines.as_mut(),
            ontology,
        };
        solve(&turns, register, pack, &mut output);
        output.relations
    }

    pub(super) fn line_names(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().flatten().flat_map(|line| {
            line.speaker
                .iter()
                .chain(&line.listener)
                .map(String::as_str)
        })
    }

    pub(super) fn into_dialogue(
        mut self,
        common: &BTreeSet<String>,
        register: &Register,
        pack: &LanguagePack,
        aliases: &BTreeMap<String, String>,
    ) -> Dialogue {
        let mut lines = self.lines.take().unwrap_or_default();
        let known: BTreeSet<&str> = aliases.values().map(String::as_str).collect();
        let allowed = |name: &String| {
            known.contains(name.as_str())
                || (!register.is_thing(name)
                    && !common.contains(name)
                    && !pack.sentence_openers.contains(&name.as_str())
                    && !matches!(
                        name.as_str(),
                        "anyone"
                            | "anybody"
                            | "someone"
                            | "somebody"
                            | "nobody"
                            | "everyone"
                            | "everybody"
                    ))
        };
        for line in &mut lines {
            line.speaker = line.speaker.take().filter(allowed);
            line.listener = line.listener.take().filter(|name| {
                allowed(name)
                    && (known.contains(name.as_str())
                        || register.is_established(name)
                        || is_voice(name, 1, &self, register))
            });
        }
        Dialogue::from_lines(lines)
    }

    fn close_paragraph(&mut self, last: bool) -> Option<Vec<Para>> {
        let mut para = std::mem::take(&mut self.current);
        // A turn addressing three people or more is recounting a
        // conversation, and its names are the recounted speakers' addresses.
        if para
            .vocatives
            .iter()
            .map(|v| &v.0)
            .collect::<BTreeSet<_>>()
            .len()
            > 2
        {
            para.vocatives.clear();
        }
        if para.speech.is_empty() {
            self.quiet += 1;
        } else {
            self.quiet = 0;
            self.vote(&para);
            // An apostrophe reads as a closing mark, so a quotation open at
            // the end is trusted only when the next paragraph reopens it and
            // names no other speaker.
            let merge = self.conversation.last().is_some_and(|previous| {
                let carried = previous.open_end && para.opens_quoted;
                (carried || para.resumed)
                    && para.speakers.iter().all(|s| previous.speakers.contains(s))
            });
            match self.conversation.last_mut() {
                Some(previous) if merge => {
                    previous.speakers.extend(para.speakers);
                    previous.speakers.dedup();
                    previous.pronoun_tags.extend(para.pronoun_tags);
                    previous.vocatives.extend(para.vocatives);
                    previous.speech.extend(para.speech);
                    previous.lines.extend(para.lines);
                    previous.open_end = para.open_end;
                    previous.first_person |= para.first_person;
                }
                _ => self.conversation.push(para),
            }
        }
        if self.quiet >= QUIET || last {
            self.last_turn = None;
            return Some(std::mem::take(&mut self.conversation));
        }
        None
    }

    /// The narrator is whoever the turns beside the narrator's address.
    fn vote(&mut self, para: &Para) {
        let names: Vec<String> = para.vocatives.iter().map(|(n, _, _)| n.clone()).collect();
        if let Some((last_first_person, last_names)) = self.last_turn.take() {
            let votes = match (last_first_person, para.first_person) {
                (false, true) => last_names,
                (true, false) => names.clone(),
                _ => vec![],
            };
            for name in votes {
                *self.narrator_votes.entry(name).or_default() += 1;
            }
        }
        self.last_turn = Some((para.first_person, names));
    }
}

struct ConversationOutput<'a> {
    relations: Vec<RelationCandidate>,
    lines: Option<&'a mut Vec<DialogueLine>>,
    ontology: &'a BTreeMap<String, String>,
}

impl ConversationOutput<'_> {
    fn record_lines(&mut self, turn: &Para, speaker: Option<&str>, listener: Option<&str>) {
        if let Some(lines) = &mut self.lines {
            lines.extend(
                turn.lines
                    .iter()
                    .map(|line| line.attributed(speaker, listener, &turn.vocatives)),
            );
        }
    }
}

pub(super) fn infer_narrator(sentences: &[Sentence<'_>], pack: &LanguagePack) -> Option<String> {
    let mut turn = Turn::default();
    for sentence in sentences {
        if sentence.paragraph_start {
            turn.close_paragraph(false);
        }
        turn.observe(
            sentence.text,
            sentence.start,
            sentence.named(),
            &sentence.quotes,
            sentence.opened,
            pack,
        );
    }
    turn.close_paragraph(true);
    turn.narrator()
}

/// Who speaks each turn of a conversation, and to whom.
///
/// A turn's own tag anchors it, and so does the turn before it addressing
/// someone other than its own speaker: the person addressed answers. The
/// conversation is cut wherever an anchor breaks alternation (a third name,
/// one person on both sides, two tags on one turn, or an address its own tag
/// contradicts), and each stretch whose anchors name two people gives every
/// turn to its side, before the first anchor and after the last, said to the
/// other person unless it addresses someone. A pronoun tag that fits only the
/// other side stops that inference for the stretch; tagged turns that address
/// someone still count.
fn solve(
    turns: &[Para],
    register: &Register,
    pack: &LanguagePack,
    out: &mut ConversationOutput<'_>,
) {
    let mut anchors: Vec<Option<String>> = turns
        .iter()
        .map(|t| match t.speakers.as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        })
        .collect();
    let tagged: Vec<bool> = anchors.iter().map(Option::is_some).collect();
    let mut breaks: Vec<bool> = turns.iter().map(|t| t.speakers.len() > 1).collect();
    for i in 1..turns.len() {
        let addressed = turns[i - 1]
            .vocatives
            .iter()
            .map(|(n, _, _)| n)
            .find(|n| Some(*n) != anchors[i - 1].as_ref());
        if let Some(name) = addressed {
            match &anchors[i] {
                None => anchors[i] = Some(name.clone()),
                Some(own) if own != name => breaks[i] = true,
                Some(_) => {}
            }
        }
    }

    let mut start = 0;
    // The two sides of the alternation, by the parity of the turn.
    let mut sides: [Option<String>; 2] = [None, None];
    let mut sides_tagged = [false, false];
    for i in 0..=turns.len() {
        let conflict = i < turns.len()
            && (breaks[i]
                || anchors[i].as_ref().is_some_and(|name| {
                    sides[i % 2].as_ref().is_some_and(|side| side != name)
                        || sides[1 - i % 2].as_ref() == Some(name)
                }));
        if i == turns.len() || conflict {
            // Each side needs a tag of its own: an address alone is too weak
            // to carry a whole stretch.
            let pair = match &sides {
                [Some(a), Some(b)] if sides_tagged == [true, true] => Some([a.clone(), b.clone()]),
                _ => None,
            };
            stretch(
                &turns[start..i],
                start,
                &tagged[start..i],
                pair,
                register,
                pack,
                out,
            );
            start = i;
            sides = [None, None];
            sides_tagged = [false, false];
        }
        if let Some(name) = anchors.get(i).and_then(Option::as_ref) {
            sides[i % 2].get_or_insert_with(|| name.clone());
            sides_tagged[i % 2] |= tagged[i];
        }
    }
}

/// One stretch of a conversation; `offset` is its first turn's index, which
/// `pair` is keyed to by parity.
fn stretch(
    turns: &[Para],
    offset: usize,
    tagged: &[bool],
    pair: Option<[String; 2]>,
    register: &Register,
    pack: &LanguagePack,
    out: &mut ConversationOutput<'_>,
) {
    let side = |j: usize| (offset + j) % 2;
    let Some(pair) = pair.filter(|pair| {
        !turns.iter().enumerate().any(|(j, t)| {
            t.pronoun_tags.iter().any(|p| {
                !register.allows(p, &pair[side(j)], pack)
                    && register.allows(p, &pair[1 - side(j)], pack)
            })
        })
    }) else {
        solve_tagged(turns, tagged, out);
        return;
    };

    for (j, turn) in turns.iter().enumerate() {
        let speaker = &pair[side(j)];
        let other = &pair[1 - side(j)];
        out.record_lines(turn, Some(speaker), Some(other));
        let addressed: Vec<&(String, usize, usize)> = turn
            .vocatives
            .iter()
            .filter(|(n, _, _)| n != speaker)
            .collect();
        if addressed.is_empty() {
            for &span in &turn.speech {
                out.relations.push(candidate(
                    speaker,
                    other,
                    span,
                    TURN_RULE,
                    INFERRED,
                    out.ontology,
                ));
            }
        }
        let (rule, base) = if tagged[j] {
            (RULE, BASE)
        } else {
            (TURN_RULE, INFERRED)
        };
        let mut seen: Vec<&str> = Vec::new();
        for (name, start, end) in addressed {
            out.relations.push(candidate(
                speaker,
                name,
                [*start, *end],
                rule,
                base,
                out.ontology,
            ));
            if !seen.contains(&name.as_str()) {
                seen.push(name);
                for &span in &turn.speech {
                    out.relations.push(candidate(
                        speaker,
                        name,
                        span,
                        TURN_RULE,
                        INFERRED,
                        out.ontology,
                    ));
                }
            }
        }
    }
}

/// Without a two-person alternation, only a tagged turn's own address.
fn solve_tagged(turns: &[Para], tagged: &[bool], out: &mut ConversationOutput<'_>) {
    for (turn, &tagged) in turns.iter().zip(tagged) {
        let speaker = tagged.then(|| turn.speakers[0].as_str());
        out.record_lines(turn, speaker, None);
        let Some(speaker) = speaker else { continue };
        for (name, start, end) in &turn.vocatives {
            if name != speaker {
                out.relations.push(candidate(
                    speaker,
                    name,
                    [*start, *end],
                    RULE,
                    BASE,
                    out.ontology,
                ));
            }
        }
    }
}

fn candidate(
    speaker: &str,
    listener: &str,
    span: [usize; 2],
    rule: &str,
    base: f32,
    ontology: &BTreeMap<String, String>,
) -> RelationCandidate {
    RelationCandidate {
        subject: speaker.to_string(),
        relation: normalize_relation("speaks_to", ontology),
        object: listener.to_string(),
        rule: rule.into(),
        span,
        base,
        gap: 0,
        stance: Stance::Asserted,
        single_filler: false,
        role: None,
        state: None,
        ends_state: false,
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
    speech_tag_span(sentence, m, quotes, pack).is_some()
}

fn speech_tag_span(
    sentence: &str,
    m: &EntityCandidate,
    quotes: &QuoteMap,
    pack: &LanguagePack,
) -> Option<[usize; 2]> {
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
            return Some([at, m.end]);
        }
    }
    let tail = &sentence[m.end..];
    let offset = m.end + (tail.len() - tail.trim_start().len());
    let next = tail
        .trim_start()
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or("");
    (!next.is_empty() && is_verb(next) && !quotes.inside(offset))
        .then_some([m.start, offset + next.len()])
}

/// A name that is the whole of a quoted fragment, with the speech resuming in
/// lowercase after the tag: `"Marco," Elena said, "married Nora."` is one
/// sentence split by its tag, and Marco is its subject, not its listener.
fn interrupted(sentence: &str, m: &EntityCandidate, quotes: &QuoteMap) -> bool {
    let Some((open, close)) = quotes.range_at(m.start) else {
        return false;
    };
    // A mention that runs past the closing mark is not wholly quoted.
    if m.end > close {
        return false;
    }
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
