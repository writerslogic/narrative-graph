use narrative_graph::{extract_candidate_triples, Options, Result};

fn main() -> Result<()> {
    // Test simple cases first
    println!("=== Test 1: Simple possessive ===");
    let text1 = "Elena is Marco's sister.";
    let opts = Options::default();
    let cands1 = extract_candidate_triples(text1, &opts)?;
    println!("Input: {:?}", text1);
    println!("Found {} candidates:", cands1.len());
    for c in cands1 {
        println!("  {} --{}-> {}", c.subject, c.relation, c.object);
    }

    println!("\n=== Test 2: Simple verb ===");
    let text2 = "Marco mentors Dev.";
    let cands2 = extract_candidate_triples(text2, &opts)?;
    println!("Input: {:?}", text2);
    println!("Found {} candidates:", cands2.len());
    for c in cands2 {
        println!("  {} --{}-> {}", c.subject, c.relation, c.object);
    }

    // Example from the README
    println!("\n=== Test 3: README example ===");
    let text = "Elena is Marco's sister. Marco mentors Dev, who works at the Archive.";
    let candidates = extract_candidate_triples(text, &opts)?;

    println!("Input: {:?}\n", text);
    println!("Found {} candidates:\n", candidates.len());
    for candidate in candidates {
        println!(
            "{:20} --{:15}--> {:20} {:.2}  [{:3}..{:3}]",
            candidate.subject,
            candidate.relation,
            candidate.object,
            candidate.confidence,
            candidate.span[0],
            candidate.span[1]
        );
    }

    Ok(())
}
