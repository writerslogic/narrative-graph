//! What kind of thing a mention names, judged from its own words and from what
//! the document has said about it.
//!
//! IMPORTANT: this only ever rules a mention *out*. `Unknown` is the common
//! case in narrative prose, where a bare given name carries no evidence, and a
//! rule that needs a person accepts it; a rule that needs a venue does not. The
//! asymmetry is deliberate: "Nora met Marco" must fire on two bare names, and
//! "Nora owns Marco" must not.

use super::entities::EntityCandidate;
use super::pack::{LanguagePack, Slot};
use super::register::Register;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Person,
    Place,
    Org,
    /// Capitalized but neither: a language, a faith, a form of address, or a
    /// common noun phrase introduced by a determiner.
    Thing,
    Unknown,
}

pub fn classify(
    text: &str,
    entity: &EntityCandidate,
    register: &Register,
    pack: &LanguagePack,
) -> Kind {
    let words: Vec<String> = entity
        .text
        .split(' ')
        .map(|w| w.to_ascii_lowercase())
        .collect();
    let Some(first) = words.first() else {
        return Kind::Unknown;
    };
    let last = words.last().unwrap_or(first);

    if entity.pronoun {
        return if pack.neuter_pronouns.contains(&first.as_str()) {
            Kind::Thing
        } else {
            Kind::Person
        };
    }
    if pack.honorifics.contains(&first.as_str()) && words.len() > 1 {
        return Kind::Person;
    }

    let whole = words.join(" ");
    if pack.place_names.contains(&whole.as_str()) {
        return Kind::Place;
    }
    if pack.thing_names.contains(&whole.as_str()) || pack.temporal_names.contains(&whole.as_str()) {
        return Kind::Thing;
    }
    if words.len() > 1 {
        if pack.org_heads.contains(&last.as_str()) {
            return Kind::Org;
        }
        if pack.place_heads.contains(&last.as_str()) || pack.place_leads.contains(&first.as_str()) {
            return Kind::Place;
        }
    }

    if register.is_person(&entity.normalized) {
        return Kind::Person;
    }
    if register.is_thing(&entity.normalized) {
        return Kind::Thing;
    }
    // "the Archive": a determiner in front of a capitalized run marks a common
    // noun phrase, which is a thing and never someone's name.
    let before = text[..entity.start].split_whitespace().next_back();
    if before.is_some_and(|w| pack.determiners.contains(&w.to_ascii_lowercase().as_str())) {
        return Kind::Thing;
    }
    Kind::Unknown
}

impl Slot {
    pub fn admits(self, kind: Kind) -> bool {
        match self {
            Slot::Any => true,
            Slot::Person => matches!(kind, Kind::Person | Kind::Unknown),
            Slot::Place => !matches!(kind, Kind::Person),
            Slot::Venue => matches!(kind, Kind::Place | Kind::Org),
        }
    }
}
