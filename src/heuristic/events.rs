use super::coreference::genitive_head;
use super::entities::EntityCandidate;
use super::kinds::{classify, Kind};
use super::pack::{LanguagePack, Slot};
use super::relations::{
    conditioned_before, ends_in_question, opens_possessive, pronoun_fits_phrase, Scope,
};
use crate::types::{EventCandidate, EventKind};

const PREDICATES: &[(EventKind, &[&str])] = &[
    (
        EventKind::Duel,
        &["fought a duel with", "duelled", "dueled", "duels"],
    ),
    (
        EventKind::Proposal,
        &["proposed marriage to", "proposed to", "proposes to"],
    ),
    (
        EventKind::Journey,
        &[
            "traveled to",
            "travelled to",
            "journeyed to",
            "went to",
            "sailed to",
            "rode to",
            "departed for",
            "set out for",
            "arrived in",
            "arrived at",
        ],
    ),
    (EventKind::Death, &["passed away", "died", "dies"]),
];

pub(super) fn extract(
    text: &str,
    offset: usize,
    mentions: &[EntityCandidate],
    pack: &LanguagePack,
    scope: &Scope<'_>,
) -> Vec<EventCandidate> {
    let person = |m: &EntityCandidate| Slot::Person.admits(classify(text, m, scope.register, pack));
    let place = |m: &EntityCandidate| {
        !m.pronoun
            && matches!(
                classify(text, m, scope.register, pack),
                Kind::Place | Kind::Org | Kind::Unknown
            )
    };
    let mut events = Vec::new();
    for (i, actor) in mentions.iter().enumerate() {
        let Some((kind, verb_end)) = predicate(text, actor.end) else {
            continue;
        };
        if !person(actor)
            || !pronoun_fits_phrase(text, actor, pack, true)
            || opens_possessive(text, actor.end, pack)
            || genitive_head(text, actor.start, pack).is_some()
            || conditioned_before(text, actor.start, pack)
        {
            continue;
        }
        let at = if kind == EventKind::Journey {
            after_article(text, verb_end)
        } else {
            skip_space(text, verb_end)
        };
        let remaining = &mentions[i + 1..];
        let mention_at = |at| {
            remaining.iter().find(|m| {
                m.start == at
                    && !opens_possessive(text, m.end, pack)
                    && pronoun_fits_phrase(text, m, pack, false)
            })
        };
        let object = mention_at(at);
        let target = match kind {
            EventKind::Duel | EventKind::Proposal => {
                let Some(target) = object.filter(|m| m.normalized != actor.normalized && person(m))
                else {
                    continue;
                };
                Some(target)
            }
            _ => None,
        };
        let event_end = target.map_or(verb_end, |m| m.end);
        let location = if kind == EventKind::Journey {
            let Some(location) = object.filter(|m| place(m)) else {
                continue;
            };
            Some(location)
        } else {
            place_after(text, event_end)
                .and_then(mention_at)
                .filter(|m| place(m))
        };
        let end = location.map_or(event_end, |location| location.end);
        if ends_in_question(text, end) || !scope.quotes.contains_span(text, actor.start, end) {
            continue;
        }
        // A proposal followed by "that" offers an idea. "Died laughing" is an idiom.
        if matches!(kind, EventKind::Proposal | EventKind::Death) && !valid_tail(&text[end..]) {
            continue;
        }
        events.push(EventCandidate {
            kind,
            actor: actor.normalized.clone(),
            target: target.map(|m| m.normalized.clone()),
            location: location.map(|m| m.normalized.clone()),
            span: [offset + actor.start, offset + end],
            trigger: [offset + skip_space(text, actor.end), offset + verb_end],
        });
    }
    events
}

fn skip_space(text: &str, at: usize) -> usize {
    text.len() - text[at..].trim_start().len()
}

fn predicate(text: &str, at: usize) -> Option<(EventKind, usize)> {
    let start = skip_space(text, at);
    if start == at {
        return None;
    }
    for &(kind, phrases) in PREDICATES {
        for phrase in phrases {
            let end = start + phrase.len();
            if text
                .get(start..end)
                .is_some_and(|s| s.eq_ignore_ascii_case(phrase))
                && !text[end..].starts_with(char::is_alphanumeric)
            {
                return Some((kind, end));
            }
        }
    }
    None
}

fn place_after(text: &str, at: usize) -> Option<usize> {
    let start = skip_space(text, at);
    let rest = &text[start..];
    let i = rest.find(char::is_whitespace)?;
    ["in", "at", "near", "outside"]
        .iter()
        .any(|p| rest[..i].eq_ignore_ascii_case(p))
        .then(|| after_article(text, start + i))
}

fn after_article(text: &str, at: usize) -> usize {
    let start = skip_space(text, at);
    if text
        .get(start..start + 3)
        .is_some_and(|a| a.eq_ignore_ascii_case("the"))
        && text[start + 3..].starts_with(char::is_whitespace)
    {
        skip_space(text, start + 3)
    } else {
        start
    }
}

fn valid_tail(tail: &str) -> bool {
    let word = tail
        .trim_start()
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or("");
    word.is_empty()
        || [
            "in",
            "at",
            "near",
            "of",
            "from",
            "and",
            "then",
            "later",
            "there",
            "yesterday",
            "on",
            "during",
        ]
        .iter()
        .any(|w| word.eq_ignore_ascii_case(w))
}
