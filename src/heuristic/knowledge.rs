use super::cooccurrence::base;
use super::entities::EntityCandidate;
use super::kinds::classify;
use super::pack::{LanguagePack, Slot};
use super::quotes::is_quote_mark;
use super::relations::{
    conditioned_before, normalize_relation, opens_possessive, pronoun_fits_phrase,
    RelationCandidate, Scope, Stance,
};
use crate::types::{KnowledgeFact, KnowledgeKind};
use std::collections::BTreeMap;

pub(super) fn tell_verb(word: &str) -> bool {
    ["tell", "tells", "told"]
        .iter()
        .any(|verb| word.eq_ignore_ascii_case(verb))
}

pub(super) fn extract(
    text: &str,
    offset: usize,
    mentions: &[EntityCandidate],
    pack: &LanguagePack,
    scope: &Scope<'_>,
    ontology: &BTreeMap<String, String>,
    mut facts: Option<&mut Vec<KnowledgeFact>>,
) -> Vec<RelationCandidate> {
    let mut relations = Vec::new();
    for (i, actor) in mentions.iter().enumerate() {
        let mut phrase = text[actor.end..].trim_start();
        let Some((verb, rest)) = phrase.split_once(char::is_whitespace) else {
            continue;
        };
        let Some(kind) = knowledge_verb(verb) else {
            continue;
        };
        if phrase.len() == text[actor.end..].len()
            || !pronoun_fits_phrase(text, actor, pack, true)
            || opens_possessive(text, actor.end, pack)
            || !Slot::Person.admits(classify(text, actor, scope.register, pack))
            || conditioned_before(text, actor.start, pack)
            || [
                "everyone",
                "everybody",
                "somebody",
                "nobody",
                "anyone",
                "someone",
            ]
            .contains(&actor.normalized.as_str())
            || (i > 0
                && mentions[i - 1].end <= actor.start
                && text[mentions[i - 1].end..actor.start]
                    .trim()
                    .eq_ignore_ascii_case("and"))
        {
            continue;
        }
        phrase = rest.trim_start();
        let others = &mentions[i + 1..];
        let recipient = if kind == KnowledgeKind::Told {
            let at = text.len() - phrase.len();
            let Some(person) = others.iter().find(|m| {
                m.start == at
                    && m.normalized != actor.normalized
                    && !opens_possessive(text, m.end, pack)
                    && Slot::Person.admits(classify(text, m, scope.register, pack))
            }) else {
                continue;
            };
            phrase = text[person.end..].trim_start();
            Some(person)
        } else {
            None
        };
        let rest = match phrase.split_once(char::is_whitespace) {
            Some((marker, rest))
                if ["about", "that"]
                    .iter()
                    .any(|w| marker.eq_ignore_ascii_case(w)) =>
            {
                rest
            }
            _ if kind == KnowledgeKind::Known
                && others.iter().any(|m| {
                    m.start == text.len() - phrase.len() && opens_complement(&text[m.end..])
                }) =>
            {
                phrase
            }
            _ => continue,
        };
        let rest = rest.trim_start();
        let start = text.len() - rest.len();
        let content = rest
            .split(';')
            .next()
            .unwrap_or("")
            .split(is_quote_mark)
            .next()
            .unwrap_or("")
            .trim_end();
        if content.is_empty() || content.ends_with('?') {
            continue;
        }
        let content = content.trim_end_matches(['.', '!']).trim_end();
        let end = start + content.len();
        if content.is_empty() || !scope.quotes.contains_span(text, actor.start, end) {
            continue;
        }
        let topic = others.iter().find(|m| {
            m.start >= start
                && m.end <= end
                && (m.start == start || text[start..m.start].trim().eq_ignore_ascii_case("the"))
        });
        let span = [actor.start, end];
        let mut emit = |object: &EntityCandidate, label: &str, rule: &str| {
            if actor.normalized != object.normalized {
                relations.push(RelationCandidate {
                    subject: actor.normalized.clone(),
                    object: object.normalized.clone(),
                    relation: normalize_relation(label, ontology),
                    rule: rule.to_string(),
                    span,
                    base: base::PHRASE_FACTUAL,
                    gap: object.start - actor.end,
                    stance: Stance::Asserted,
                    single_filler: false,
                    role: None,
                    state: None,
                    ends_state: false,
                });
            }
        };
        if let Some(recipient) = recipient {
            emit(recipient, "speaks_to", "phrase-told-recipient");
        }
        if let Some(topic) = topic {
            let (label, rule) = match kind {
                KnowledgeKind::Learned => ("learned_about", "phrase-learned-about"),
                KnowledgeKind::Known => ("knows_about", "phrase-knows-about"),
                KnowledgeKind::Told => ("told_about", "phrase-told-topic"),
            };
            emit(topic, label, rule);
        }
        if let Some(facts) = facts.as_deref_mut() {
            facts.push(KnowledgeFact {
                kind,
                actor: actor.normalized.clone(),
                recipient: recipient.map(|m| m.normalized.clone()),
                topic: topic.map(|m| m.normalized.clone()),
                span: [offset + actor.start, offset + end],
                content: [offset + start, offset + end],
            });
        }
    }
    relations
}

pub(super) fn opens_complement(after_topic: &str) -> bool {
    after_topic.split_whitespace().next().is_some_and(|word| {
        let word = word
            .trim_matches(|c: char| !c.is_alphabetic() && c != '\'' && c != '’')
            .to_lowercase();
        let positive = word
            .strip_suffix("n't")
            .or_else(|| word.strip_suffix("n’t"))
            .unwrap_or(&word);
        [
            "is", "are", "was", "were", "had", "has", "have", "will", "wo", "would", "could",
            "shall", "should", "does", "do", "did", "might", "must", "ca", "can",
        ]
        .contains(&positive)
    })
}

fn knowledge_verb(word: &str) -> Option<KnowledgeKind> {
    let matches = |forms: &[&str]| forms.iter().any(|form| word.eq_ignore_ascii_case(form));
    if tell_verb(word) {
        Some(KnowledgeKind::Told)
    } else if matches(&["learned", "learnt", "learns"]) {
        Some(KnowledgeKind::Learned)
    } else if matches(&["knows", "knew", "know"]) {
        Some(KnowledgeKind::Known)
    } else {
        None
    }
}
