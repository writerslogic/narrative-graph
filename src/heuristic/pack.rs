//! The closed vocabularies one language needs, gathered into one named unit.
//!
//! Every list here was a `const` sitting beside the code that read it, in four
//! different files. Gathering them makes the English assumption explicit rather
//! than implicit: what a second language must supply is exactly this struct,
//! and nothing outside it.
//!
//! IMPORTANT: a pack is vocabulary, not strategy. Three assumptions live in the
//! code that reads a pack and cannot be moved into one:
//!
//! - Capitalization is the entity signal (`extract_capitalized_entities`). A
//!   language that capitalizes every noun, or none, needs a different detector,
//!   not a different word list.
//! - A possessive is a clitic on the possessor, read left to right.
//! - Sentence-final `.`/`!`/`?` terminate, and a lowercase continuation does not.
//!
//! A second pack is therefore only enough for a language that shares those
//! three. Deciding otherwise is what the second pack is for; it is not decided
//! here.

use super::cooccurrence::base;

mod names;
mod phrases;

/// A verb-phrase rule: a marker that must appear between the two mentions,
/// optionally a second one, and the relation that follows from both.
pub struct VerbRule {
    /// The token that licenses the relation. Leading space where the rule
    /// wants a word boundary in front of it.
    pub marker: &'static str,
    /// A second token the same window must also carry, as "work" needs "at".
    pub also: Option<&'static str>,
    /// A token that reverses subject and object, as passive voice does.
    pub passive_marker: Option<&'static str>,
    pub relation: &'static str,
    pub rule: &'static str,
    pub base: f32,
}

/// What a mention must be for a phrase rule to apply to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Anything not positively known to be a place, organization or thing.
    Person,
    /// Anything not positively known to be a person.
    Place,
    /// Positively a place or organization by its own words.
    Venue,
    /// No constraint.
    Any,
}

/// A phrase rule: the whole text between two mentions, once fillers and
/// trailing determiners are peeled off, equals one of `phrases`. See
/// `pack/phrases.rs`.
pub struct PhraseRule {
    pub phrases: &'static [&'static str],
    pub relation: &'static str,
    pub rule: &'static str,
    pub base: f32,
    /// What the earlier mention must be, and the later one. Read in text order,
    /// independent of `passive`.
    pub subject: Slot,
    pub object: Slot,
    /// The relation's subject is the later mention.
    pub passive: bool,
}

/// Everything the pipeline reads as vocabulary for one language.
pub struct LanguagePack {
    pub sentence_openers: &'static [&'static str],
    pub name_particles: &'static [&'static str],
    pub honorifics: &'static [&'static str],
    pub honorific_qualifiers: &'static [&'static str],
    pub pronouns: &'static [&'static str],
    /// The pronoun list cut by what it agrees with. IMPORTANT: every entry
    /// must also appear in `pronouns`, which is what decides whether a word is
    /// a pronoun at all; these only say what kind.
    pub masculine_pronouns: &'static [&'static str],
    pub feminine_pronouns: &'static [&'static str],
    pub neuter_pronouns: &'static [&'static str],
    pub plural_pronouns: &'static [&'static str],
    /// Titles that settle the bearer's gender. A subset of `honorifics`:
    /// "dr", "captain" and "professor" are titles and settle nothing.
    pub masculine_titles: &'static [&'static str],
    pub feminine_titles: &'static [&'static str],
    /// Words whose presence in front of a capitalized run marks it as a common
    /// noun phrase rather than a personal name.
    pub determiners: &'static [&'static str],
    /// Pronouns that name the speaker or the listener rather than anyone in
    /// the story. IMPORTANT: these are never mentions. "I" is capitalized in
    /// every sentence of first-person narration, and a graph node called `i`
    /// names whoever happens to be talking.
    pub deictic_pronouns: &'static [&'static str],
    /// Verbs of a dialogue tag ("said Diana", "Heathcliff asked"): the name
    /// next to one, outside quotation marks, is the speaker of the paragraph.
    pub speech_verbs: &'static [&'static str],
    /// The pronouns that stand in subject position. IMPORTANT: only these are
    /// read as evidence about the name in front of them. An object or
    /// possessive pronoun after a name usually names someone else — "Archer
    /// looked at her" is about two people — so learning from one records the
    /// opposite of what the sentence says.
    pub subject_pronouns: &'static [&'static str],
    /// Prepositions that take a place and essentially never take a person.
    /// "to" and "from" are absent: both take people constantly.
    pub locative_prepositions: &'static [&'static str],
    /// Names of times rather than of people.
    pub temporal_names: &'static [&'static str],
    pub abbreviations: &'static [&'static str],
    pub possessive_nouns: &'static [(&'static str, &'static str, f32)],
    pub possessive_copulas: &'static [&'static str],
    pub appositive_modifiers: &'static [&'static str],
    pub suspending_words: &'static [&'static str],
    pub negators: &'static [&'static str],
    /// Relations whose object admits exactly one subject, so two different
    /// subjects asserted over one object contradict each other.
    pub single_filler_relations: &'static [&'static str],
    /// A negator that attaches as a suffix rather than standing as a word.
    pub negation_clitic: &'static str,
    /// The word whose infinitive complement suspends the event it names.
    pub infinitive_marker: &'static str,
    /// The clitic marking the mention before it as a possessor.
    pub possessive_clitic: &'static str,
    /// The link word in an "of" genitive: "the sister of Marco".
    pub genitive_link: &'static str,
    /// The conjunction that closes a possessed noun phrase.
    pub phrase_conjunction: &'static str,
    /// The opener of a relative clause about the preceding mention.
    pub relative_opener: &'static str,
    /// Rules read directly from the text between two mentions.
    pub verb_rules: &'static [VerbRule],
    /// Rules read from the same text once `relative_opener` is present.
    pub relative_rules: &'static [VerbRule],
    /// Exact-phrase rules between two mentions.
    pub phrase_rules: &'static [PhraseRule],
    /// Words peeled from the front of the text between mentions before a
    /// phrase rule is compared.
    pub phrase_fillers: &'static [&'static str],
    /// Words peeled from the end of it.
    pub phrase_trailers: &'static [&'static str],
    /// Last words of a multi-word mention that name a place.
    pub place_heads: &'static [&'static str],
    /// Last words of a multi-word mention that name an organization.
    pub org_heads: &'static [&'static str],
    /// First words of a multi-word mention that name a place.
    pub place_leads: &'static [&'static str],
    /// Whole names that are places.
    pub place_names: &'static [&'static str],
    /// Whole names that are capitalized things: languages, faiths, address terms.
    pub thing_names: &'static [&'static str],
    /// Possessive determiners that stand for a possessor named elsewhere.
    pub possessive_pronouns: &'static [&'static str],
    /// First-person pronouns, resolved only when the caller names the narrator.
    pub first_person_pronouns: &'static [&'static str],
    /// Plural relational nouns for "x and y are <plural>", as (noun, relation, base).
    pub plural_nouns: &'static [(&'static str, &'static str, f32)],
    /// The copulas that introduce a plural noun or adjective.
    pub plural_copulas: &'static [&'static str],
    /// Words allowed between the copula and a plural noun: "are old friends".
    pub plural_modifiers: &'static [&'static str],
    /// Closed-class words that may follow an object pronoun, closing the phrase
    /// it ends: "met her at the station".
    pub object_followers: &'static [&'static str],
    /// Words that may follow a possessed relational noun without making it a
    /// modifier of a later noun: "Marco's student for two terms".
    pub possessive_tails: &'static [&'static str],
}

/// The pack the pipeline runs with. Selecting a different one is a caller-facing
/// decision that waits on a second pack existing; nothing here presumes English
/// is the only possible value.
pub static ENGLISH: LanguagePack = LanguagePack {
    sentence_openers: SENTENCE_OPENERS,
    name_particles: NAME_PARTICLES,
    honorifics: HONORIFICS,
    honorific_qualifiers: HONORIFIC_QUALIFIERS,
    pronouns: PRONOUNS,
    masculine_pronouns: &["he", "him", "his"],
    feminine_pronouns: &["she", "her"],
    neuter_pronouns: &["it", "its"],
    plural_pronouns: &["they", "them", "their"],
    masculine_titles: &["mr", "mister", "sir", "lord"],
    feminine_titles: &["mrs", "ms", "miss", "dame", "lady"],
    determiners: &["the", "a", "an"],
    deictic_pronouns: &[
        "i",
        "me",
        "my",
        "mine",
        "myself",
        "you",
        "your",
        "yours",
        "yourself",
        "we",
        "us",
        "our",
        "ours",
        "ourselves",
    ],
    speech_verbs: &[
        "said",
        "says",
        "asked",
        "asks",
        "cried",
        "cries",
        "replied",
        "replies",
        "answered",
        "answers",
        "exclaimed",
        "exclaims",
        "shouted",
        "shouts",
        "called",
        "calls",
        "whispered",
        "whispers",
        "continued",
        "continues",
        "interrupted",
        "interrupts",
        "inquired",
        "inquires",
        "demanded",
        "demands",
        "returned",
        "rejoined",
        "added",
        "adds",
        "repeated",
        "pronounced",
        "hallooed",
        "screamed",
        "muttered",
        "murmured",
        "observed",
        "remarked",
        "retorted",
        "sobbed",
        "snapped",
        "growled",
        "laughed",
        "began",
        "pleaded",
        "urged",
        "insisted",
        "declared",
        "resumed",
    ],
    subject_pronouns: &["he", "she", "it", "they"],
    locative_prepositions: &[
        "at", "in", "near", "into", "within", "upon", "beyond", "outside", "inside", "across",
    ],
    temporal_names: &[
        "january",
        "february",
        "march",
        "april",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
        "christmas",
        "easter",
        "michaelmas",
        "lady day",
        "midsummer",
    ],
    abbreviations: ABBREVIATIONS,
    possessive_nouns: POSSESSIVE_NOUNS,
    possessive_copulas: POSSESSIVE_COPULAS,
    appositive_modifiers: APPOSITIVE_MODIFIERS,
    suspending_words: SUSPENDING_WORDS,
    negators: NEGATORS,
    single_filler_relations: SINGLE_FILLER_RELATIONS,
    negation_clitic: "n't",
    infinitive_marker: "to",
    possessive_clitic: "'s",
    genitive_link: "of",
    phrase_conjunction: " and ",
    relative_opener: ", who ",
    verb_rules: VERB_RULES,
    relative_rules: RELATIVE_RULES,
    phrase_rules: phrases::PHRASE_RULES,
    phrase_fillers: phrases::PHRASE_FILLERS,
    phrase_trailers: phrases::PHRASE_TRAILERS,
    place_heads: names::PLACE_HEADS,
    org_heads: names::ORG_HEADS,
    place_leads: names::PLACE_LEADS,
    place_names: names::PLACE_NAMES,
    thing_names: names::THING_NAMES,
    possessive_pronouns: &["his", "her", "my", "our"],
    first_person_pronouns: &["i", "me", "my", "mine", "myself", "our"],
    plural_nouns: PLURAL_NOUNS,
    plural_copulas: &["are", "were"],
    plural_modifiers: &[
        "close", "longtime", "lifelong", "very", "great", "real", "true",
    ],
    object_followers: &[
        "at",
        "in",
        "on",
        "to",
        "with",
        "for",
        "from",
        "and",
        "but",
        "as",
        "when",
        "after",
        "before",
        "while",
        "again",
        "once",
        "too",
        "there",
        "here",
        "today",
        "later",
        "that",
        "because",
        "until",
        "by",
        "outside",
        "inside",
        "yesterday",
        "tonight",
    ],
    possessive_tails: &[
        "for", "since", "during", "until", "when", "while", "before", "after", "at", "in", "from",
        "once", "then", "now", "there", "here", "too", "again", "as", "by", "that", "who",
    ],
};

/// Words that are capitalized at the start of a sentence by position alone.
///
/// IMPORTANT: a capitalized run becomes one mention, so "But Elena" normalizes
/// to `but_elena` and never unifies with `elena` elsewhere in the text. That
/// silently splits a character into two graph nodes, and narrative prose opens
/// sentences this way constantly.
///
/// Membership is restricted to words that cannot also be a given name in that
/// position: "May", "Will", "Grace", "June", "Faith" and "Summer" are all
/// names and are deliberately absent, because dropping a real mention costs
/// more than keeping a malformed one.
///
/// The pronouns `extract_pronouns` owns are listed here too. A capitalized
/// pronoun otherwise becomes a second mention at the same offsets as the
/// resolved one, and `she` as a graph node names nobody.
#[rustfmt::skip]
const SENTENCE_OPENERS: &[&str] = &[
    "a", "after", "again", "all", "although", "an", "and", "another", "any", "are", "as", "at",
    "because", "before", "both", "but", "by", "did", "do", "does", "each", "either", "even",
    "every", "for", "from", "had", "has", "have", "he", "her", "here", "him", "his", "how",
    "however", "if",
    "in", "indeed", "instead", "well", "oh", "ah", "yes", "aye", "nay", "hush", "thanks", "hello", "alas", "pray", "dear", "is", "its", "just", "later", "maybe", "meanwhile", "my", "neither",
    "never", "no", "nor", "not", "now", "of", "often", "on", "once", "only", "or", "our",
    "perhaps", "since", "so", "some", "sometimes", "soon", "still", "suddenly", "that", "the",
    "she", "their", "them", "then", "there", "these", "they", "this", "those", "though", "to",
    "today", "tomorrow",
    "tonight", "was", "were", "what", "when", "where", "which", "while", "who", "whom", "whose",
    "why", "with", "yesterday", "yet", "your",
];

/// Lowercase particles that belong to the name they sit inside.
///
/// IMPORTANT: a particle only continues a run already open. "Bourgh" is what
/// "Lady Catherine de Bourgh" normalized to before this list existed, which
/// names a surname shared with "Sir Lewis de Bourgh" rather than either person.
/// A particle may not open a mention, because the same words open ordinary
/// clauses ("van" rarely, "bin" and "af" never, but "du" and "della" occur in
/// quoted French and Italian).
///
/// English "of" is deliberately absent. It is the only candidate that also
/// appears in `SENTENCE_OPENERS` and as the link text `of_genitive_noun`
/// matches, so admitting it would fold "the Duchess of Devonshire" into one
/// mention and change the shape of mentions already measured in
/// `docs/EVALUATION.md`, while doing nothing for a title whose noun is
/// lowercase and therefore closes the run before "of" is reached.
const NAME_PARTICLES: &[&str] = &[
    "de", "del", "della", "der", "des", "di", "du", "la", "le", "van", "von", "af", "ter", "bin",
    "ibn", "al", "da", "dos", "das", "y",
];

/// Titles and ranks that precede a name and are not part of the identity.
///
/// IMPORTANT: this cuts differently from both lists above. A particle joins a
/// run, an opener is dropped at position 0 only, and a title is capitalized,
/// real, and correctly part of the surface form; it just must not be part of
/// the identity, or "the Right Honourable Lady Catherine de Bourgh" can never
/// unify with any shorter mention of her.
///
/// Entries are bare: `.` is a separator in `extract_capitalized_entities`, so
/// the run already reads "Mrs Bennet" and a "mrs." entry would never match.
///
/// Membership is restricted to words that do not also do relational or naming
/// work. "Father", "Mother", "Sister" and "Master" are titles in address but
/// heads of the relational lexicon `find_relation_pattern` matches; "Major",
/// "General" and "President" are common nouns. "Grace" is excluded for the
/// reason "May" and "June" are excluded above: it is a given name, and "His
/// Grace" needs no entry, since `strip_honorifics` keeps a title standing in
/// front of a single name word.
#[rustfmt::skip]
const HONORIFICS: &[&str] = &[
    "mr", "mrs", "ms", "miss", "mister", "dr", "doctor", "sir", "dame", "lady", "lord",
    "captain", "capt", "colonel", "col", "reverend", "rev", "professor", "prof",
    "sergeant", "sgt", "lieutenant", "admiral", "bishop", "cardinal",
    "judge", "dean", "officer", "detective", "inspector", "senator", "governor", "mayor",
    "coach", "pastor", "principal", "nurse", "deputy",
    "hon", "honourable", "honorable",
];

/// Words that form a compounded style only in front of another title. Alone
/// they are ordinary adjectives, so they are stripped only when an entry of
/// `HONORIFICS` follows.
const HONORIFIC_QUALIFIERS: &[&str] = &["right", "most", "very"];

/// Pronouns that can stand for a mention made earlier in the same sentence.
const PRONOUNS: &[&str] = &[
    "he", "she", "they", "him", "her", "them", "his", "their", "it",
];

/// Words that end in a period without ending a sentence.
const ABBREVIATIONS: &[&str] = &[
    // Personal and professional titles
    "mr", "mrs", "ms", "dr", "prof", "rev", "fr", "sr", "jr", "st", "hon", "msgr",
    // Military and civil ranks
    "lt", "capt", "col", "gen", "sgt", "maj", "adm", "cmdr", "gov", "sen", "rep", "det", "insp",
    "supt", // Common Latin and editorial abbreviations
    "etc", "vs", "viz", "cf", "al", "ibid", "approx", "est", "min", "max",
    // Organizational and address abbreviations
    "inc", "ltd", "co", "corp", "dept", "univ", "mt", "ft", "ave", "blvd", "no", "vol", "ed", "pp",
    "fig",
];

/// Relational nouns recognized in a possessive, as (noun, relation, base
/// confidence). Every entry reads "X is Y's <noun>", giving relation(X, Y).
///
/// The relation is named explicitly rather than derived from the noun so a
/// noun can share a label with a verb rule: "Elena is Marco's mentor" and
/// "Elena mentors Marco" are the same fact and must not produce two edge
/// types. The rule name stays `possessive-<noun>-pattern`, so attribution
/// remains per-noun. Matching is whole-word, so entries are order-independent.
const POSSESSIVE_NOUNS: &[(&str, &str, f32)] = &[
    // Hyphenated nouns first: a stem ("wife") also matches inside them, and the
    // first noun that heads the phrase wins.
    (
        "mother-in-law",
        "mother_in_law_of",
        base::POSSESSIVE_FACTUAL,
    ),
    (
        "father-in-law",
        "father_in_law_of",
        base::POSSESSIVE_FACTUAL,
    ),
    (
        "sister-in-law",
        "sister_in_law_of",
        base::POSSESSIVE_FACTUAL,
    ),
    (
        "brother-in-law",
        "brother_in_law_of",
        base::POSSESSIVE_FACTUAL,
    ),
    ("ex-wife", "ex_wife_of", base::POSSESSIVE_FACTUAL),
    ("ex-husband", "ex_husband_of", base::POSSESSIVE_FACTUAL),
    // Kinship and role: the possessive states the relation outright.
    ("sister", "sister_of", base::POSSESSIVE_FACTUAL),
    ("brother", "brother_of", base::POSSESSIVE_FACTUAL),
    ("mother", "mother_of", base::POSSESSIVE_FACTUAL),
    ("father", "father_of", base::POSSESSIVE_FACTUAL),
    ("grandmother", "grandmother_of", base::POSSESSIVE_FACTUAL),
    ("grandfather", "grandfather_of", base::POSSESSIVE_FACTUAL),
    ("daughter", "daughter_of", base::POSSESSIVE_FACTUAL),
    ("son", "son_of", base::POSSESSIVE_FACTUAL),
    ("wife", "wife_of", base::POSSESSIVE_FACTUAL),
    ("husband", "husband_of", base::POSSESSIVE_FACTUAL),
    ("cousin", "cousin_of", base::POSSESSIVE_FACTUAL),
    ("aunt", "aunt_of", base::POSSESSIVE_FACTUAL),
    ("uncle", "uncle_of", base::POSSESSIVE_FACTUAL),
    ("niece", "niece_of", base::POSSESSIVE_FACTUAL),
    ("nephew", "nephew_of", base::POSSESSIVE_FACTUAL),
    ("widow", "widow_of", base::POSSESSIVE_FACTUAL),
    ("stepmother", "stepmother_of", base::POSSESSIVE_FACTUAL),
    ("stepfather", "stepfather_of", base::POSSESSIVE_FACTUAL),
    ("stepdaughter", "stepdaughter_of", base::POSSESSIVE_FACTUAL),
    ("stepson", "stepson_of", base::POSSESSIVE_FACTUAL),
    ("stepsister", "stepsister_of", base::POSSESSIVE_FACTUAL),
    ("stepbrother", "stepbrother_of", base::POSSESSIVE_FACTUAL),
    (
        "granddaughter",
        "granddaughter_of",
        base::POSSESSIVE_FACTUAL,
    ),
    ("grandson", "grandson_of", base::POSSESSIVE_FACTUAL),
    ("godmother", "godmother_of", base::POSSESSIVE_FACTUAL),
    ("godfather", "godfather_of", base::POSSESSIVE_FACTUAL),
    ("godson", "godson_of", base::POSSESSIVE_FACTUAL),
    ("goddaughter", "goddaughter_of", base::POSSESSIVE_FACTUAL),
    ("twin", "twin_of", base::POSSESSIVE_FACTUAL),
    ("fiance", "fiance_of", base::POSSESSIVE_FACTUAL),
    ("fiancee", "fiance_of", base::POSSESSIVE_FACTUAL),
    ("girlfriend", "girlfriend_of", base::POSSESSIVE_FACTUAL),
    ("boyfriend", "boyfriend_of", base::POSSESSIVE_FACTUAL),
    ("partner", "partner_of", base::POSSESSIVE_STANCE),
    ("boss", "boss_of", base::POSSESSIVE_FACTUAL),
    ("colleague", "colleague_of", base::POSSESSIVE_STANCE),
    ("coworker", "coworker_of", base::POSSESSIVE_STANCE),
    ("roommate", "roommate_of", base::POSSESSIVE_STANCE),
    ("classmate", "classmate_of", base::POSSESSIVE_STANCE),
    ("neighbor", "neighbor_of", base::POSSESSIVE_STANCE),
    ("neighbour", "neighbor_of", base::POSSESSIVE_STANCE),
    ("landlord", "landlord_of", base::POSSESSIVE_FACTUAL),
    ("tenant", "tenant_of", base::POSSESSIVE_FACTUAL),
    ("patient", "patient_of", base::POSSESSIVE_FACTUAL),
    ("doctor", "doctor_of", base::POSSESSIVE_FACTUAL),
    ("therapist", "therapist_of", base::POSSESSIVE_FACTUAL),
    ("lawyer", "lawyer_of", base::POSSESSIVE_FACTUAL),
    ("client", "client_of", base::POSSESSIVE_FACTUAL),
    ("assistant", "assistant_of", base::POSSESSIVE_FACTUAL),
    ("sponsor", "sponsor_of", base::POSSESSIVE_FACTUAL),
    ("ex", "ex_of", base::POSSESSIVE_FACTUAL),
    ("guardian", "guardian_of", base::POSSESSIVE_FACTUAL),
    ("employer", "employer_of", base::POSSESSIVE_FACTUAL),
    ("servant", "servant_of", base::POSSESSIVE_FACTUAL),
    ("master", "master_of", base::POSSESSIVE_FACTUAL),
    ("teacher", "teacher_of", base::POSSESSIVE_FACTUAL),
    ("student", "student_of", base::POSSESSIVE_FACTUAL),
    ("pupil", "pupil_of", base::POSSESSIVE_FACTUAL),
    ("apprentice", "apprentice_of", base::POSSESSIVE_FACTUAL),
    // Shares its label with verb-mentor-pattern: the same fact, said two ways.
    ("mentor", "mentors", base::POSSESSIVE_FACTUAL),
    // Social stance: same shape, weaker claim.
    ("friend", "friend_of", base::POSSESSIVE_STANCE),
    ("enemy", "enemy_of", base::POSSESSIVE_STANCE),
    ("rival", "rival_of", base::POSSESSIVE_STANCE),
    ("lover", "lover_of", base::POSSESSIVE_STANCE),
    ("companion", "companion_of", base::POSSESSIVE_STANCE),
    ("ally", "ally_of", base::POSSESSIVE_STANCE),
    ("acquaintance", "acquaintance_of", base::POSSESSIVE_STANCE),
];

/// Plural nouns for "x and y are <plural>". "married" is an adjective, listed
/// because it states the same fact the same way.
const PLURAL_NOUNS: &[(&str, &str, f32)] = &[
    ("sisters", "sister_of", base::POSSESSIVE_FACTUAL),
    ("brothers", "brother_of", base::POSSESSIVE_FACTUAL),
    ("siblings", "sibling_of", base::POSSESSIVE_FACTUAL),
    ("cousins", "cousin_of", base::POSSESSIVE_FACTUAL),
    ("twins", "twin_of", base::POSSESSIVE_FACTUAL),
    ("married", "spouse_of", base::POSSESSIVE_FACTUAL),
    ("friends", "friend_of", base::POSSESSIVE_STANCE),
    ("lovers", "lover_of", base::POSSESSIVE_STANCE),
    ("partners", "partner_of", base::POSSESSIVE_STANCE),
    ("colleagues", "colleague_of", base::POSSESSIVE_STANCE),
    ("coworkers", "coworker_of", base::POSSESSIVE_STANCE),
    ("roommates", "roommate_of", base::POSSESSIVE_STANCE),
    ("neighbors", "neighbor_of", base::POSSESSIVE_STANCE),
    ("neighbours", "neighbor_of", base::POSSESSIVE_STANCE),
    ("classmates", "classmate_of", base::POSSESSIVE_STANCE),
    ("rivals", "rival_of", base::POSSESSIVE_STANCE),
    ("enemies", "enemy_of", base::POSSESSIVE_STANCE),
    ("allies", "ally_of", base::POSSESSIVE_STANCE),
];

/// Links that let the possessive be read as a statement about the subject.
///
/// IMPORTANT: matched against the whole trimmed text between the two mentions,
/// never as a substring. "Elena visited Marco's sister" names a third person,
/// and "is not", "was never", "could be" and "believed ... was" each deny,
/// hedge or attribute the claim rather than making it. Every one of those
/// contains a copula; none of them is one.
///
/// The plural copulas are absent. "Dev and Elena are Marco's cousins" has two
/// subjects and the pair loop sees one of them, so extracting it would be half
/// right by construction; the lexicon also matches whole-word and would have
/// to carry "cousins" to reach the phrase at all.
const POSSESSIVE_COPULAS: &[&str] = &["is", "was"];

/// Pre-nominal modifiers that may stand between an appositive comma and the
/// possessor, as in "Bob Spicer, old Mrs. Mingott's father".
///
/// IMPORTANT: closed class, and it holds no verb, no conjunction and no
/// relative pronoun. "Elena, who visited Marco's sister" and "Dev, and Marco's
/// sister" are not appositions, and admitting "who" or "and" here would read
/// both as one.
#[rustfmt::skip]
const APPOSITIVE_MODIFIERS: &[&str] = &[
    "the", "a", "an", "this", "that",
    "his", "her", "their", "its", "my", "our", "your",
    "old", "young", "little", "poor", "dear", "late", "good",
    "younger", "older", "elder", "eldest", "youngest", "oldest", "best", "closest", "only",
    "twin", "adopted", "biological", "favorite", "own", "new",
];

/// Closed-class words that hold the event open without settling it either way.
///
/// IMPORTANT: this is a closed class on purpose. The verbs that suspend a
/// complement (refuse, hope, intend, pretend) are an open one, and are caught
/// structurally by the `to`-infinitive instead. "will" is absent because a
/// future tense asserts.
///
/// A suspender outranks a negator: "if Elena is not Marco's sister" settles
/// nothing, so it is suspended rather than denied. A conditional denial is not
/// a denial, and reading it as one is how a contradiction check invents a
/// conflict the text never states.
const SUSPENDING_WORDS: &[&str] = &[
    "if", "unless", "whether", "could", "would", "might", "may", "should",
];

/// Words that deny the event outright. Paired with the `n't` clitic, which is
/// a suffix rather than a word and is handled beside this list.
const NEGATORS: &[&str] = &["not", "never", "no", "nor", "neither"];

/// Relations where the object admits one subject and no more.
///
/// IMPORTANT: membership is about the world, not about the grammar. A person
/// has one mother and one father; they may have any number of siblings,
/// cousins, teachers and enemies, so those are absent. "widow_of" is absent
/// too: remarriage makes it repeatable over a lifetime, and a manuscript
/// spanning one is not in error for saying so.
///
/// Read with the relation's own direction: an entry reads "X is Y's <noun>",
/// so the constraint binds the object. Two different people asserted as
/// `mother_of` one person contradict each other; one person asserted as
/// `mother_of` two people does not.
const SINGLE_FILLER_RELATIONS: &[&str] = &["mother_of", "father_of"];

/// Verbs read straight from the text between the mentions.
///
/// IMPORTANT: passive voice ("was mentored by") names the mentor second, so the
/// relation's subject is the later entity, not the earlier one. The leading
/// space on each marker is the word boundary in front of it; the mention's own
/// end supplies the one behind.
const VERB_RULES: &[VerbRule] = &[
    VerbRule {
        marker: " mentor",
        also: None,
        passive_marker: Some(" by"),
        relation: "mentors",
        rule: "verb-mentor-pattern",
        base: base::VERB,
    },
    VerbRule {
        marker: " work",
        also: Some(" at"),
        passive_marker: None,
        relation: "works_at",
        rule: "verb-works-at-pattern",
        base: base::VERB,
    },
];

/// The same verbs inside a relative clause, which puts more text between the
/// mentions and scores lower for it. The markers carry no leading space: the
/// relative opener has already supplied the boundary.
const RELATIVE_RULES: &[VerbRule] = &[
    VerbRule {
        marker: "work",
        also: Some("at"),
        passive_marker: None,
        relation: "works_at",
        rule: "relative-works-at-pattern",
        base: base::RELATIVE_CLAUSE,
    },
    VerbRule {
        marker: "mentor",
        also: None,
        passive_marker: None,
        relation: "mentors",
        rule: "relative-mentor-pattern",
        base: base::RELATIVE_CLAUSE,
    },
];
