use anyhow::Context;
use clap::{Parser, ValueEnum};
use narrative_graph::{extract_candidate_triples, Options};
use std::fs;
use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;

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
        input: PathBuf,

        /// Minimum confidence threshold (0.0-1.0)
        #[arg(long, default_value = "0.0")]
        min_confidence: f32,

        /// Output format
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    Json,
    Text,
}

fn main() -> anyhow::Result<()> {
    let Command::Extract {
        input,
        min_confidence,
        format,
    } = Args::parse().command;

    let text = if input.as_os_str() == "-" {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("failed to read stdin")?;
        buffer
    } else {
        fs::read_to_string(&input).with_context(|| format!("failed to read {}", input.display()))?
    };

    let opts = Options {
        aliases: Default::default(),
        min_confidence: Some(min_confidence),
        rejections: Default::default(),
        ontology: Default::default(),
    };
    let candidates = extract_candidate_triples(&text, &opts)?;

    match write_candidates(&candidates, format) {
        // A closed pipe (`| head`) is the reader finishing, not a failure.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other.context("failed to write output"),
    }
}

fn write_candidates(
    candidates: &[narrative_graph::TripleCandidate],
    format: Format,
) -> io::Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    match format {
        Format::Json => {
            serde_json::to_writer_pretty(&mut out, candidates)?;
            writeln!(out)?;
        }
        Format::Text => {
            for c in candidates {
                writeln!(
                    out,
                    "{} --{}--> {} {:.2} [{}..{}]",
                    c.subject, c.relation, c.object, c.confidence, c.span[0], c.span[1]
                )?;
            }
        }
    }
    out.flush()
}
