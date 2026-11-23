//! The Chinese reader on passages of 紅樓夢 (Project Gutenberg #24264, public
//! domain), hard wraps and fullwidth stops kept as the edition prints them.

use narrative_graph::{
    extract_aggregates, extract_candidate_triples, Language, Options, Polarity, TripleCandidate,
};

fn chinese(cast: &[(&str, &str)]) -> Options {
    Options {
        language: Language::Chinese,
        aliases: cast
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..Default::default()
    }
}

fn extract(text: &str, cast: &[(&str, &str)]) -> Vec<TripleCandidate> {
    extract_candidate_triples(text, &chinese(cast)).expect("extraction failed")
}

fn triples(candidates: &[TripleCandidate]) -> Vec<(&str, &str, &str)> {
    candidates
        .iter()
        .map(|c| (c.subject.as_str(), c.relation.as_str(), c.object.as_str()))
        .collect()
}

#[test]
fn a_classical_genitive_names_a_wife_across_a_hard_wrap() {
    let text = "\u{3000}\u{3000}當下茶果已撤，賈母命兩個老嬤嬤帶了黛玉去見兩個母舅．時賈赦之妻\n邢氏忙亦起身，笑回道：“我帶了外甥女過去，倒也便宜。”";
    let found = extract(text, &[("賈赦", "jia_she")]);
    assert_eq!(triples(&found), vec![("邢氏", "wife_of", "jia_she")]);
    // The span runs from the husband's name to the end of the wife's, across
    // the line break the edition wraps on.
    let [start, end] = found[0].span;
    assert_eq!(&text[start..end], "賈赦之妻\n邢氏");
}

#[test]
fn a_vernacular_genitive_and_a_copula_state_the_same_kinship() {
    let possessive = "與榮國府賈政的夫人王氏，是一母所生的姊妹，今年方四十上下年紀．";
    assert_eq!(
        triples(&extract(possessive, &[("賈政", "jia_zheng")])),
        vec![("王氏", "wife_of", "jia_zheng")]
    );

    let copula = "\u{3000}\u{3000}原來這李氏即賈珠之妻．珠雖夭亡，幸存一子，取名賈蘭，今方五歲，";
    let found = extract(copula, &[("賈珠", "jia_zhu")]);
    assert_eq!(triples(&found), vec![("李氏", "wife_of", "jia_zhu")]);
    assert_eq!(found[0].rule, "zh-copula");
}

#[test]
fn a_denied_copula_is_a_denial_not_an_assertion() {
    // Constructed, in simplified characters: the edition has no denial of a
    // named kinship.
    let text = "王熙凤并非贾赦的女儿。";
    let opts = chinese(&[("王熙凤", "wang_xifeng"), ("贾赦", "jia_she")]);
    let aggregates = extract_aggregates(text, &opts).expect("aggregation failed");
    assert_eq!(aggregates.len(), 1);
    assert_eq!(
        (
            aggregates[0].subject.as_str(),
            aggregates[0].relation.as_str(),
            aggregates[0].object.as_str(),
            aggregates[0].polarity,
        ),
        ("wang_xifeng", "daughter_of", "jia_she", Polarity::Denied)
    );
    assert!(extract_candidate_triples(text, &opts).unwrap().is_empty());
}

#[test]
fn a_conditional_holds_a_kinship_open() {
    // Constructed: the same copula, held open by 若是.
    let text = "若是王熙凤是贾赦的女儿，便好了。";
    let cast = [("王熙凤", "wang_xifeng"), ("贾赦", "jia_she")];
    assert!(extract(text, &cast).is_empty());
}

#[test]
fn a_marriage_verb_joins_the_two_it_names() {
    let text = "\u{3000}\u{3000}當時黛玉气絕， 正是寶玉娶寶釵的這個時辰．紫鵑等都大哭起來．";
    let cast = [("寶玉", "baoyu"), ("寶釵", "baochai"), ("黛玉", "daiyu")];
    assert_eq!(
        triples(&extract(text, &cast)),
        vec![("baoyu", "spouse_of", "baochai")]
    );
}

#[test]
fn speech_aimed_at_someone_is_speaks_to() {
    let asked = "\u{3000}\u{3000}一時寶玉來了， 寶釵方出去．寶玉便問襲人道：“怎么寶\n姐姐和你說的這么熱鬧，見我進來就跑了？ ";
    let cast = [("寶玉", "baoyu"), ("寶釵", "baochai"), ("襲人", "xiren")];
    assert_eq!(
        triples(&extract(asked, &cast)),
        vec![("baoyu", "speaks_to", "xiren")]
    );

    // 王夫人 is found by her title, with no cast entry.
    let toward = "賈母向王夫人道：“讓他們小妯娌伏侍，你在那里\n坐了，好說話儿。”";
    assert_eq!(
        triples(&extract(toward, &[("賈母", "grandmother_jia")])),
        vec![("grandmother_jia", "speaks_to", "王夫人")]
    );
}

#[test]
fn a_quotation_that_opens_on_a_name_is_said_to_that_name() {
    let text = "\u{3000}\u{3000}二人正說話，只見紫鵑進來．寶玉笑道：“紫鵑，把你\n們的好茶倒碗我吃。”紫鵑道：“那里是好的呢？要好的，\n只是等襲人來。”";
    let cast = [("寶玉", "baoyu"), ("紫鵑", "zijuan"), ("襲人", "xiren")];
    let found = extract(text, &cast);
    assert_eq!(triples(&found), vec![("baoyu", "speaks_to", "zijuan")]);
    assert_eq!(found[0].rule, "zh-vocative");
}

#[test]
fn english_options_find_nothing_in_chinese_text() {
    let text = "時賈赦之妻邢氏忙亦起身．";
    assert!(extract_candidate_triples(text, &Options::default())
        .unwrap()
        .is_empty());
}

#[test]
fn a_straight_mark_closes_a_curly_quotation() {
    // Two quotations in this paragraph close on `"`. Read as still open, they
    // would put 黛玉's later tag inside speech, and the vocative would have no
    // speaker.
    let text = "\u{3000}\u{3000}倒是\n襲人听見帘子響，從屋里出來一看，見是黛玉，便讓道：“姑娘屋里坐罷。”黛\n玉笑著道：“寶二爺在家么？\"襲人不知底里，剛要答言，只見紫鵑在黛玉身后\n和他努嘴儿，指著黛玉，又搖搖手儿．襲人不解何意，也不敢言語．\n听著黛玉說道：“寶玉，你為什么病了？\"寶玉";
    let cast = [
        ("寶玉", "baoyu"),
        ("黛玉", "daiyu"),
        ("襲人", "xiren"),
        ("紫鵑", "zijuan"),
    ];
    let found = extract(text, &cast);
    assert_eq!(triples(&found), vec![("daiyu", "speaks_to", "baoyu")]);
    assert_eq!(found[0].rule, "zh-vocative");
}

#[test]
fn a_quotation_carried_into_a_paragraph_ends_at_its_first_mark() {
    // The paragraph opens inside speech begun in the one before; its first
    // mark closes it. Read as an opener, every later tag would sit in speech.
    let text = "\u{3000}\u{3000}了不成？\"寶玉和襲人嗤的一笑．寶玉道：“你何苦來替他招罵名儿．”襲人笑道：“林姑娘，你不知道我的心事。”";
    let cast = [("寶玉", "baoyu"), ("襲人", "xiren")];
    let found = extract(text, &cast);
    assert_eq!(triples(&found), vec![("xiren", "speaks_to", "林姑娘")]);
}
