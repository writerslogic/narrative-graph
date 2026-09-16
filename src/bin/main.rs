use clap::Parser;
use narrative_graph::{extract_candidate_triples, Options};
use std::fs;
use std::io::{self, Read};

#[derive(Parser, Debug)]
#[command(name = "narrative-graph")]
#[command(about = "Extract relational facts from prose")]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    /// Extract candidate triples from a file
    Extract {
        /// Input file (or - for stdin)
        #[arg(value_name = "FILE")]
        input: String,

        /// Minimum confidence threshold (0.0-1.0)
        #[arg(long, default_value = "0.0")]
        min_confidence: f32,

        /// Output format: json or text
        #[arg(long, default_value = "text")]
        format: String,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Extract {
            input,
            min_confidence,
            format,
        } => {
            // Read input
            let text = if input == "-" {
                let mut buffer = String::new();
                io::stdin().read_to_string(&mut buffer)?;
                buffer
            } else {
                fs::read_to_string(&input)?
            };

            // Extract
            let opts = Options {
                aliases: Default::default(),
                min_confidence: if min_confidence > 0.0 {
                    Some(min_confidence)
                } else {
                    None
                },
                cross_sentence_pronouns: false,
                rejections: Default::default(),
                ontology: Default::default(),
            };

            let candidates = extract_candidate_triples(&text, &opts)?;

            // Output
            match format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&candidates)?;
                    println!("{}", json);
                }
                _ => {
                    for candidate in candidates {
                        println!(
                            "{} --{}--> {} {:.2} [{}..{}]",
                            candidate.subject,
                            candidate.relation,
                            candidate.object,
                            candidate.confidence,
                            candidate.span[0],
                            candidate.span[1]
                        );
                    }
                }
            }

            Ok(())
        }
    }
}
