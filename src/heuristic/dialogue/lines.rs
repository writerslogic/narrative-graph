use super::{interrupted, is_vocative, speech_tag_span};
use crate::heuristic::entities::EntityCandidate;
use crate::heuristic::pack::LanguagePack;
use crate::heuristic::quotes::{is_quote_mark, QuoteMap};
use crate::types::DialogueLine;
use std::collections::BTreeSet;

#[derive(Clone, Default)]
pub(super) struct Line {
    spoken: Vec<[usize; 2]>,
    speakers: BTreeSet<String>,
    listeners: BTreeSet<String>,
}

impl Line {
    pub(super) fn attributed(
        &self,
        turn_speaker: Option<&str>,
        other: Option<&str>,
        addresses: &[(String, usize, usize)],
    ) -> DialogueLine {
        let speaker = if self.speakers.is_empty() {
            turn_speaker
        } else {
            unique(self.speakers.iter().map(String::as_str))
        };
        let local = self
            .listeners
            .iter()
            .map(String::as_str)
            .filter(|&n| Some(n) != speaker);
        let listener = if local.clone().next().is_some() {
            unique(local)
        } else if speaker.is_some() && speaker == turn_speaker {
            let mut names = addresses
                .iter()
                .map(|(name, _, _)| name.as_str())
                .filter(|&name| Some(name) != speaker);
            match names.next() {
                Some(first) if names.all(|n| n == first) => Some(first),
                Some(_) => None,
                None => other.filter(|&n| Some(n) != speaker),
            }
        } else {
            None
        };
        DialogueLine {
            speaker: speaker.map(str::to_string),
            listener: listener.map(str::to_string),
            span: [
                self.spoken[0][0],
                self.spoken.last().expect("quoted span")[1],
            ],
            spoken_spans: self.spoken.clone(),
        }
    }

    fn append(&mut self, line: Self, continuous: bool) {
        if continuous {
            self.spoken.last_mut().expect("open quoted span")[1] = line.spoken[0][1];
        } else {
            self.spoken.extend(line.spoken);
        }
        self.speakers.extend(line.speakers);
        self.listeners.extend(line.listeners);
    }
}

fn unique<'a>(mut names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let name = names.next()?;
    names.next().is_none().then_some(name)
}

pub(super) fn observe(
    lines: &mut Vec<Line>,
    sentence: &str,
    offset: usize,
    mentions: &[EntityCandidate],
    quotes: &QuoteMap,
    open: bool,
    pack: &LanguagePack,
) {
    let ranges = quotes.ranges();
    if ranges.is_empty() {
        return;
    }
    let mut fragments = vec![Line::default(); ranges.len()];
    let mut resumes = vec![false; ranges.len()];
    for (index, mention) in mentions.iter().enumerate() {
        if mention.quoted {
            if !mention.pronoun && is_address(sentence, mention, quotes) {
                let upper = ranges.partition_point(|&(start, _)| start <= mention.start);
                if let Some(fragment) = upper.checked_sub(1).filter(|&i| mention.end <= ranges[i].1)
                {
                    fragments[fragment]
                        .listeners
                        .insert(mention.normalized.clone());
                }
            }
            continue;
        }
        if mention.pronoun
            && !pack
                .first_person_pronouns
                .iter()
                .any(|p| mention.text.eq_ignore_ascii_case(p))
        {
            continue;
        }
        let Some(mut tag) = speech_tag_span(sentence, mention, quotes, pack) else {
            continue;
        };
        let recipient = mentions[index + 1..]
            .iter()
            .find(|m| m.start >= tag[1])
            .filter(|m| !m.quoted && sentence[tag[1]..m.start].trim().eq_ignore_ascii_case("to"));
        if let Some(name) = recipient {
            tag[1] = name.end;
        }
        if let Some((fragment, resumed)) = attaches_to(sentence, ranges, tag) {
            fragments[fragment]
                .speakers
                .insert(mention.normalized.clone());
            if let Some(name) = recipient {
                fragments[fragment]
                    .listeners
                    .insert(name.normalized.clone());
            }
            if resumed {
                resumes[fragment + 1] = true;
            }
        }
    }
    for (i, ((start, end), mut line)) in ranges.iter().copied().zip(fragments).enumerate() {
        let continuous = i == 0 && open;
        let spoken = &sentence[start..end];
        let extends = end == sentence.len() && quotes.open_at_end;
        let previous = start > 0 && i > 0 && sentence[ranges[i - 1].1..start].trim().is_empty();
        let before = &sentence[if i == 0 { 0 } else { ranges[i - 1].1 }..start];
        let introduced = before.trim_end().ends_with([',', ':'])
            && before
                .rsplit(|c: char| !c.is_alphabetic())
                .filter(|word| !word.is_empty())
                .take(4)
                .any(|w| pack.speech_verbs.iter().any(|v| w.eq_ignore_ascii_case(v)));
        if !(continuous
            || extends
            || start == 0
            || previous
            || introduced
            || resumes[i]
            || !line.speakers.is_empty())
        {
            continue;
        }
        line.spoken.push([offset + start, offset + end]);
        if let Some(previous) = lines.last_mut().filter(|_| continuous || resumes[i]) {
            previous.append(line, continuous);
        } else if spoken
            .chars()
            .any(|c| !c.is_whitespace() && !is_quote_mark(c))
        {
            lines.push(line);
        }
    }
}

fn is_address(sentence: &str, mention: &EntityCandidate, quotes: &QuoteMap) -> bool {
    let final_address = sentence[..mention.start].trim_end().ends_with(',')
        && sentence[mention.end..].trim_start().starts_with('.');
    (is_vocative(sentence, mention) || final_address) && !interrupted(sentence, mention, quotes)
}

fn attaches_to(
    sentence: &str,
    ranges: &[(usize, usize)],
    [start, end]: [usize; 2],
) -> Option<(usize, bool)> {
    let split = ranges.partition_point(|&(_, close)| close <= start);
    let previous = split.checked_sub(1).filter(|&i| {
        sentence[ranges[i].1..start]
            .trim()
            .trim_matches(',')
            .trim()
            .is_empty()
    });
    let next = ranges.get(split).filter(|&&(open, _)| {
        open >= end
            && sentence[end..open]
                .chars()
                .all(|c| c.is_whitespace() || matches!(c, ',' | ':'))
    });
    let previous_comma = previous.is_some_and(|i| {
        sentence[ranges[i].0..ranges[i].1]
            .trim_end_matches(is_quote_mark)
            .trim_end()
            .ends_with(',')
    });
    if previous_comma {
        let resumed = next.is_some_and(|&(open, close)| {
            sentence[end..open].trim() == ","
                && sentence[open..close]
                    .trim_start_matches(is_quote_mark)
                    .trim_start()
                    .starts_with(char::is_lowercase)
        });
        return previous.map(|i| (i, resumed));
    }
    if next.is_some_and(|&(open, _)| matches!(sentence[end..open].trim(), "," | ":")) {
        Some((split, false))
    } else {
        previous
            .map(|i| (i, false))
            .or_else(|| next.map(|_| (split, false)))
    }
}
