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

/// Everything the pipeline reads as vocabulary for one language.
pub struct LanguagePack {
    pub sentence_openers: &'static [&'static str],
    pub name_particles: &'static [&'static str],
    pub honorifics: &'static [&'static str],
    pub honorific_qualifiers: &'static [&'static str],
    pub pronouns: &'static [&'static str],
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
    "in", "indeed", "instead", "is", "its", "just", "later", "maybe", "meanwhile", "my", "neither",
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
