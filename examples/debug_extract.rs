fn main() {
    let text = "Elena is Marco's sister. Marco mentors Dev, who works at the Archive.";

    // Manual sentence split
    let sentences: Vec<&str> = text.split(". ").collect();
    println!("Sentences: {:?}", sentences);

    for (i, sent) in sentences.iter().enumerate() {
        println!("\nSentence {}:{:?}", i, sent);

        // Find capitalized words
        let words: Vec<&str> = sent.split_whitespace().collect();
        println!("  Words: {:?}", words);

        for word in words.iter() {
            let word_clean = word.trim_end_matches(|c: char| !c.is_alphanumeric());
            let is_cap = word_clean.chars().next().is_some_and(|c| c.is_uppercase());
            println!("    {:20} clean={:20} cap={}", word, word_clean, is_cap);
        }
    }
}
