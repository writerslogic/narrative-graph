use narrative_graph::{extract_candidate_triples, Options};

fn main() -> anyhow::Result<()> {
    // Example from the README
    let text = "Elena is Marco's sister. Marco mentors Dev, who works at the Archive.";

    let opts = Options::default();
    let candidates = extract_candidate_triples(text, &opts)?;

    println!("Extracted triples from: {:?}\n", text);
    println!("Total candidates: {}\n", candidates.len());
    if candidates.is_empty() {
        println!("No candidates extracted!");
        return Ok(());
    }
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

    // This output can be fed to holographic-memory's memorizeTriplet:
    // for candidate in candidates {
    //     memory.memorize_triplet(&candidate.subject, &candidate.relation, &candidate.object)?;
    // }

    Ok(())
}
