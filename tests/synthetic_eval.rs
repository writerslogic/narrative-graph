//! Precision and recall over a hand-written synthetic fiction set.
//!
//! Every paragraph below was written for this file; none is quoted from a real
//! manuscript. A case lists the claims its paragraph states (`gold`), and the
//! score counts an extracted (subject, relation, object, polarity) as correct
//! only when it is listed. A case with no gold is a negative: dialogue
//! fragments, pronouns, common nouns, modal and conditional phrasing, and
//! possessives that name a third party, where any output is a false positive.
//!
//! Run `cargo test --test synthetic_eval -- --nocapture` for the numbers and
//! the per-case misses.

use narrative_graph::heuristic::Context;
use narrative_graph::{extract_aggregates_in, Options, Polarity};
use std::collections::BTreeSet;

type Claim = (&'static str, &'static str, &'static str, Polarity);

const A: Polarity = Polarity::Asserted;
const D: Polarity = Polarity::Denied;

struct Case {
    text: &'static str,
    gold: &'static [Claim],
}

const fn case(text: &'static str, gold: &'static [Claim]) -> Case {
    Case { text, gold }
}

/// Paragraphs read with no shared context, as a caller extracting one paragraph
/// at a time sees them.
const CASES: &[Case] = &[
    // Kinship: possessive, prefix, appositive, genitive.
    case(
        "Elena is Marco's sister.",
        &[("elena", "sister_of", "marco", A)],
    ),
    case(
        "Elena is Marco\u{2019}s sister.",
        &[("elena", "sister_of", "marco", A)],
    ),
    case(
        "Bram's daughter Odile arrived late.",
        &[("odile", "daughter_of", "bram", A)],
    ),
    case(
        "Odette\u{2019}s brother Lucian waved from the car.",
        &[("lucian", "brother_of", "odette", A)],
    ),
    case(
        "Dev, Marco's younger brother, left early.",
        &[("dev", "brother_of", "marco", A)],
    ),
    case(
        "Hollis, the father of Cass, called twice.",
        &[("hollis", "father_of", "cass", A)],
    ),
    case("Our neighbor Greta brought soup.", &[]),
    case(
        "Marco's sister Elena's husband waved.",
        &[("elena", "sister_of", "marco", A)],
    ),
    case(
        "Cass's mother Nora opened the door.",
        &[("nora", "mother_of", "cass", A)],
    ),
    case(
        "Marco's stepdaughter Iris hid in the pantry.",
        &[("iris", "stepdaughter_of", "marco", A)],
    ),
    case(
        "Nora and Marco are siblings.",
        &[("nora", "sibling_of", "marco", A)],
    ),
    case(
        "Wren and Cass are not sisters.",
        &[("wren", "sister_of", "cass", D)],
    ),
    case(
        "Dev and Elena were friends for years.",
        &[("dev", "friend_of", "elena", A)],
    ),
    case(
        "Ines is Dev's ex-wife.",
        &[("ines", "ex_wife_of", "dev", A)],
    ),
    case(
        "Marco's mother-in-law Teresa runs the house.",
        &[("teresa", "mother_in_law_of", "marco", A)],
    ),
    // Romance and marriage.
    case(
        "Nora married Dev in June.",
        &[("nora", "spouse_of", "dev", A)],
    ),
    case(
        "Elena never married Marco.",
        &[("elena", "spouse_of", "marco", D)],
    ),
    case(
        "Cass fell in love with Hollis that summer.",
        &[("cass", "loves", "hollis", A)],
    ),
    case(
        "Wren was dating Soren by then.",
        &[("wren", "dating", "soren", A)],
    ),
    case(
        "Marco proposed to Elena on the pier.",
        &[("marco", "proposed_to", "elena", A)],
    ),
    case(
        "Nora is engaged to Dev.",
        &[("nora", "engaged_to", "dev", A)],
    ),
    case(
        "Dev divorced Ines years ago.",
        &[("dev", "divorced", "ines", A)],
    ),
    case(
        "Marco kissed Elena before the train left.",
        &[("marco", "kissed", "elena", A)],
    ),
    // Friendship, acquaintance, enmity.
    case(
        "Dev befriended Elena at college.",
        &[("dev", "friend_of", "elena", A)],
    ),
    case(
        "Marco is friends with Elena.",
        &[("marco", "friend_of", "elena", A)],
    ),
    case(
        "Nora met Marco at the station.",
        &[("nora", "meets", "marco", A)],
    ),
    case(
        "Dev knows Elena from school.",
        &[("dev", "knows", "elena", A)],
    ),
    case(
        "Cass hated Hollis for a decade.",
        &[("cass", "hates", "hollis", A)],
    ),
    case(
        "Marco argued with Dev all night.",
        &[("marco", "argues_with", "dev", A)],
    ),
    case("Dev betrayed Marco.", &[("dev", "betrayed", "marco", A)]),
    // Work and study.
    case(
        "Dev works at the Archive.",
        &[("dev", "works_at", "archive", A)],
    ),
    case(
        "Nora worked for Marco.",
        &[("nora", "works_for", "marco", A)],
    ),
    case(
        "Marco hired Elena last spring.",
        &[("marco", "employs", "elena", A)],
    ),
    case(
        "Elena was hired by Marco.",
        &[("marco", "employs", "elena", A)],
    ),
    case(
        "Marco taught Elena for two years.",
        &[("marco", "teacher_of", "elena", A)],
    ),
    case(
        "Dev graduated from Alder College.",
        &[("dev", "attends", "alder_college", A)],
    ),
    case(
        "Elena studied at Durham.",
        &[("elena", "attends", "durham", A)],
    ),
    // Ownership, membership, leadership.
    case(
        "Nora owns the Hollow Alder Diner.",
        &[("nora", "owns", "hollow_alder_diner", A)],
    ),
    case(
        "Marco bought Bluebell Farm.",
        &[("marco", "owns", "bluebell_farm", A)],
    ),
    case(
        "Elena joined the Riverside Book Club.",
        &[("elena", "member_of", "riverside_book_club", A)],
    ),
    case(
        "Dev runs the Corner Diner.",
        &[("dev", "runs", "corner_diner", A)],
    ),
    case(
        "Nora founded Brightline Press.",
        &[("nora", "founded", "brightline_press", A)],
    ),
    // Residence and place.
    case("Nora lives in Tulsa.", &[("nora", "lives_in", "tulsa", A)]),
    case(
        "Marco moved to Chicago.",
        &[("marco", "lives_in", "chicago", A)],
    ),
    case(
        "Elena grew up in Tuscany.",
        &[("elena", "lives_in", "tuscany", A)],
    ),
    case("Dev was born in Lagos.", &[("dev", "born_in", "lagos", A)]),
    case(
        "Wren lives with Soren.",
        &[("wren", "lives_with", "soren", A)],
    ),
    case(
        "Nora lived next door to Marco.",
        &[("nora", "neighbor_of", "marco", A)],
    ),
    case(
        "Union Station is in Chicago.",
        &[("union_station", "located_in", "chicago", A)],
    ),
    case(
        "The Hollow Alder Diner is in Bakersfield.",
        &[("hollow_alder_diner", "located_in", "bakersfield", A)],
    ),
    // Relative clauses and chains.
    case(
        "Marco mentors Dev, who works at the Archive.",
        &[
            ("marco", "mentors", "dev", A),
            ("dev", "works_at", "archive", A),
        ],
    ),
    case(
        "Elena met Dev, who married Nora.",
        &[
            ("elena", "meets", "dev", A),
            ("dev", "spouse_of", "nora", A),
        ],
    ),
    // Spoken claims stated whole inside one quotation are kept.
    case(
        "\"Elena is Marco's sister,\" Dev said.",
        &[("elena", "sister_of", "marco", A)],
    ),
    // Negatives: dialogue fragments crossing an attribution.
    case("\"Papa's stuck at work,\" Nora says.", &[]),
    case(
        "\"Thanks, Cass,\" Wren says.",
        &[("wren", "speaks_to", "cass", A)],
    ),
    case("\"I met Dev,\" Nora tells Marco.", &[]),
    case("\"Marco,\" Elena said, \"married Nora.\"", &[]),
    case("Cass said, \"He married Dev.\"", &[]),
    case("\"Wren lives,\" Marco says, \"with Soren.\"", &[]),
    // Negatives: a third party's possessive, a common noun, a pronoun alone.
    case("Elena visited Marco's sister.", &[]),
    case("Nora met Marco's brother at the station.", &[]),
    case("Dev lived in Marco's house.", &[]),
    case("The doctor married the nurse.", &[]),
    case("She married him.", &[]),
    case("Nora met the Garrisons at dinner.", &[]),
    case("Nora loves Paris.", &[]),
    case("Dev owns Marco.", &[]),
    case("Marco joined Elena at the table.", &[]),
    case(
        "Elena visited Dev's sister Ines.",
        &[("ines", "sister_of", "dev", A)],
    ),
    // Negatives: modal, conditional, desire, question.
    case("Dev wanted to marry Elena.", &[]),
    case("If Nora married Dev, she would move to Chicago.", &[]),
    case("Nora would marry Dev.", &[]),
    case("Did Marco marry Elena?", &[]),
    case("Wren asked Cass if she loved Dev.", &[]),
    case("Elena hoped to live with Marco.", &[]),
    case("Marco refused to hire Dev.", &[]),
    // Negatives: verbs with other content between the mentions.
    case("Nora met her friend Marco.", &[]),
    case(
        "Nora said Marco married Elena.",
        &[("marco", "spouse_of", "elena", A)],
    ),
    case(
        "Nora kissed Dev and Marco hugged Elena.",
        &[
            ("nora", "kissed", "dev", A),
            ("marco", "comforts", "elena", A),
        ],
    ),
    case("Marco told Nora about Elena.", &[]),
    case("Nora, Dev and Marco are friends.", &[]),
    case("Marco, Elena and Dev went home.", &[]),
    case("Cass walked past Mrs. Chen and Dr. Quill.", &[]),
    case("Exhibit six: Mrs. Ashby's affidavit.", &[]),
];

/// Longer, dialogue-heavy paragraphs written after the rules, as a check that
/// the first set was not simply fitted. Most state no relation at all.
const HELD_OUT: &[Case] = &[
    case("Nora stirs the soup while Marco reads the paper. \"You're quiet,\" she says. He shrugs and turns a page.", &[]),
    case("Dev has worked at the cannery since he was nineteen. Last winter Ines Ward, his sister, took the night shift beside him.", &[]),
    case("Ines Ward, Dev's sister, took the night shift at the cannery.", &[("ines_ward", "sister_of", "dev", A)]),
    case("Cass grew up in Tulsa. She still hears the highway when she closes her eyes.", &[("cass", "lives_in", "tulsa", A)]),
    case("\"Did you hear?\" Marco asks. \"Elena married Bram in secret.\" Dev laughs and pours more wine.", &[("elena", "spouse_of", "bram", A)]),
    case("Elena and Bram are married now, though nobody at the Hollow Alder Diner believes it.", &[("elena", "spouse_of", "bram", A)]),
    case("Marco leans against the counter, watching Dev count the till. Neither speaks. Outside, Ines parks and kills the engine.", &[]),
    case("Mrs. Varga lives next door to Dr. Quill and waters his roses when he travels.", &[("mrs_varga", "neighbor_of", "dr_quill", A)]),
    case("Bram said that Elena once dated Marco, which explains the silence at dinner.", &[("elena", "dating", "marco", A)]),
    case("Wren glances at her homework. \"Is Soren coming?\" \"He's on his way,\" Cass says, and sets another plate.", &[]),
    case("Hollis owns the Lantern Hospice building, and Chen runs the front desk.", &[("hollis", "owns", "lantern_hospice", A)]),
    case("Pryor joined the Maple Street Church choir last autumn.", &[("pryor", "member_of", "maple_street_church", A)]),
    case("Judge Voss knew Fenn from law school, and neither of them mentioned it.", &[("judge_voss", "knows", "fenn", A)]),
    case("Odile takes Wren's hand and leads her down the hall to Cass's room.", &[]),
    case("Cass telephoned Hollis from the porch, then called her mother.", &[("cass", "contacts", "hollis", A)]),
    case("Gideon told Hollis he had never met Chen, and Hollis believed him.", &[]),
    case("Odette taught Cass for a term, and Cass never forgot it.", &[("odette", "teacher_of", "cass", A)]),
    case("The Archive sits on Dover Street. Dev walks past it each morning.", &[]),
    case("Marco's wife Elena teaches at Alder College.", &[("elena", "wife_of", "marco", A), ("elena", "works_at", "alder_college", A)]),
    case("Tamsin lives in the apartment above Wren.", &[]),
    case("Wren Hart never forgave Soren, and Soren never asked her to.", &[]),
    case("Nora is not Dev's mother, whatever the neighbors think.", &[("nora", "mother_of", "dev", D)]),
    case("After the funeral Elena moved to Chicago, and Marco followed her a year later.", &[("elena", "lives_in", "chicago", A)]),
    case("Cass stands beside Wren at the window, watching Hollis carry the boxes.", &[]),
    case("Dev says Nora is his cousin, but he says a lot of things.", &[]),
    case("Mr. Marlow's daughter never visits, Mrs. Ashby tells Cass.", &[]),
    case("Ines Ward divorced Bram in 2019 and moved back to Tulsa.", &[("ines_ward", "divorced", "bram", A), ("ines_ward", "lives_in", "tulsa", A)]),
    case("Elena, who works for Marco, has never met Dev.", &[("elena", "works_for", "marco", A), ("elena", "meets", "dev", D)]),
    case("Nora hugged Cass at the door and kissed Wren on the forehead.", &[("nora", "comforts", "cass", A)]),
    case("Dev and Bram are old friends who share a boat in Seattle.", &[("dev", "friend_of", "bram", A)]),
];

/// A first-person narrator named `wren`, with paragraphs that give the names
/// their gender before extraction, as a caller sharing one context would.
struct Book {
    learn: &'static [&'static str],
    cases: &'static [Case],
}

const BOOK: Book = Book {
    learn: &[
        "Later Nora said she was tired. Then Nora crossed her arms.",
        "Soon Cass smiled and she nodded. Then Hollis said he would drive.",
        "Then Marco said he was late. Later Elena said she knew.",
    ],
    cases: &[
        case(
            "My daughter Cass laughed.",
            &[("cass", "daughter_of", "wren", A)],
        ),
        case(
            "Cass, my sister, called.",
            &[("cass", "sister_of", "wren", A)],
        ),
        case("Nora is my mother.", &[("nora", "mother_of", "wren", A)]),
        case(
            "Our neighbor Greta waved.",
            &[("greta", "neighbor_of", "wren", A)],
        ),
        case(
            "I met Marco at the station.",
            &[("wren", "meets", "marco", A)],
        ),
        case(
            "I live with Cass now.",
            &[("wren", "lives_with", "cass", A)],
        ),
        case(
            "Nora opened the door. She married Marco.",
            &[("nora", "spouse_of", "marco", A)],
        ),
        case(
            "Marco's sister Elena called. She works at the Archive.",
            &[
                ("elena", "sister_of", "marco", A),
                ("elena", "works_at", "archive", A),
            ],
        ),
        case(
            "Hollis said his sister Cass was late.",
            &[("cass", "sister_of", "hollis", A)],
        ),
        case("Elena told Nora her sister Cass was late.", &[]),
        case("\"I met Marco,\" I say.", &[]),
        case("\"My mother Nora lives here.\"", &[]),
        case("She married him.", &[]),
    ],
};

fn score(label: &str, cases: &[(&Case, BTreeSet<Claim>)]) -> (usize, usize, usize) {
    let (mut tp, mut fp, mut fn_) = (0, 0, 0);
    for (case, found) in cases {
        let gold: BTreeSet<Claim> = case.gold.iter().copied().collect();
        for claim in found {
            if gold.contains(claim) {
                tp += 1;
            } else {
                fp += 1;
                println!("[{label}] FALSE POSITIVE {claim:?} <- {:?}", case.text);
            }
        }
        for claim in &gold {
            if !found.contains(claim) {
                fn_ += 1;
                println!("[{label}] MISS {claim:?} <- {:?}", case.text);
            }
        }
    }
    (tp, fp, fn_)
}

fn extract(text: &str, context: &Context) -> BTreeSet<Claim> {
    extract_aggregates_in(text, &Options::default(), context)
        .expect("extraction failed")
        .into_iter()
        .map(|a| {
            (
                leak(a.subject),
                leak(a.relation),
                leak(a.object),
                a.polarity,
            )
        })
        .collect()
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn report(label: &str, runs: &[(&Case, BTreeSet<Claim>)]) -> (f64, f64) {
    let (tp, fp, fn_) = score(label, runs);
    let precision = tp as f64 / (tp + fp).max(1) as f64;
    let recall = tp as f64 / (tp + fn_).max(1) as f64;
    println!(
        "{label}: paragraphs={} gold={} tp={tp} fp={fp} fn={fn_} precision={precision:.3} recall={recall:.3}",
        runs.len(),
        tp + fn_
    );
    (precision, recall)
}

#[test]
fn synthetic_set_meets_precision_and_recall_floors() {
    let alone = Context::default();
    let development: Vec<_> = CASES.iter().map(|c| (c, extract(c.text, &alone))).collect();
    let held_out: Vec<_> = HELD_OUT
        .iter()
        .map(|c| (c, extract(c.text, &alone)))
        .collect();

    let mut context = Context::default().with_narrator("wren");
    for text in BOOK.learn {
        context.learn(text);
    }
    let book: Vec<_> = BOOK
        .cases
        .iter()
        .map(|c| (c, extract(c.text, &context)))
        .collect();

    report("development", &development);
    report("held-out", &held_out);
    report("narrator-context", &book);
    let all: Vec<_> = development
        .into_iter()
        .chain(held_out)
        .chain(book)
        .collect();
    let (precision, recall) = report("all", &all);
    assert!(precision >= 0.95, "precision {precision:.3}");
    assert!(recall >= 0.85, "recall {recall:.3}");
}
