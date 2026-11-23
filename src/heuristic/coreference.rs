use super::context::Context;
use super::entities::{self, EntityCandidate, Pronoun};
use super::kinds::{self, Kind};
use super::pack::LanguagePack;
use super::quotes::QuoteMap;
use super::register::{self, PronounClass, Register};
use super::relations::{self, Scope, Stance};
use super::{dialogue, segment};
use crate::types::{CharacterProfile, FamilyLink, Gender};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) struct Sentence<'a> {
    pub text: &'a str,
    pub start: usize,
    pub quotes: QuoteMap,
    pub opened: bool,
    pub paragraph_start: bool,
    blank_line: bool,
    named: Vec<EntityCandidate>,
    definitions: Vec<[usize; 2]>,
    descriptions: Vec<EntityCandidate>,
}

pub(super) struct Document<'a> {
    pub sentences: Vec<Sentence<'a>>,
    pub register: Register,
    narrator: Option<String>,
    aliases: BTreeMap<String, BTreeSet<String>>,
    descriptions: BTreeMap<String, BTreeSet<String>>,
    roles: BTreeMap<(String, &'static str), BTreeSet<String>>,
}

impl<'a> Document<'a> {
    pub fn read(
        text: &'a str,
        aliases: &BTreeMap<String, String>,
        pack: &LanguagePack,
        context: &Context,
    ) -> Self {
        let mut document = Self {
            sentences: Vec::new(),
            register: context.register().clone(),
            narrator: context.narrator().map(str::to_string),
            aliases: BTreeMap::new(),
            descriptions: BTreeMap::new(),
            roles: BTreeMap::new(),
        };
        let mut previous_end = 0;
        let mut open = false;
        let paragraph_break = if text.contains("\n\n") { 2 } else { 1 };
        for (sentence, start) in segment::split_sentences(text, pack) {
            let breaks = text[previous_end..start]
                .bytes()
                .filter(|&c| c == b'\n')
                .count();
            let paragraph_start = breaks >= paragraph_break;
            let opened = open && !paragraph_start;
            let quotes = QuoteMap::scan(sentence, opened);
            open = quotes.open_at_end;
            previous_end = start + sentence.len();
            document.sentences.push(Sentence {
                text: sentence,
                start,
                named: entities::named_mentions(sentence, aliases, pack, &quotes),
                quotes,
                opened,
                paragraph_start,
                blank_line: breaks >= 2,
                definitions: Vec::new(),
                descriptions: Vec::new(),
            });
        }
        document.observe_names(pack);
        let redirects = name_aliases(&document.sentences, &document.register, aliases, pack);
        if !redirects.is_empty() {
            for mention in document.sentences.iter_mut().flat_map(|s| &mut s.named) {
                if let Some(id) = redirects.get(&mention.normalized) {
                    mention.normalized.clone_from(id);
                }
            }
            document.register = context.register().clone();
            document.observe_names(pack);
        }
        if document.narrator.is_none() {
            document.narrator = explicit_narrator(&document.sentences)
                .or_else(|| dialogue::infer_narrator(&document.sentences, pack));
        }
        document.learn_descriptions(pack);
        document
    }

    fn observe_names(&mut self, pack: &LanguagePack) {
        for sentence in &self.sentences {
            let named: Vec<_> = sentence.named.iter().filter(|m| !m.quoted).collect();
            let pronouns: Vec<_> = entities::extract_pronouns(sentence.text, pack)
                .into_iter()
                .filter(|p| !sentence.quotes.inside(p.start))
                .map(|p| (p.text, p.start))
                .collect();
            self.register
                .observe(sentence.text, &named, &pronouns, pack);
        }
    }

    fn learn_descriptions(&mut self, pack: &LanguagePack) {
        let mut genders = BTreeSet::new();
        let mut denied = BTreeSet::new();
        for sentence in &mut self.sentences {
            for m in &sentence.named {
                if is_possible_person(sentence.text, m, &self.register, pack) {
                    self.aliases
                        .entry(m.normalized.clone())
                        .or_default()
                        .insert(
                            sentence.text[m.start..m.end]
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" "),
                        );
                }
            }
            if sentence.named.is_empty() {
                continue;
            }
            for (start, end, description, noun) in person_descriptions(sentence.text, pack, true) {
                let name = sentence.named.iter().find(|name| {
                    is_possible_person(sentence.text, name, &self.register, pack)
                        && role_gender_agrees(&self.register, &name.normalized, noun)
                        && describes(sentence, [start, end], name, pack)
                });
                if let Some(name) = name {
                    self.descriptions
                        .entry(description)
                        .or_default()
                        .insert(name.normalized.clone());
                    sentence.definitions.push([start, end]);
                    sentence.descriptions.push(description_mention(
                        sentence,
                        start,
                        end,
                        &name.normalized,
                    ));
                    if let Some(gender) = role_gender(noun) {
                        genders.insert((name.normalized.clone(), gender));
                    }
                }
            }
            let mut names = sentence.named.clone();
            if let Some(narrator) = &self.narrator {
                names.extend(entities::first_person_mentions(
                    sentence.text,
                    pack,
                    narrator,
                    &sentence.quotes,
                ));
                names.sort_by_key(|m| m.start);
            }
            let scope = Scope {
                register: &self.register,
                quotes: &sentence.quotes,
            };
            let unmapped = BTreeMap::new();
            for relation in
                relations::extract_relations(sentence.text, &names, &unmapped, pack, &scope)
            {
                let Some(role) = relation.role else { continue };
                let [start, end] = relation.span;
                if names
                    .iter()
                    .filter(|m| m.start >= start && m.end <= end)
                    .any(|m| {
                        !is_possible_person(sentence.text, m, &self.register, pack)
                            || relations::opens_possessive(sentence.text, end, pack)
                    })
                {
                    continue;
                }
                if !role_gender_agrees(&self.register, &relation.subject, role) {
                    continue;
                }
                sentence.definitions.push(relation.span);
                let reciprocal = relation.rule.starts_with("plural-")
                    && is_family_role(role)
                    && role_gender_agrees(&self.register, &relation.object, role);
                if reciprocal {
                    let reverse = (relation.subject.clone(), role);
                    if relation.stance == Stance::Asserted {
                        self.roles
                            .entry(reverse)
                            .or_default()
                            .insert(relation.object.clone());
                    } else if relation.stance == Stance::Denied {
                        denied.insert((reverse, relation.object.clone()));
                    }
                }
                let key = (relation.object, role);
                if relation.stance == Stance::Asserted {
                    if let Some([start, end]) =
                        role_phrase(sentence.text, &names, &key.0, role, relation.span, pack)
                    {
                        sentence.descriptions.push(description_mention(
                            sentence,
                            start,
                            end,
                            &relation.subject,
                        ));
                    }
                    self.roles.entry(key).or_default().insert(relation.subject);
                } else if relation.stance == Stance::Denied {
                    denied.insert((key, relation.subject));
                }
            }
        }
        for (key, name) in &denied {
            if let Some(names) = self.roles.get_mut(key) {
                names.remove(name);
            }
        }
        for ((_, role), names) in &self.roles {
            if let Some(gender) = role_gender(role) {
                genders.extend(names.iter().cloned().map(|name| (name, gender)));
            }
        }
        for (name, gender) in genders {
            if self
                .register
                .gender(&name)
                .map_or(true, |evidence| evidence == gender)
            {
                self.register.observe_role(&name, gender);
            }
        }
        let mut inverses = Vec::new();
        for ((owner, role), names) in &self.roles {
            if let Some(inverse) = inverse_role(role, self.register.gender(owner)) {
                for name in names {
                    inverses.push(((name.clone(), inverse), owner.clone()));
                }
            }
        }
        for (key, person) in inverses {
            if !denied.contains(&(key.clone(), person.clone())) {
                self.roles.entry(key).or_default().insert(person);
            }
        }
    }

    pub fn characters(&self) -> Vec<CharacterProfile> {
        let mut profiles: BTreeMap<_, _> = self
            .aliases
            .iter()
            .map(|(id, aliases)| {
                (
                    id.as_str(),
                    CharacterProfile {
                        id: id.clone(),
                        aliases: aliases.iter().cloned().collect(),
                        gender: self.register.gender(id),
                        family: Vec::new(),
                    },
                )
            })
            .collect();
        if let Some(id) = self.narrator.as_deref() {
            profiles.entry(id).or_insert_with(|| CharacterProfile {
                id: id.to_string(),
                aliases: Vec::new(),
                gender: self.register.gender(id),
                family: Vec::new(),
            });
        }
        for (description, names) in &self.descriptions {
            if let Some(profile) =
                unique(names.iter()).and_then(|name| profiles.get_mut(name.as_str()))
            {
                profile.aliases.push(description.clone());
            }
        }
        for ((relative, role), names) in &self.roles {
            if !is_family_role(role) {
                continue;
            }
            for name in names {
                if let Some(profile) = profiles.get_mut(name.as_str()) {
                    profile.family.push(FamilyLink {
                        role: (*role).to_string(),
                        relative: relative.clone(),
                    });
                }
            }
        }
        profiles.into_values().collect()
    }

    pub fn mentions(
        &self,
        sentence: &Sentence<'_>,
        recent: &mut Recent,
        pack: &LanguagePack,
    ) -> Vec<EntityCandidate> {
        recent.start(sentence);
        let mut mentions = sentence.named.clone();
        if let Some(narrator) = &self.narrator {
            mentions.extend(
                entities::first_person_mentions(sentence.text, pack, narrator, &sentence.quotes)
                    .into_iter()
                    .filter(|m| !overlaps(&sentence.named, m.start, m.end)),
            );
        }
        self.resolve_descriptions(sentence, &mut mentions, pack);
        for pronoun in entities::extract_pronouns(sentence.text, pack) {
            if sentence.quotes.inside(pronoun.start)
                || overlaps(&mentions, pronoun.start, pronoun.end)
            {
                continue;
            }
            if let Some(name) = self.antecedent(sentence, &pronoun, &mentions, recent, pack) {
                mentions.push(pronoun.mention(name));
            }
        }
        self.resolve_roles(sentence, &mut mentions, pack);
        mentions.sort_by_key(|m| m.start);
        recent.push(entities::carried_referents(sentence.text, &mentions, pack));
        mentions
    }

    fn antecedent<'b>(
        &'b self,
        sentence: &Sentence<'_>,
        pronoun: &Pronoun,
        mentions: &'b [EntityCandidate],
        recent: &'b Recent,
        pack: &LanguagePack,
    ) -> Option<&'b str> {
        let class = register::classify(pronoun.text, pack)?;
        let mut compatible = BTreeSet::new();
        let mut agreeing = BTreeSet::new();
        let possessive = pack.possessive_pronouns.contains(&pronoun.text);
        let speaker = if matches!(pronoun.text, "him" | "her")
            && sentence.text[pronoun.end..]
                .split_whitespace()
                .next()
                .is_some_and(|word| {
                    word.eq_ignore_ascii_case("about") || word.eq_ignore_ascii_case("that")
                }) {
            mentions
                .iter()
                .find(|m| {
                    m.end <= pronoun.start
                        && !m.quoted
                        && super::knowledge::tell_verb(sentence.text[m.end..pronoun.start].trim())
                })
                .map(|m| m.normalized.as_str())
        } else {
            None
        };
        for m in mentions
            .iter()
            .filter(|m| m.end <= pronoun.start && !m.quoted)
        {
            if speaker == Some(m.normalized.as_str())
                || (m.pronoun && !possessive)
                || relations::opens_possessive(sentence.text, m.end, pack)
                || !self.register.compatible(&m.normalized, class)
                || (class != PronounClass::Neuter
                    && !is_possible_person(sentence.text, m, &self.register, pack))
            {
                continue;
            }
            compatible.insert(m.normalized.as_str());
            if self.register.allows(pronoun.text, &m.normalized, pack) {
                agreeing.insert(m.normalized.as_str());
            }
        }
        if !agreeing.is_empty() {
            return unique(agreeing.into_iter());
        }
        if compatible.len() > 1 {
            return None;
        }
        if !possessive && pack.subject_pronouns.contains(&pronoun.text) {
            if let Some(name) =
                unique(compatible.into_iter()).filter(|name| self.register.is_established(name))
            {
                return Some(name);
            }
        }
        let distance = if class == PronounClass::Neuter {
            usize::from(!sentence.blank_line)
        } else {
            RECENT_SENTENCES
        };
        for names in recent.sentences.iter().take(distance) {
            let mut matches = names.iter().map(String::as_str).filter(|name| {
                speaker != Some(*name) && self.register.allows(pronoun.text, name, pack)
            });
            if let Some(name) = matches.next() {
                return matches.next().is_none().then_some(name);
            }
        }
        None
    }

    fn resolve_descriptions(
        &self,
        sentence: &Sentence<'_>,
        mentions: &mut Vec<EntityCandidate>,
        pack: &LanguagePack,
    ) {
        for (start, end, description, _) in person_descriptions(sentence.text, pack, false) {
            if is_definition(sentence, start, end) || overlaps(mentions, start, end) {
                continue;
            }
            if let Some(name) = self
                .descriptions
                .get(&description)
                .and_then(|names| unique(names.iter()))
            {
                if !self.register.is_thing(name) {
                    mentions.push(description_mention(sentence, start, end, name));
                }
            }
        }
    }

    fn resolve_roles(
        &self,
        sentence: &Sentence<'_>,
        mentions: &mut Vec<EntityCandidate>,
        pack: &LanguagePack,
    ) {
        let mut descriptions = Vec::new();
        for owner in mentions.iter() {
            if owner.quoted {
                continue;
            }
            let mut name = &owner.normalized;
            let mut start = owner.start;
            let mut end = owner.end;
            let mut possessive = owner.pronoun
                && pack
                    .possessive_pronouns
                    .iter()
                    .any(|p| owner.text.eq_ignore_ascii_case(p));
            loop {
                let from = if possessive {
                    end
                } else if let Some(len) =
                    relations::clitic_len(&sentence.text.as_bytes()[end..], pack)
                {
                    end + len
                } else {
                    break;
                };
                let Some((role, tail)) = description_head(sentence.text, from, pack, true) else {
                    break;
                };
                let Some(target) = self
                    .roles
                    .get(&(name.clone(), role))
                    .and_then(|names| unique(names.iter()))
                else {
                    break;
                };
                if is_definition(sentence, owner.start, tail)
                    || !sentence
                        .quotes
                        .contains_span(sentence.text, owner.start, tail)
                    || mentions
                        .iter()
                        .any(|m| m.start >= owner.end && m.start < tail)
                    || role_gender(role).is_some_and(|gender| {
                        self.register.gender(target).is_some_and(|g| g != gender)
                    })
                {
                    break;
                }
                end = tail;
                name = target;
                possessive = false;
            }
            while let Some((head, role)) = genitive_head(sentence.text, start, pack) {
                if is_definition(sentence, head, end)
                    || !sentence.quotes.contains_span(sentence.text, head, end)
                    || overlaps(mentions, head, start)
                {
                    break;
                }
                let Some(target) = self
                    .roles
                    .get(&(name.clone(), role))
                    .and_then(|names| unique(names.iter()))
                else {
                    break;
                };
                if !role_gender_agrees(&self.register, target, role) {
                    break;
                }
                start = head;
                name = target;
            }
            if start < owner.start || end > owner.end {
                descriptions.push(description_mention(sentence, start, end, name));
            }
        }
        mentions.retain(|m| !overlaps(&descriptions, m.start, m.end));
        mentions.extend(descriptions);
    }
}

const RECENT_SENTENCES: usize = 3;

#[derive(Default)]
pub(super) struct Recent {
    sentences: VecDeque<Vec<String>>,
}

impl Recent {
    fn start(&mut self, sentence: &Sentence<'_>) {
        if sentence.paragraph_start {
            self.sentences.truncate(1);
        }
    }

    fn push(&mut self, names: Vec<String>) {
        self.sentences.push_front(names);
        self.sentences.truncate(RECENT_SENTENCES);
    }
}

impl Sentence<'_> {
    pub fn named(&self) -> &[EntityCandidate] {
        &self.named
    }

    pub fn descriptions(&self) -> &[EntityCandidate] {
        &self.descriptions
    }
}

fn explicit_narrator(sentences: &[Sentence<'_>]) -> Option<String> {
    let names: BTreeSet<_> = sentences
        .iter()
        .flat_map(|sentence| {
            sentence.named.iter().filter_map(|m| {
                let prefix = sentence.text[..m.start].trim();
                let tail = sentence.text[m.end..].trim();
                (!m.quoted
                    && [".", "!"].contains(&tail)
                    && ["my name is", "call me"]
                        .iter()
                        .any(|p| prefix.eq_ignore_ascii_case(p)))
                .then_some(m.normalized.as_str())
            })
        })
        .collect();
    unique(names.into_iter()).map(str::to_string)
}

fn unique<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let only = values.next()?;
    values.next().is_none().then_some(only)
}

fn overlaps(mentions: &[EntityCandidate], start: usize, end: usize) -> bool {
    mentions.iter().any(|m| start < m.end && m.start < end)
}

fn is_possible_person(
    text: &str,
    mention: &EntityCandidate,
    register: &Register,
    pack: &LanguagePack,
) -> bool {
    matches!(
        kinds::classify(text, mention, register, pack),
        Kind::Person | Kind::Unknown
    )
}

fn is_definition(sentence: &Sentence<'_>, start: usize, end: usize) -> bool {
    sentence
        .definitions
        .iter()
        .any(|&[a, b]| a <= start && end <= b)
}

fn description_mention(
    sentence: &Sentence<'_>,
    start: usize,
    end: usize,
    name: &str,
) -> EntityCandidate {
    EntityCandidate {
        start,
        end,
        normalized: name.to_string(),
        text: sentence.text[start..end].to_string(),
        quoted: sentence.quotes.inside(start),
        pronoun: false,
    }
}

fn words(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split(|c: char| !c.is_alphabetic() && c != '-')
        .filter(|word| !word.is_empty())
        .map(move |word| (word.as_ptr() as usize - text.as_ptr() as usize, word))
}

fn person_descriptions(
    text: &str,
    pack: &LanguagePack,
    include_appositions: bool,
) -> Vec<(usize, usize, String, &'static str)> {
    words(text)
        .filter_map(|(start, word)| {
            let definite = word.eq_ignore_ascii_case("the");
            let indefinite =
                include_appositions && ["a", "an"].iter().any(|a| word.eq_ignore_ascii_case(a));
            if !definite && !indefinite {
                return None;
            }
            let (noun, end) = description_head(text, start + word.len(), pack, false)?;
            let key = std::iter::once("the".to_string())
                .chain(
                    text[start + word.len()..end]
                        .split_whitespace()
                        .map(str::to_lowercase),
                )
                .collect::<Vec<_>>()
                .join(" ");
            Some((start, end, key, noun))
        })
        .collect()
}

fn description_head(
    text: &str,
    from: usize,
    pack: &LanguagePack,
    possessed: bool,
) -> Option<(&'static str, usize)> {
    let mut end = from;
    for (offset, word) in words(&text[from..]).take(4) {
        let at = from + offset;
        if at == end || !text[end..at].chars().all(char::is_whitespace) {
            return None;
        }
        end = at + word.len();
        if let Some((noun, _, _)) = pack
            .possessive_nouns
            .iter()
            .find(|(n, _, _)| word.eq_ignore_ascii_case(n))
        {
            return Some((noun, end));
        }
        if !possessed {
            const PERSON_NOUNS: &[&str] = &[
                "man",
                "woman",
                "boy",
                "girl",
                "gentleman",
                "lady",
                "child",
                "detective",
                "captain",
                "stranger",
                "sailor",
                "soldier",
                "nurse",
                "doctor",
                "teacher",
                "neighbor",
                "neighbour",
            ];
            if let Some(noun) = PERSON_NOUNS.iter().find(|n| word.eq_ignore_ascii_case(n)) {
                return Some((noun, end));
            }
        }
        if !pack
            .appositive_modifiers
            .iter()
            .any(|m| word.eq_ignore_ascii_case(m))
            || pack
                .possessive_pronouns
                .iter()
                .chain(pack.determiners)
                .any(|m| word.eq_ignore_ascii_case(m))
        {
            return None;
        }
    }
    None
}

fn describes(
    sentence: &Sentence<'_>,
    [start, end]: [usize; 2],
    name: &EntityCandidate,
    pack: &LanguagePack,
) -> bool {
    let text = sentence.text;
    let (first, last, gap, trailing) = if name.end <= start {
        (name.start, end, &text[name.end..start], &text[end..])
    } else if end <= name.start {
        (start, name.end, &text[end..name.start], &text[name.end..])
    } else {
        return false;
    };
    if !sentence.quotes.contains_span(text, first, last)
        || relations::conditioned_before(text, first, pack)
        || text
            .trim_end_matches(super::quotes::is_quote_mark)
            .ends_with('?')
    {
        return false;
    }
    let tail = trailing.trim_start();
    let closes = tail.is_empty() || tail.starts_with([',', '.', ';', '!', '—', '"', '”']);
    closes
        && (gap.trim() == ","
            || text[start..end]
                .split_whitespace()
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("the"))
                && pack
                    .possessive_copulas
                    .iter()
                    .any(|c| gap.trim().eq_ignore_ascii_case(c)))
}

fn role_phrase(
    text: &str,
    names: &[EntityCandidate],
    owner: &str,
    role: &str,
    [first, last]: [usize; 2],
    pack: &LanguagePack,
) -> Option<[usize; 2]> {
    for name in names
        .iter()
        .filter(|m| m.normalized == owner && m.start >= first && m.end <= last)
    {
        if let Some((start, title)) = genitive_head(text, name.start, pack) {
            if title == role && start >= first {
                return Some([start, name.end]);
            }
        }
        let start = if name.pronoun
            && pack
                .possessive_pronouns
                .iter()
                .any(|p| name.text.eq_ignore_ascii_case(p))
        {
            name.end
        } else if let Some(len) = relations::clitic_len(&text.as_bytes()[name.end..], pack) {
            name.end + len
        } else {
            continue;
        };
        if let Some((title, end)) = description_head(text, start, pack, true) {
            if title == role && end <= last {
                return Some([name.start, end]);
            }
        }
    }
    None
}

fn is_family_role(role: &str) -> bool {
    const ROLES: &[&str] = &[
        "mother", "father", "sister", "brother", "daughter", "son", "wife", "husband", "widow",
        "cousin", "aunt", "uncle", "niece", "nephew", "twin", "fiance", "fiancee", "parent",
        "child", "sibling", "spouse",
    ];
    ROLES.contains(&role)
        || ["step", "grand", "god", "ex-"]
            .iter()
            .any(|prefix| role.starts_with(prefix))
        || role.ends_with("-in-law")
}

fn inverse_role(role: &str, gender: Option<Gender>) -> Option<&'static str> {
    let (feminine, masculine, neutral) = match role {
        "father" | "mother" => ("daughter", "son", "child"),
        "daughter" | "son" => ("mother", "father", "parent"),
        "sister" | "brother" => ("sister", "brother", "sibling"),
        "wife" | "husband" => ("wife", "husband", "spouse"),
        "aunt" | "uncle" => ("niece", "nephew", ""),
        "niece" | "nephew" => ("aunt", "uncle", ""),
        "twin" => return Some("twin"),
        "cousin" => return Some("cousin"),
        _ => return None,
    };
    let name = match gender {
        Some(Gender::Feminine) => feminine,
        Some(Gender::Masculine) => masculine,
        None => neutral,
    };
    (!name.is_empty()).then_some(name)
}

pub(super) fn genitive_head(
    text: &str,
    start: usize,
    pack: &LanguagePack,
) -> Option<(usize, &'static str)> {
    let prefix = text[..start].trim_end();
    let (before, link) = prefix.rsplit_once(char::is_whitespace)?;
    if !link.eq_ignore_ascii_case(pack.genitive_link) {
        return None;
    }
    let (before, noun) = before.trim_end().rsplit_once(char::is_whitespace)?;
    let (role, _, _) = pack
        .possessive_nouns
        .iter()
        .find(|(role, _, _)| noun.eq_ignore_ascii_case(role))?;
    let before = before.trim_end();
    let article = before.rsplit(|c: char| !c.is_alphabetic()).next()?;
    article
        .eq_ignore_ascii_case("the")
        .then(|| (before.len() - article.len(), *role))
}

fn role_gender(role: &str) -> Option<Gender> {
    let core = role
        .trim_start_matches("step")
        .trim_start_matches("grand")
        .trim_start_matches("god")
        .trim_start_matches("ex-")
        .trim_end_matches("-in-law");
    match core {
        "woman" | "girl" | "lady" | "mother" | "sister" | "daughter" | "wife" | "widow"
        | "aunt" | "niece" | "fiancee" => Some(Gender::Feminine),
        "man" | "boy" | "gentleman" | "father" | "brother" | "son" | "husband" | "uncle"
        | "nephew" => Some(Gender::Masculine),
        _ => None,
    }
}

pub(super) fn role_gender_agrees(register: &Register, name: &str, role: &str) -> bool {
    role_gender(role)
        .zip(register.gender(name))
        .map_or(true, |(a, b)| a == b)
}

struct NameForm {
    first: String,
    last: String,
    full: bool,
    titled: bool,
    explicit: bool,
    gender: Option<Gender>,
    sentences: Vec<usize>,
    established: bool,
    compound: bool,
}

impl NameForm {
    fn could_be(&self, other: &Self) -> bool {
        self.gender.zip(other.gender).map_or(true, |(a, b)| a == b)
            && !self
                .sentences
                .iter()
                .any(|i| other.sentences.binary_search(i).is_ok())
    }
}

fn name_aliases(
    sentences: &[Sentence<'_>],
    register: &Register,
    aliases: &BTreeMap<String, String>,
    pack: &LanguagePack,
) -> BTreeMap<String, String> {
    let mut forms = BTreeMap::<&str, NameForm>::new();
    for (index, sentence) in sentences.iter().enumerate() {
        for m in &sentence.named {
            if !is_possible_person(sentence.text, m, register, pack) {
                continue;
            }
            if sentence.text[m.start..m.end]
                .bytes()
                .filter(|&b| b == b'\n')
                .nth(1)
                .is_some()
            {
                continue;
            }
            let explicit = aliases
                .get(&m.text)
                .or_else(|| aliases.get(&m.text.to_lowercase()))
                == Some(&m.normalized);
            let lowered = m.text.to_lowercase();
            let mut words = lowered.split_whitespace().peekable();
            let mut titled = false;
            while words.peek().is_some_and(|w| {
                pack.honorifics.contains(w) || pack.honorific_qualifiers.contains(w)
            }) {
                titled = true;
                words.next();
            }
            let Some(first) = words.next() else { continue };
            if pack.sentence_openers.contains(&first)
                || pack.appositive_modifiers.contains(&first)
                || matches!(
                    first,
                    "dearest" | "mild" | "chapter" | "volume" | "enter" | "exit"
                )
                || first.contains('-') && !titled
            {
                continue;
            }
            let rest: Vec<_> = words.collect();
            if rest.iter().any(|w| pack.honorifics.contains(w)) {
                continue;
            }
            let last = rest.last().copied();
            let form = forms.entry(&m.normalized).or_insert_with(|| NameForm {
                first: first.to_string(),
                last: last.unwrap_or(first).to_string(),
                full: last.is_some(),
                titled: false,
                explicit: false,
                gender: register.gender(&m.normalized),
                sentences: Vec::new(),
                established: register.is_established(&m.normalized),
                compound: rest
                    .iter()
                    .filter(|word| !pack.name_particles.contains(word))
                    .count()
                    > 1,
            });
            form.titled |= titled;
            form.explicit |= explicit;
            if form.sentences.last() != Some(&index) {
                form.sentences.push(index);
            }
        }
    }
    let mut full_by_last: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (&name, form) in forms
        .iter()
        .filter(|(_, form)| form.full && (!form.compound || form.titled))
    {
        full_by_last.entry(&form.last).or_default().push(name);
    }
    let mut titled_by_last = BTreeMap::new();
    for form in forms.values().filter(|form| form.titled && !form.full) {
        *titled_by_last
            .entry((form.last.as_str(), form.gender))
            .or_insert(0usize) += 1;
    }
    let mut redirects = BTreeMap::new();
    for (&name, form) in forms
        .iter()
        .filter(|(_, form)| form.titled && !form.full && !form.explicit)
    {
        if form.gender.is_none() || titled_by_last[&(form.last.as_str(), form.gender)] != 1 {
            continue;
        }
        let Some(candidates) = full_by_last.get(form.last.as_str()) else {
            continue;
        };
        if let Some(target) = unique(
            candidates
                .iter()
                .copied()
                .filter(|id| form.gender == forms[id].gender && form.could_be(&forms[id])),
        ) {
            redirects.insert(name.to_string(), target.to_string());
        }
    }
    let titled_full_names: BTreeSet<_> = redirects.values().map(String::as_str).collect();
    let mut by_token: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (&name, form) in forms.iter().filter(|(name, form)| {
        form.titled
            || titled_full_names.contains(*name)
            || (form.full && form.established && form.sentences.len() > 1)
    }) {
        let target = redirects.get(name).map_or(name, String::as_str);
        by_token.entry(&form.first).or_default().insert(target);
        by_token.entry(&form.last).or_default().insert(target);
    }
    let short = forms
        .iter()
        .filter(|(_, form)| !form.full && !form.titled && !form.explicit)
        .filter_map(|(&name, form)| {
            let target = unique(by_token.get(form.first.as_str())?.iter().copied())?;
            (name != target && form.could_be(&forms[target]))
                .then(|| (name.to_string(), target.to_string()))
        })
        .collect::<Vec<_>>();
    redirects.extend(short);
    redirects
}
