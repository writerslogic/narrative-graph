//! Which bytes of a sentence are spoken.
//!
//! IMPORTANT: a relation read across a quotation mark pairs a name inside
//! someone's speech with a name outside it. `"Papa's stuck at work," Nora says`
//! is a line of dialogue followed by an attribution, and neither the speech nor
//! the speaker states that Papa works at Nora. Every stage that pairs or
//! resolves mentions asks this map first.
//!
//! A straight double quote toggles; curly and guillemet marks open and close
//! explicitly. Apostrophes and single quotes are not read: they are
//! possessives and contractions far more often than quotation marks. The state
//! is carried from one sentence to the next, because a quotation spans
//! sentences ("I know. You don't." she says) and the segmenter splits inside it.

#[derive(Debug, Default, Clone)]
pub struct QuoteMap {
    /// Byte ranges `[open, close)` of spoken text, quote marks included.
    ranges: Vec<(usize, usize)>,
    /// Whether a quotation is still open at the end of the sentence.
    pub open_at_end: bool,
}

const OPENERS: [char; 4] = ['\u{201C}', '\u{201E}', '\u{00AB}', '\u{300C}'];
const CLOSERS: [char; 3] = ['\u{201D}', '\u{00BB}', '\u{300D}'];

pub fn is_quote_mark(c: char) -> bool {
    c == '"' || OPENERS.contains(&c) || CLOSERS.contains(&c)
}

impl QuoteMap {
    /// Scan one sentence, starting inside a quotation when `open` says so.
    pub fn scan(text: &str, open: bool) -> Self {
        let mut ranges = Vec::new();
        let mut open_from = open.then_some(0usize);

        for (pos, c) in text.char_indices() {
            if c == '"' {
                match open_from.take() {
                    Some(from) => ranges.push((from, pos + 1)),
                    None => open_from = Some(pos),
                }
            } else if OPENERS.contains(&c) {
                open_from.get_or_insert(pos);
            } else if CLOSERS.contains(&c) {
                if let Some(from) = open_from.take() {
                    ranges.push((from, pos + c.len_utf8()));
                }
            }
        }

        let open_at_end = open_from.is_some();
        if let Some(from) = open_from {
            ranges.push((from, text.len()));
        }
        Self {
            ranges,
            open_at_end,
        }
    }

    pub fn inside(&self, pos: usize) -> bool {
        self.ranges.iter().any(|&(a, b)| a <= pos && pos < b)
    }

    /// Whether `[start, end)` lies wholly in speech or wholly outside it, with
    /// no quotation mark inside it. A span that starts in one and ends in the
    /// other, or that holds a mark, runs from speech into narration or across
    /// two utterances.
    pub fn contains_span(&self, text: &str, start: usize, end: usize) -> bool {
        if end <= start {
            return true;
        }
        let first = self.inside(start);
        let last = self.inside(end - 1);
        first == last && !text[start..end].chars().any(is_quote_mark)
    }
}

#[cfg(test)]
mod tests {
    use super::QuoteMap;

    #[test]
    fn marks_speech_and_carries_state_across_a_split() {
        let text = "\"Marco left,\" Nora says.";
        let map = QuoteMap::scan(text, false);
        assert!(map.inside(2));
        assert!(!map.inside(text.find("Nora").unwrap()));
        assert!(!map.open_at_end);

        let open = QuoteMap::scan("\"I know.", false);
        assert!(open.open_at_end);
        let rest = QuoteMap::scan("You don't.\" she says.", open.open_at_end);
        assert!(rest.inside(0));
        assert!(!rest.inside(rest_pos("she")));
    }

    fn rest_pos(word: &str) -> usize {
        "You don't.\" she says.".find(word).unwrap()
    }

    #[test]
    fn a_span_across_a_mark_is_not_contained() {
        let text = "\"Papa's stuck at work,\" Nora says.";
        let map = QuoteMap::scan(text, false);
        assert!(!map.contains_span(text, 1, text.find("Nora").unwrap() + 4));
        assert!(map.contains_span(text, 1, 5));
    }
}
