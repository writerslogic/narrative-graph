//! Chinese prose, read without the English detector.
//!
//! IMPORTANT: none of the three assumptions `pack.rs` names hold here. Han
//! script has no capitals, so capitalization cannot find a name; words are not
//! separated by spaces, so no rule can ask for a word boundary; and a sentence
//! ends at `。．！？` with nothing after them to inspect. This reader therefore
//! shares no detector with the English pipeline, only its output type, and the
//! caller's scoring, rejections and ontology apply to both alike.
//!
//! A mention is one of two things:
//!
//! - A key of `Options::aliases`, matched as a literal run of characters. The
//!   caller has asserted the run names someone, which is the only way a bare
//!   given name (黛玉) can be told from ordinary words without a segmenter.
//! - A surname directly followed by a title: 王夫人, 林姑娘, 邢氏, 賈母. The
//!   title is what makes the surname a name rather than the word it also is.
//!
//! Hard-wrapped text breaks lines mid-word, so line breaks inside a paragraph
//! are dropped before anything is matched. Traditional and simplified forms
//! are both listed wherever they differ.

use super::cooccurrence::base;
use super::relations::{normalize_relation, RelationCandidate, Stance};
use crate::types::Options;
use std::collections::BTreeSet;

/// Widest gap, in characters, any rule reads across. A Han character is
/// roughly a word, so this is a short clause.
const MAX_GAP: usize = 12;

/// Speech has no fixed base in `cooccurrence::base`; the dialogue resolver
/// uses the same value for a tagged speaker.
const SPEECH: f32 = 0.70;

const SURNAMES: &[&str] = &[
    "歐陽", "欧阳", "司馬", "司马", "諸葛", "诸葛", "上官", "東方", "东方", "王", "李", "張", "张",
    "劉", "刘", "陳", "陈", "楊", "杨", "趙", "赵", "黃", "黄", "周", "吳", "吴", "徐", "孫", "孙",
    "胡", "朱", "高", "林", "何", "郭", "馬", "马", "羅", "罗", "梁", "宋", "鄭", "郑", "謝", "谢",
    "韓", "韩", "唐", "馮", "冯", "董", "蕭", "萧", "程", "曹", "袁", "鄧", "邓", "許", "许", "傅",
    "沈", "曾", "彭", "呂", "吕", "蘇", "苏", "盧", "卢", "蔣", "蒋", "蔡", "賈", "贾", "丁", "魏",
    "薛", "葉", "叶", "閻", "阎", "余", "潘", "杜", "戴", "夏", "鍾", "钟", "汪", "田", "任", "姜",
    "范", "方", "石", "姚", "譚", "谭", "廖", "鄒", "邹", "熊", "金", "陸", "陆", "郝", "孔", "白",
    "崔", "康", "毛", "邱", "秦", "江", "史", "顧", "顾", "侯", "邵", "孟", "龍", "龙", "萬", "万",
    "段", "雷", "錢", "钱", "湯", "汤", "尹", "黎", "易", "常", "武", "喬", "乔", "賀", "贺", "賴",
    "赖", "龔", "龚", "文", "邢", "尤", "甄", "鳳", "凤", "柳", "祝", "嚴", "严", "魯", "鲁",
];

/// Titles that make the surname before them a name. Longest first, so 老太太
/// is read whole rather than as 老 followed by 太太.
const TITLES: &[&str] = &[
    "老太太",
    "老太爺",
    "老太爷",
    "夫人",
    "太太",
    "老爺",
    "老爷",
    "先生",
    "小姐",
    "姑娘",
    "奶奶",
    "大人",
    "公子",
    "少爺",
    "少爷",
    "員外",
    "员外",
    "大爺",
    "大爷",
    "二爺",
    "二爷",
    "太君",
    "氏",
    "母",
];

/// Kinship and role nouns: `A的K B` and `B是A的K` both state that B is A's K.
const KIN: &[(&str, &str)] = &[
    ("外祖母", "grandmother_of"),
    ("外祖父", "grandfather_of"),
    ("外甥女", "niece_of"),
    ("妻子", "wife_of"),
    ("老婆", "wife_of"),
    ("夫人", "wife_of"),
    ("太太", "wife_of"),
    ("妻", "wife_of"),
    ("丈夫", "husband_of"),
    ("夫君", "husband_of"),
    ("母親", "mother_of"),
    ("母亲", "mother_of"),
    ("娘親", "mother_of"),
    ("娘亲", "mother_of"),
    ("母", "mother_of"),
    ("父親", "father_of"),
    ("父亲", "father_of"),
    ("爹爹", "father_of"),
    ("父", "father_of"),
    ("女兒", "daughter_of"),
    ("女儿", "daughter_of"),
    ("女", "daughter_of"),
    ("兒子", "son_of"),
    ("儿子", "son_of"),
    ("子", "son_of"),
    ("姐姐", "sister_of"),
    ("姊姊", "sister_of"),
    ("妹妹", "sister_of"),
    ("姐", "sister_of"),
    ("妹", "sister_of"),
    ("哥哥", "brother_of"),
    ("弟弟", "brother_of"),
    ("兄長", "brother_of"),
    ("兄长", "brother_of"),
    ("兄", "brother_of"),
    ("弟", "brother_of"),
    ("祖母", "grandmother_of"),
    ("奶奶", "grandmother_of"),
    ("祖父", "grandfather_of"),
    ("爺爺", "grandfather_of"),
    ("爷爷", "grandfather_of"),
    ("孫女", "granddaughter_of"),
    ("孙女", "granddaughter_of"),
    ("孫子", "grandson_of"),
    ("孙子", "grandson_of"),
    ("舅舅", "uncle_of"),
    ("叔叔", "uncle_of"),
    ("伯父", "uncle_of"),
    ("叔父", "uncle_of"),
    ("舅父", "uncle_of"),
    ("姑媽", "aunt_of"),
    ("姑妈", "aunt_of"),
    ("姨媽", "aunt_of"),
    ("姨妈", "aunt_of"),
    ("姑母", "aunt_of"),
    ("舅母", "aunt_of"),
    ("嬸娘", "aunt_of"),
    ("婶娘", "aunt_of"),
    ("姪女", "niece_of"),
    ("侄女", "niece_of"),
    ("姪兒", "nephew_of"),
    ("侄儿", "nephew_of"),
    ("侄子", "nephew_of"),
    ("姪子", "nephew_of"),
    ("外甥", "nephew_of"),
    ("表哥", "cousin_of"),
    ("表弟", "cousin_of"),
    ("表兄", "cousin_of"),
    ("表姐", "cousin_of"),
    ("表妹", "cousin_of"),
    ("堂兄", "cousin_of"),
    ("堂弟", "cousin_of"),
    ("堂姐", "cousin_of"),
    ("堂妹", "cousin_of"),
    ("丫鬟", "servant_of"),
    ("丫頭", "servant_of"),
    ("丫头", "servant_of"),
    ("侍女", "servant_of"),
    ("小廝", "servant_of"),
    ("小厮", "servant_of"),
    ("僕人", "servant_of"),
    ("仆人", "servant_of"),
    ("朋友", "friend_of"),
    ("好友", "friend_of"),
    ("師父", "teacher_of"),
    ("师父", "teacher_of"),
    ("老師", "teacher_of"),
    ("老师", "teacher_of"),
    ("徒弟", "student_of"),
    ("學生", "student_of"),
    ("学生", "student_of"),
];

/// Links between possessor and kin noun: 的 in the vernacular, 之 in the
/// classical register.
const GENITIVES: &[&str] = &["的", "之"];

/// What may stand between a kin noun and the name it introduces.
const APPOSITIVE_FILLERS: &[&str] = &["", "名叫", "叫做", "名", "叫"];

/// Copulas of `B是A的K`, and their denials.
const COPULAS: &[&str] = &["乃是", "便是", "就是", "即是", "是", "乃", "即", "係", "系"];
const DENIED_COPULAS: &[&str] = &["並不是", "并不是", "不是", "並非", "并非", "非"];

const MARRY: &[&str] = &[
    "娶了",
    "娶",
    "嫁給了",
    "嫁给了",
    "嫁給",
    "嫁给",
    "嫁與",
    "嫁与",
    "嫁了",
    "嫁",
];

/// Adverbs that may open the text between two mentions without changing what
/// the verb after them says.
const ADVERBS: &[&str] = &[
    "已經", "已经", "早已", "已", "便", "就", "又", "遂", "也", "忙",
];

const NEGATORS: &[&str] = &["沒有", "没有", "不曾", "未曾", "未", "沒", "没"];

/// The preposition of `A對B說`: whom the speech is aimed at.
const TOWARD: &[&str] = &["對着", "对着", "對著", "對", "对", "向", "和", "跟"];

/// Speech verbs, longest first. A name directly before one, outside speech,
/// is the speaker.
const SPEECH_VERBS: &[&str] = &[
    "笑道", "說道", "说道", "嘆道", "叹道", "喝道", "罵道", "骂道", "問道", "问道", "回道", "笑說",
    "笑说", "告訴", "告诉", "說", "说", "道", "問", "问", "講", "讲", "叫", "喊",
];

/// Verbs that address the person after them directly: `A問B`.
const ADDRESSING_VERBS: &[&str] = &["告訴", "告诉", "問", "问"];

/// Conditionals hold the claim open; nothing in their sentence is asserted.
const CONDITIONALS: &[&str] = &["如果", "假如", "倘若", "倘或", "若是", "要是", "如若"];

const OPENERS: &[char] = &['“', '「', '『', '‘'];
const CLOSERS: &[char] = &['”', '」', '』', '’'];
const TERMINATORS: &[char] = &['。', '．', '！', '？', '!', '?', '…'];

/// One paragraph with line breaks and indentation removed, each character
/// keeping its byte range in the source.
#[derive(Default)]
struct Paragraph {
    chars: Vec<char>,
    spans: Vec<(usize, usize)>,
}

struct Mention {
    start: usize,
    end: usize,
    name: String,
    quoted: bool,
}

/// Every relation the text states, with spans absolute in `text`.
pub fn relations(text: &str, opts: &Options) -> Vec<RelationCandidate> {
    let mut keys: Vec<(Vec<char>, &str)> = opts
        .aliases
        .iter()
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, value)| (key.chars().collect(), value.as_str()))
        .collect();
    keys.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    let titles = chars_of(TITLES);
    let surnames = chars_of(SURNAMES);

    let mut out = Vec::new();
    for para in paragraphs(text) {
        let quoted = quote_depths(&para.chars);
        let mentions = mentions(&para.chars, &quoted, &keys, &surnames, &titles);
        // Mentions are sorted and sentences come in order, so one cursor
        // walks both.
        let mut next = 0;
        for (from, to) in sentences(&para.chars) {
            while next < mentions.len() && mentions[next].start < from {
                next += 1;
            }
            let inside: Vec<&Mention> = mentions[next..]
                .iter()
                .take_while(|m| m.start < to)
                .filter(|m| m.end <= to)
                .collect();
            if inside.is_empty() {
                continue;
            }
            let held_open = contains_any(&para.chars[from..to], CONDITIONALS);
            let question = para.chars[from..to]
                .iter()
                .rev()
                .find(|c| !CLOSERS.contains(c))
                .is_some_and(|c| *c == '？' || *c == '?');
            let sentence = Sentence {
                chars: &para.chars,
                end: to,
                held_open,
                question,
            };
            for (i, a) in inside.iter().enumerate() {
                for b in &inside[i + 1..] {
                    if b.start < a.end || b.start - a.end > MAX_GAP {
                        break;
                    }
                    if a.name == b.name || a.quoted != b.quoted {
                        continue;
                    }
                    let between = &para.chars[a.end..b.start];
                    if between
                        .iter()
                        .any(|c| OPENERS.contains(c) || CLOSERS.contains(c))
                    {
                        continue;
                    }
                    if let Some(found) = sentence.pair(b, between) {
                        out.push(candidate(found, a, b, between.len(), &para, opts));
                    }
                }
            }
            out.extend(
                vocatives(&para.chars, &quoted, &inside, from)
                    .into_iter()
                    .map(|(a, b)| {
                        let found = Found {
                            relation: "speaks_to",
                            rule: "zh-vocative",
                            base: SPEECH,
                            stance: Stance::Asserted,
                            swapped: false,
                        };
                        candidate(found, a, b, 0, &para, opts)
                    }),
            );
        }
    }
    out
}

struct Sentence<'a> {
    chars: &'a [char],
    end: usize,
    held_open: bool,
    question: bool,
}

struct Found {
    relation: &'static str,
    rule: &'static str,
    base: f32,
    stance: Stance,
    /// The relation's subject is the later mention.
    swapped: bool,
}

impl Sentence<'_> {
    fn pair(&self, b: &Mention, between: &[char]) -> Option<Found> {
        let speech = self.speech(b, between);
        if speech.is_some() {
            return speech;
        }
        if self.held_open || self.question {
            return None;
        }
        self.appositive(between)
            .or_else(|| self.copula(b, between))
            .or_else(|| marriage(between))
    }

    /// `A的K B`: B is A's K.
    fn appositive(&self, between: &[char]) -> Option<Found> {
        let rest = strip_any(between, GENITIVES)?;
        let (kin, relation) = longest_prefix(rest, KIN)?;
        let filler = &rest[kin..];
        APPOSITIVE_FILLERS
            .iter()
            .any(|f| equals(filler, f))
            .then_some(Found {
                relation,
                rule: "zh-possessive",
                base: base::POSSESSIVE_FACTUAL,
                stance: Stance::Asserted,
                swapped: true,
            })
    }

    /// `B是A的K`: B is A's K, read after the second mention.
    fn copula(&self, b: &Mention, between: &[char]) -> Option<Found> {
        let (stance, rest) = if let Some(rest) = strip_any(between, DENIED_COPULAS) {
            (Stance::Denied, rest)
        } else {
            (Stance::Asserted, strip_any(between, COPULAS)?)
        };
        if !rest.is_empty() {
            return None;
        }
        let after = &self.chars[b.end..self.end];
        let after = strip_any(after, GENITIVES)?;
        let (kin, relation) = longest_prefix(after, KIN)?;
        // `B是A的妹妹的丈夫` names someone else's kin.
        if after
            .get(kin)
            .is_some_and(|c| GENITIVES.iter().any(|g| equals(&[*c], g)))
        {
            return None;
        }
        Some(Found {
            relation,
            rule: "zh-copula",
            base: base::POSSESSIVE_FACTUAL,
            stance,
            swapped: false,
        })
    }

    /// `A對B說`, `A問B`: A speaks to B. Asked or conditional, the speech still
    /// happened, so neither holds this one open.
    fn speech(&self, b: &Mention, between: &[char]) -> Option<Found> {
        let rest = skip_adverbs(between);
        let addressed = if let Some(rest) = strip_any(rest, TOWARD) {
            rest.is_empty() && starts_with_any(&self.chars[b.end..self.end], SPEECH_VERBS)
        } else {
            ADDRESSING_VERBS.iter().any(|v| {
                strip_any(rest, &[v]).is_some_and(|tail| tail.is_empty() || equals(tail, "了"))
            })
        };
        addressed.then_some(Found {
            relation: "speaks_to",
            rule: "zh-addressed",
            base: SPEECH,
            stance: Stance::Asserted,
            swapped: false,
        })
    }
}

/// `A娶了B`, `A嫁給B`: A and B married.
fn marriage(between: &[char]) -> Option<Found> {
    let rest = skip_adverbs(between);
    let (stance, rest) = match strip_any(rest, NEGATORS) {
        Some(rest) => (Stance::Denied, rest),
        None => (Stance::Asserted, rest),
    };
    MARRY.iter().any(|m| equals(rest, m)).then_some(Found {
        relation: "spouse_of",
        rule: "zh-marriage",
        base: base::VERB,
        stance,
        swapped: false,
    })
}

/// A speech tag whose quotation opens on a name and a pause:
/// `賈母笑道：“鳳丫頭，……”` is the grandmother speaking to 鳳丫頭.
fn vocatives<'m>(
    chars: &[char],
    quoted: &[bool],
    mentions: &[&'m Mention],
    from: usize,
) -> Vec<(&'m Mention, &'m Mention)> {
    let mut out = Vec::new();
    for listener in mentions.iter().filter(|m| m.quoted) {
        let open = listener.start.checked_sub(1);
        if !open.is_some_and(|i| i >= from && OPENERS.contains(&chars[i])) {
            continue;
        }
        if !chars
            .get(listener.end)
            .is_some_and(|c| matches!(c, '，' | ',' | '！' | '!' | '。' | '．'))
        {
            continue;
        }
        let mut tag_end = listener.start - 1;
        while tag_end > from && matches!(chars[tag_end - 1], '：' | ':' | '，' | ',') {
            tag_end -= 1;
        }
        let Some(verb) = SPEECH_VERBS.iter().find(|v| {
            let v: Vec<char> = v.chars().collect();
            tag_end >= from + v.len() && chars[tag_end - v.len()..tag_end] == v[..]
        }) else {
            continue;
        };
        let verb_start = tag_end - verb.chars().count();
        // The speaker is the nearest name before the verb, outside speech, with
        // no more than a few characters of manner (忙, 笑着) between.
        let speaker = mentions
            .iter()
            .filter(|m| !m.quoted && m.end <= verb_start && verb_start - m.end <= 4)
            .filter(|m| !quoted[m.end..verb_start].iter().any(|q| *q))
            .max_by_key(|m| m.end);
        if let Some(speaker) = speaker {
            if speaker.name != listener.name {
                out.push((*speaker, *listener));
            }
        }
    }
    out
}

fn candidate(
    found: Found,
    a: &Mention,
    b: &Mention,
    gap: usize,
    para: &Paragraph,
    opts: &Options,
) -> RelationCandidate {
    let (subject, object) = if found.swapped { (b, a) } else { (a, b) };
    let first = a.start.min(b.start);
    let last = a.end.max(b.end) - 1;
    RelationCandidate {
        subject: subject.name.clone(),
        relation: normalize_relation(found.relation, &opts.ontology),
        object: object.name.clone(),
        rule: found.rule.to_string(),
        span: [para.spans[first].0, para.spans[last].1],
        base: found.base,
        gap,
        stance: found.stance,
        single_filler: matches!(found.relation, "mother_of" | "father_of"),
    }
}

/// Paragraphs: a blank line ends one, and so does a line indented as the
/// start of the next where the text indents its paragraphs. Without either
/// convention every line is a paragraph.
fn paragraphs(text: &str) -> Vec<Paragraph> {
    let indents = text
        .lines()
        .any(|l| l.starts_with('\u{3000}') || l.starts_with("  "));
    let blanks = text.contains("\n\n") || text.contains("\r\n\r\n");
    let mut out = Vec::new();
    let mut current = Paragraph::default();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        let indented = body.starts_with(|c: char| c.is_whitespace());
        let blank = body.trim().is_empty();
        let breaks = blank || (indents && indented) || (!indents && !blanks);
        if breaks && !current.chars.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        let lead = body.len() - body.trim_start().len();
        for (i, c) in body[lead..].char_indices() {
            let at = start + lead + i;
            current.chars.push(c);
            current.spans.push((at, at + c.len_utf8()));
        }
    }
    if !current.chars.is_empty() {
        out.push(current);
    }
    out
}

/// Whether each character is inside a quotation, its marks included. Nested
/// quotations count once; an unclosed one runs to the end of the paragraph.
fn quote_depths(chars: &[char]) -> Vec<bool> {
    let mut depth = 0usize;
    chars
        .iter()
        .map(|c| {
            if OPENERS.contains(c) {
                depth += 1;
                true
            } else if CLOSERS.contains(c) {
                let inside = depth > 0;
                depth = depth.saturating_sub(1);
                inside
            } else {
                depth > 0
            }
        })
        .collect()
}

/// Sentence ranges `[from, to)`, each ending after its terminators and any
/// closing marks that follow them.
fn sentences(chars: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    let mut i = 0;
    while i < chars.len() {
        if TERMINATORS.contains(&chars[i]) {
            let mut end = i + 1;
            while end < chars.len()
                && (TERMINATORS.contains(&chars[end]) || CLOSERS.contains(&chars[end]))
            {
                end += 1;
            }
            out.push((from, end));
            from = end;
            i = end;
        } else {
            i += 1;
        }
    }
    if from < chars.len() {
        out.push((from, chars.len()));
    }
    out
}

fn mentions(
    chars: &[char],
    quoted: &[bool],
    keys: &[(Vec<char>, &str)],
    surnames: &[Vec<char>],
    titles: &[Vec<char>],
) -> Vec<Mention> {
    let mut taken = vec![false; chars.len()];
    let mut out = Vec::new();
    let mut claim = |start: usize, end: usize, name: String, out: &mut Vec<Mention>| {
        if taken[start..end].iter().any(|t| *t) {
            return;
        }
        taken[start..end].iter_mut().for_each(|t| *t = true);
        out.push(Mention {
            start,
            end,
            name,
            quoted: quoted[start],
        });
    };
    for (key, name) in keys {
        let mut i = 0;
        while i + key.len() <= chars.len() {
            if chars[i..i + key.len()] == key[..] {
                claim(i, i + key.len(), (*name).to_string(), &mut out);
                i += key.len();
            } else {
                i += 1;
            }
        }
    }
    // Most characters open no surname; testing the first one alone skips the
    // list for them.
    let firsts: BTreeSet<char> = surnames.iter().filter_map(|s| s.first().copied()).collect();
    for start in 0..chars.len() {
        if !firsts.contains(&chars[start]) {
            continue;
        }
        let Some(surname) = surnames.iter().find(|s| chars[start..].starts_with(s)) else {
            continue;
        };
        let after = start + surname.len();
        if let Some(title) = titles.iter().find(|t| chars[after..].starts_with(t)) {
            let end = after + title.len();
            claim(start, end, chars[start..end].iter().collect(), &mut out);
        }
    }
    out.sort_by_key(|m| m.start);
    out
}

fn chars_of(words: &[&str]) -> Vec<Vec<char>> {
    let mut out: Vec<Vec<char>> = words.iter().map(|w| w.chars().collect()).collect();
    out.sort_by_key(|w| std::cmp::Reverse(w.len()));
    out
}

fn equals(chars: &[char], word: &str) -> bool {
    chars.iter().copied().eq(word.chars())
}

fn starts_with_any(chars: &[char], words: &[&str]) -> bool {
    words.iter().any(|w| strip(chars, w).is_some())
}

fn contains_any(chars: &[char], words: &[&str]) -> bool {
    (0..chars.len()).any(|i| starts_with_any(&chars[i..], words))
}

fn strip<'c>(chars: &'c [char], word: &str) -> Option<&'c [char]> {
    let n = word.chars().count();
    (chars.len() >= n && equals(&chars[..n], word)).then(|| &chars[n..])
}

/// The rest after the longest of `words` that opens `chars`.
fn strip_any<'c>(chars: &'c [char], words: &[&str]) -> Option<&'c [char]> {
    words
        .iter()
        .filter_map(|w| strip(chars, w))
        .min_by_key(|rest| rest.len())
}

fn skip_adverbs(mut chars: &[char]) -> &[char] {
    while let Some(rest) = strip_any(chars, ADVERBS) {
        chars = rest;
    }
    chars
}

/// The length and relation of the longest kin noun opening `chars`.
fn longest_prefix(chars: &[char], table: &[(&str, &'static str)]) -> Option<(usize, &'static str)> {
    table
        .iter()
        .filter(|(word, _)| strip(chars, word).is_some())
        .map(|(word, relation)| (word.chars().count(), *relation))
        .max_by_key(|(len, _)| *len)
}

#[cfg(test)]
mod tests {
    use super::{paragraphs, sentences};

    #[test]
    fn a_hard_wrapped_paragraph_joins_its_lines_and_keeps_byte_spans() {
        let text = "\u{3000}\u{3000}賈赦之妻邢\n氏忙亦起身．\n\u{3000}\u{3000}次段。";
        let paras = paragraphs(text);
        assert_eq!(paras.len(), 2);
        let first: String = paras[0].chars.iter().collect();
        assert_eq!(first, "賈赦之妻邢氏忙亦起身．");
        for (c, (s, e)) in paras[0].chars.iter().zip(&paras[0].spans) {
            assert_eq!(&text[*s..*e], c.to_string());
        }
    }

    #[test]
    fn a_sentence_keeps_the_closing_mark_after_its_terminator() {
        let chars: Vec<char> = "他道：“去罷。”她笑了．".chars().collect();
        let ends: Vec<usize> = sentences(&chars).into_iter().map(|(_, e)| e).collect();
        assert_eq!(ends, vec![8, 12]);
    }
}
