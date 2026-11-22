//! What extraction knows beyond the paragraph in front of it.
//!
//! A paragraph read alone knows little: a bare given name carries no title, and
//! the one sentence that says "she" settles nothing about a name that three
//! hundred other paragraphs settle at once. A [`Context`] carries that
//! evidence from one call to the next, and names the first person for a
//! narrator who is never called anything in the text.

use super::pack::LanguagePack;
use super::quotes::QuoteMap;
use super::register::Register;
use super::{entities, segment};

/// Document-wide evidence about names, shared across extraction calls.
///
/// ```
/// use narrative_graph::heuristic::{extract_aggregates_in, Context};
/// use narrative_graph::Options;
///
/// let mut context = Context::default().with_narrator("wren");
/// context.learn("Mrs. Chen waved. Then she fed the cat.");
/// let found = extract_aggregates_in("I met my mother, Nora.", &Options::default(), &context)
///     .unwrap();
/// assert!(found.iter().any(|a| a.relation == "mother_of"));
/// ```
#[derive(Debug, Default, Clone)]
pub struct Context {
    register: Register,
    narrator: Option<String>,
}

impl Context {
    /// Resolve "I", "me" and "my" outside quoted speech to `canonical`, the
    /// normalized name (lowercase, underscores) the narrator should carry in
    /// the graph. Without one, the first person is never a mention.
    pub fn with_narrator(mut self, canonical: &str) -> Self {
        self.narrator = Some(canonical.to_string());
        self
    }

    /// Record what `text` says about the names in it. Call it for every
    /// paragraph before extracting from any of them, so that a name's evidence
    /// is complete when its first mention is read.
    pub fn learn(&mut self, text: &str) {
        observe_text(&mut self.register, text, &super::pack::ENGLISH);
    }

    pub(super) fn register(&self) -> &Register {
        &self.register
    }

    pub(super) fn narrator(&self) -> Option<&str> {
        self.narrator.as_deref()
    }
}

/// Feed one text's mentions and pronouns to a register, outside speech only:
/// a vocative inside a quotation is not the subject of the pronoun after it.
pub(super) fn observe_text(register: &mut Register, text: &str, pack: &LanguagePack) {
    let mut open = false;
    let mut previous_end = 0usize;
    for (sent_text, sent_start) in segment::split_sentences(text, pack) {
        if text[previous_end..sent_start].contains('\n') {
            open = false;
        }
        previous_end = sent_start + sent_text.len();
        let quotes = QuoteMap::scan(sent_text, open);
        open = quotes.open_at_end;

        let mentions: Vec<_> = entities::mentions_for_register(sent_text, pack)
            .into_iter()
            .filter(|m| !quotes.inside(m.start))
            .collect();
        let pronouns: Vec<_> = entities::pronouns_for_register(sent_text, pack)
            .into_iter()
            .filter(|(_, start)| !quotes.inside(*start))
            .collect();
        register.observe(sent_text, &mentions, &pronouns, pack);
    }
}
