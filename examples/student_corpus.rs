use anyhow::{ensure, Context, Result};
use narrative_graph::heuristic::model::{Input, InputTokenizer};
use narrative_graph::{extract_aggregates, Options, Polarity, StudentModel};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Book {
    path: PathBuf,
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    number: u32,
    span: [usize; 2],
}

#[derive(Serialize)]
struct TrainingInput {
    number: u32,
    #[serde(flatten)]
    input: Input,
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [command, path] = args.as_slice() else {
        anyhow::bail!(
            "expected `inputs TOKENIZER`, `predict CHECKPOINT` or `predict-with-rules CHECKPOINT`"
        );
    };
    let books: Vec<Book> = serde_json::from_reader(io::stdin().lock())?;
    let mut out = BufWriter::new(io::stdout().lock());
    let opts = Options::default();
    match command.as_str() {
        "inputs" => {
            let tokenizer = InputTokenizer::load(Path::new(path))?;
            for book in books {
                let text = read_book(&book.path)?;
                let spans: Vec<_> = book.items.iter().map(|item| item.span).collect();
                for (item, input) in book
                    .items
                    .into_iter()
                    .zip(tokenizer.prepare(&text, &spans, &opts)?)
                {
                    serde_json::to_writer(
                        &mut out,
                        &TrainingInput {
                            number: item.number,
                            input,
                        },
                    )?;
                    writeln!(out)?;
                }
            }
        }
        "predict" | "predict-with-rules" => {
            let model = StudentModel::load(path)?;
            for book in books {
                let text = read_book(&book.path)?;
                let numbers: BTreeMap<_, _> = book
                    .items
                    .iter()
                    .map(|item| (item.span, item.number))
                    .collect();
                ensure!(numbers.len() == book.items.len(), "duplicate sentence span");
                let spans: Vec<_> = numbers.keys().copied().collect();
                let mut facts = model.predict_spans(&text, &spans, &opts)?;
                if command == "predict-with-rules" {
                    facts.extend(extract_aggregates(&text, &opts)?);
                }
                let mut predictions = BTreeSet::new();
                for fact in &facts {
                    for &[offset, _] in &fact.spans {
                        let target = numbers.range(..=[offset, usize::MAX]).next_back();
                        if let Some((_, number)) = target.filter(|(span, _)| offset < span[1]) {
                            predictions.insert((
                                number,
                                &fact.subject,
                                &fact.relation,
                                &fact.object,
                                fact.polarity,
                            ));
                        }
                    }
                }
                for (number, subject, relation, object, polarity) in predictions {
                    let prefix = if polarity == Polarity::Denied {
                        "not "
                    } else {
                        ""
                    };
                    writeln!(out, "{number} | {prefix}{subject} | {relation} | {object}")?;
                }
            }
        }
        _ => anyhow::bail!("unknown student corpus command {command}"),
    }
    out.flush()?;
    Ok(())
}

fn read_book(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("read {}", path.display()))
}
