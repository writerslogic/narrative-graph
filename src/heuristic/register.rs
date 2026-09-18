//! What a document says about its own names, gathered before anything is
//! resolved.
//!
//! Cross-sentence pronoun resolution needs to know things a single sentence
//! cannot tell it: whether a capitalized word is a name at all, whether the
//! name is a person, and whether that person is male or female. None of it is
//! guessed here. Every entry is evidence the text supplied outright — a title
//! attached to the name, a determiner in front of it, or the name appearing
//! where a sentence did not force its capital.
//!
//! IMPORTANT: this is a lexical record, not a resolution. It says "the text
//! calls this one Mrs.", never "this pronoun means her".

use super::entities::EntityCandidate;
use super::pack::LanguagePack;
use std::collections::BTreeMap;

/// What the pipeline can tell about a pronoun from its surface form alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PronounClass {
    Masculine,
    Feminine,
    Neuter,
    Plural,
}

/// What a name's own evidence says. Absent evidence is `false` everywhere,
/// which is why every rule below reads as "known to conflict" rather than
/// "not known to agree".
#[derive(Debug, Default, Clone)]
struct Entry {
    /// Seen capitalized where a sentence boundary did not force it, or seen
    /// carrying a title. Without this, every sentence-opening word the opener
    /// list does not cover — "Behind", "Penned", "Imagine" — is a referent a
    /// pronoun can be linked to.
    established: bool,
    masculine: u32,
    feminine: u32,
    /// Evidence that the name is a thing: a determiner in front of it, a name
    /// of a time, use only ever as a location, or a neuter pronoun taking it.
    neuter: u32,
    locative: bool,
    elsewhere: bool,
}

/// A title is unambiguous and a bootstrapped pronoun sighting is a good guess,
/// so they must not weigh the same. One title outweighs three sightings.
const TITLE: u32 = 4;
const SIGHTING: u32 = 1;

impl Entry {
    /// IMPORTANT: evidence is counted, never latched. A single sighting decides
    /// nothing, because the sighting rule that feeds this is a heuristic: one
    /// sentence reading "Newland Archer looked at her" would otherwise record
    /// him as female for the whole document. A reading wins only by
    /// outweighing its opposite two to one, and a tie settles nothing — which
    /// costs recall and never accuracy, since an unsettled name is one no
    /// pronoun may take.
    fn wins(for_it: u32, against: u32) -> bool {
        for_it > 0 && for_it >= against.saturating_mul(2)
    }

    fn personal(&self) -> u32 {
        self.masculine + self.feminine
    }

    fn thing(&self) -> u32 {
        // A name the text only ever uses after "at" or "in" is a place, and
        // the whole document agreeing on that is strong evidence.
        self.neuter
            + if self.locative && !self.elsewhere {
                TITLE
            } else {
                0
            }
    }

    fn allows(&self, class: PronounClass) -> bool {
        match class {
            PronounClass::Masculine => {
                Self::wins(self.personal(), self.thing())
                    && Self::wins(self.masculine, self.feminine)
            }
            PronounClass::Feminine => {
                Self::wins(self.personal(), self.thing())
                    && Self::wins(self.feminine, self.masculine)
            }
            // IMPORTANT: "it" needs positive evidence of a thing, not merely
            // the absence of evidence of a person. Most of the "it"s in
            // narrative prose stand for a clause or an unnamed object, and a
            // referent the text never marked as a thing is the wrong home for
            // all of them.
            PronounClass::Neuter => Self::wins(self.thing(), self.personal()),
            PronounClass::Plural => false,
        }
    }
}

/// Every name the document uses, with what it says about each.
#[derive(Debug, Default)]
pub struct Register {
    entries: BTreeMap<String, Entry>,
}

impl Register {
    /// Record what one sentence says about the names in it.
    pub fn observe(
        &mut self,
        sentence: &str,
        mentions: &[EntityCandidate],
        pronouns: &[(String, usize)],
        pack: &LanguagePack,
    ) {
        for mention in mentions {
            let entry = self.entries.entry(mention.normalized.clone()).or_default();
            let titled = title_gender(&mention.text, pack);

            if let Some(masculine) = titled {
                entry.established = true;
                if masculine {
                    entry.masculine += TITLE;
                } else {
                    entry.feminine += TITLE;
                }
            }
            if !opens_sentence(sentence, mention.start) {
                entry.established = true;
            }
            if preceded_by(sentence, mention.start, pack.determiners) {
                entry.neuter += SIGHTING;
            }
            if pack
                .temporal_names
                .contains(&mention.normalized.replace('_', " ").as_str())
            {
                entry.neuter += TITLE;
                entry.established = true;
            }
            if preceded_by(sentence, mention.start, pack.locative_prepositions) {
                entry.locative = true;
            } else {
                entry.elsewhere = true;
            }
        }

        self.learn_from_unambiguous_pronouns(sentence, mentions, pronouns, pack);
    }

    /// Take what a pronoun says about the one name in front of it.
    ///
    /// A bare given name carries no title, so nothing above can tell whether
    /// "Jane" is a woman or "Flaubert" is a man — and in narrative prose most
    /// names are bare most of the time. The text answers it anyway, one
    /// sentence at a time: "Elena said she would go" is the document stating
    /// Elena's gender as plainly as "Mrs." would.
    ///
    /// IMPORTANT: only where exactly one name precedes the pronoun in the
    /// sentence. With two, "Mr. Darcy told Elizabeth that he would call" would
    /// record Elizabeth as masculine — the nearest-mention rule is a good guess
    /// for a resolution and a bad one for a fact, and a poisoned register is
    /// worse than an empty one. Evidence that lands both ways settles nothing
    /// and blocks nothing, so a genuine ambiguity costs recall, never accuracy.
    fn learn_from_unambiguous_pronouns(
        &mut self,
        sentence: &str,
        mentions: &[EntityCandidate],
        pronouns: &[(String, usize)],
        pack: &LanguagePack,
    ) {
        for (pronoun, start) in pronouns {
            if !pack
                .subject_pronouns
                .iter()
                .any(|p| pronoun.eq_ignore_ascii_case(p))
            {
                continue;
            }
            let mut before = mentions.iter().filter(|m| m.end <= *start);
            let (Some(only), None) = (before.next(), before.next()) else {
                continue;
            };
            // "In London she saw them" opens on an adjunct, not a subject, so
            // the name in front of the pronoun is a place the sentence happens
            // in rather than the one it is about.
            if preceded_by(sentence, only.start, pack.locative_prepositions) {
                continue;
            }
            let Some(class) = classify(pronoun, pack) else {
                continue;
            };

            let entry = self.entries.entry(only.normalized.clone()).or_default();
            match class {
                PronounClass::Masculine => entry.masculine += SIGHTING,
                PronounClass::Feminine => entry.feminine += SIGHTING,
                PronounClass::Neuter => entry.neuter += SIGHTING,
                PronounClass::Plural => {}
            }
        }
    }

    /// Whether `referent` may be what `pronoun` refers to.
    ///
    /// IMPORTANT: this is the whole of the agreement check, and it is one-sided
    /// on purpose. A referent is allowed unless the document says otherwise,
    /// except for `established`, which must be positively true — an unestablished
    /// name is not a name, and admitting one costs precision with no recall to
    /// show for it.
    ///
    /// A plural pronoun is never resolved across a sentence boundary. "They"
    /// in narrative prose names a group assembled over several sentences, and
    /// a carry that holds one referent at a time cannot represent one; every
    /// link it could make would name a single person the text did not mean.
    pub fn allows(&self, pronoun: &str, referent: &str, pack: &LanguagePack) -> bool {
        let Some(class) = classify(pronoun, pack) else {
            return false;
        };
        let Some(entry) = self.entries.get(referent) else {
            return false;
        };

        entry.established && entry.allows(class)
    }
}

fn classify(pronoun: &str, pack: &LanguagePack) -> Option<PronounClass> {
    let has = |list: &[&str]| list.iter().any(|p| pronoun.eq_ignore_ascii_case(p));

    if has(pack.masculine_pronouns) {
        Some(PronounClass::Masculine)
    } else if has(pack.feminine_pronouns) {
        Some(PronounClass::Feminine)
    } else if has(pack.neuter_pronouns) {
        Some(PronounClass::Neuter)
    } else if has(pack.plural_pronouns) {
        Some(PronounClass::Plural)
    } else {
        None
    }
}

/// `Some(true)` for a masculine title leading the mention, `Some(false)` for a
/// feminine one, `None` where the leading titles say nothing about gender
/// ("Dr.", "Captain", "Professor") or where there are none.
fn title_gender(surface: &str, pack: &LanguagePack) -> Option<bool> {
    for word in surface.split(' ') {
        let word = word.to_lowercase();
        if pack.masculine_titles.contains(&word.as_str()) {
            return Some(true);
        }
        if pack.feminine_titles.contains(&word.as_str()) {
            return Some(false);
        }
        if !pack.honorifics.contains(&word.as_str())
            && !pack.honorific_qualifiers.contains(&word.as_str())
        {
            // Past the titles and into the name itself.
            return None;
        }
    }
    None
}

/// Whether nothing but opening punctuation stands before the mention, so its
/// capital is explained by the sentence starting rather than by the word.
fn opens_sentence(sentence: &str, start: usize) -> bool {
    sentence[..start]
        .chars()
        .all(|c| c.is_whitespace() || "\"'\u{201C}\u{2018}([".contains(c))
}

/// Whether the word immediately before the mention is one of `words`.
fn preceded_by(sentence: &str, start: usize, words: &[&str]) -> bool {
    sentence[..start]
        .split_whitespace()
        .next_back()
        .is_some_and(|word| {
            let word = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            words.contains(&word.as_str())
        })
}
