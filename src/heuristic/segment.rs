//! Sentence segmentation for narrative prose.
//!
//! Hand-rolled and dependency-free. A naive split on `.`/`!`/`?` misparses
//! exactly the inputs this crate exists to handle — honorifics, initials,
//! ellipses, and quoted dialogue with attribution — so each of those is
//! handled explicitly here and covered by the tests at the bottom of this file.

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

/// Split prose into sentences, returning each as a borrowed slice of the input
/// paired with its byte offset in `text`.
///
/// Borrowing rather than allocating keeps this allocation-free per sentence;
/// the returned offsets let candidate spans map back to the original text.
pub fn split_sentences(text: &str) -> Vec<(&str, usize)> {
    let bytes = text.as_bytes();
    let mut sentences = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;

    while i < text.len() {
        if !matches!(bytes[i], b'.' | b'!' | b'?') {
            i += 1;
            continue;
        }

        let run_start = i;
        let mut j = i;
        while j < text.len() && matches!(bytes[j], b'.' | b'!' | b'?') {
            j += 1;
        }
        let run = &text[run_start..j];

        // A terminator may be followed by closing quotes or brackets that
        // belong to the same sentence: `"Get out!" she said.`
        let mut closed_quote = false;
        loop {
            let rest = &text[j..];
            if rest.starts_with(['"', '\'', ')', ']', '\u{201D}', '\u{2019}']) {
                closed_quote = true;
                j += rest.chars().next().map_or(0, char::len_utf8);
            } else {
                break;
            }
        }

        let at_end = j >= text.len();
        let next_is_space = !at_end && text[j..].starts_with(char::is_whitespace);

        if (at_end || next_is_space) && is_boundary(text, run_start, run, closed_quote, j, at_end) {
            push_sentence(&mut sentences, text, start, j);
            start = j;
            while start < text.len() && text[start..].starts_with(char::is_whitespace) {
                start += text[start..].chars().next().map_or(1, char::len_utf8);
            }
            i = start.max(j);
            continue;
        }

        i = j.max(run_start + 1);
    }

    push_sentence(&mut sentences, text, start, text.len());
    sentences
}

fn push_sentence<'t>(out: &mut Vec<(&'t str, usize)>, text: &'t str, start: usize, end: usize) {
    if start >= end {
        return;
    }
    let raw = &text[start..end];
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    let offset = start + (raw.len() - raw.trim_start().len());
    out.push((trimmed, offset));
}

fn is_boundary(
    text: &str,
    run_start: usize,
    run: &str,
    closed_quote: bool,
    next: usize,
    at_end: bool,
) -> bool {
    if at_end {
        return true;
    }

    // A single period after a known abbreviation or a lone initial ("J. R. R.
    // Tolkien") continues the sentence. Quoted dialogue is exempt: the period
    // belongs to the quote, not to an abbreviation.
    if run == "." && !closed_quote {
        let preceding = preceding_word(text, run_start);
        if is_abbreviation(preceding) {
            return false;
        }
    }

    // A lowercase continuation is never a new sentence — it is dialogue
    // attribution (`"Who are you?" she asked.`) or an unlisted abbreviation.
    match next_word_start(text, next) {
        Some(c) => !c.is_lowercase(),
        None => true,
    }
}

fn preceding_word(text: &str, end: usize) -> &str {
    let start = text[..end]
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_alphabetic())
        .map_or(0, |(idx, c)| idx + c.len_utf8());
    &text[start..end]
}

fn is_abbreviation(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    // A lone capital is an initial: "J. R. R. Tolkien", "T. S. Eliot".
    let mut chars = word.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_uppercase() {
            return true;
        }
    }
    ABBREVIATIONS
        .iter()
        .any(|abbr| abbr.eq_ignore_ascii_case(word))
}

fn next_word_start(text: &str, from: usize) -> Option<char> {
    text[from..]
        .chars()
        .find(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '(' | '\u{201C}' | '\u{2018}'))
}

#[cfg(test)]
mod tests {
    use super::split_sentences;

    fn texts(input: &str) -> Vec<&str> {
        split_sentences(input).into_iter().map(|(s, _)| s).collect()
    }

    #[test]
    fn splits_plain_sentences() {
        assert_eq!(
            texts("Elena went home. Marco stayed."),
            vec!["Elena went home.", "Marco stayed."]
        );
    }

    #[test]
    fn offsets_point_at_the_sentence_in_the_source() {
        let text = "Elena went home. Marco stayed.";
        for (sentence, offset) in split_sentences(text) {
            assert_eq!(&text[offset..offset + sentence.len()], sentence);
        }
    }

    #[test]
    fn does_not_split_on_honorifics() {
        assert_eq!(texts("Dr. Smith went home."), vec!["Dr. Smith went home."]);
        assert_eq!(
            texts("Mrs. Chen met Lt. Vasquez at the dock."),
            vec!["Mrs. Chen met Lt. Vasquez at the dock."]
        );
    }

    #[test]
    fn does_not_split_on_initials() {
        assert_eq!(
            texts("J. R. R. Tolkien wrote it."),
            vec!["J. R. R. Tolkien wrote it."]
        );
    }

    #[test]
    fn does_not_split_inside_an_ellipsis() {
        assert_eq!(
            texts("She paused... then left."),
            vec!["She paused... then left."]
        );
    }

    #[test]
    fn does_not_split_on_a_decimal_number() {
        assert_eq!(texts("It cost 3.5 credits."), vec!["It cost 3.5 credits."]);
    }

    #[test]
    fn keeps_dialogue_attribution_with_its_quote() {
        assert_eq!(
            texts("\"Who are you?\" she asked."),
            vec!["\"Who are you?\" she asked."]
        );
        assert_eq!(
            texts("\"Get out,\" Marco said. Elena left."),
            vec!["\"Get out,\" Marco said.", "Elena left."]
        );
    }

    #[test]
    fn splits_after_a_closing_quote_when_a_new_sentence_follows() {
        assert_eq!(
            texts("\"Get out!\" Elena ran."),
            vec!["\"Get out!\"", "Elena ran."]
        );
    }

    #[test]
    fn treats_a_terminator_run_as_one_boundary() {
        assert_eq!(texts("What?! Really."), vec!["What?!", "Really."]);
    }

    #[test]
    fn does_not_split_on_an_em_dash() {
        assert_eq!(
            texts("He turned—she was gone."),
            vec!["He turned—she was gone."]
        );
    }

    #[test]
    fn handles_text_with_no_terminator() {
        assert_eq!(texts("Elena went home"), vec!["Elena went home"]);
    }

    #[test]
    fn handles_empty_and_whitespace_input() {
        assert!(texts("").is_empty());
        assert!(texts("   \n  ").is_empty());
    }

    #[test]
    fn preserves_offsets_through_multibyte_text() {
        let text = "Élena went home. Marco—her brother—stayed.";
        for (sentence, offset) in split_sentences(text) {
            assert_eq!(&text[offset..offset + sentence.len()], sentence);
        }
    }
}
