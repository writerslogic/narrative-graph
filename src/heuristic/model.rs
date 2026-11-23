//! Optional, local DeBERTa relation extraction using a fine-tuned checkpoint.

mod input;

use super::{aggregate, collect_candidates, dedup_candidates, normalize_relation, pack::ENGLISH};
use super::{segment, Context, Emitted};
use crate::{
    AggregateTriple, Language, NarrativeGraphError, Options, Polarity, RelationshipState,
    TripleCandidate,
};
use anyhow::{ensure, Context as _, Result};
use input::{MAX_NAMES, MAX_TOKENS};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use tract_onnx::prelude::{
    tvec, DatumType, Framework, InferenceModelExt, IntoRunnable, Tensor, TypedRunnableModel,
};

#[doc(hidden)]
pub use input::{Input, InputTokenizer};

const BATCH: usize = 2;

#[derive(Deserialize)]
struct Manifest {
    version: u32,
    labels: Vec<String>,
    threshold: f32,
}

struct Label {
    relation: String,
    polarity: Polarity,
}

/// A local student checkpoint for English prose. Load once and reuse across documents.
pub struct StudentModel {
    tokenizer: InputTokenizer,
    network: Arc<TypedRunnableModel>,
    labels: Vec<Label>,
    threshold: f32,
}

impl StudentModel {
    /// Load `student.json`, `spm.model` and the ONNX graph exported by the training script.
    pub fn load(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref();
        let manifest = read_artifact(&directory.join("student.json"), 64 << 10)?;
        let manifest: Manifest =
            serde_json::from_slice(&manifest).context("invalid student.json")?;
        let labels = manifest.labels()?;
        let tokenizer = InputTokenizer::load(&directory.join("spm.model"))?;
        let mut artifact = open_artifact(&directory.join("model.onnx"), 640 << 20)?;
        let network = tract_onnx::onnx()
            .model_for_read(&mut artifact)?
            .into_optimized()?;
        let shapes: [&[usize]; 4] = [
            &[BATCH, MAX_TOKENS],
            &[BATCH, MAX_TOKENS],
            &[BATCH, MAX_NAMES],
            &[BATCH],
        ];
        ensure!(
            network.input_outlets()?.len() == shapes.len() && network.output_outlets()?.len() == 1,
            "invalid student inputs or output"
        );
        for (index, shape) in shapes.into_iter().enumerate() {
            let fact = network.input_fact(index)?;
            ensure!(
                fact.datum_type == DatumType::I64 && fact.shape.as_concrete() == Some(shape),
                "invalid student input {index}"
            );
        }
        let output = network.output_fact(0)?;
        ensure!(
            output.datum_type == DatumType::F32
                && output.shape.as_concrete()
                    == Some(&[BATCH, MAX_NAMES, MAX_NAMES, labels.len() + 1]),
            "invalid student scores shape"
        );
        Ok(Self {
            tokenizer,
            network: network.into_runnable()?,
            labels,
            threshold: manifest.threshold,
        })
    }

    /// Extract asserted candidates from the rules and the student.
    pub fn extract_candidate_triples(
        &self,
        text: &str,
        opts: &Options,
    ) -> Result<Vec<TripleCandidate>> {
        let mut candidates = self
            .extract(text, opts)?
            .into_iter()
            .filter(|e| e.polarity == Polarity::Asserted)
            .map(|e| e.candidate)
            .collect();
        dedup_candidates(&mut candidates);
        Ok(candidates)
    }

    /// Extract asserted and denied aggregates from the rules and the student.
    pub fn extract_aggregates(&self, text: &str, opts: &Options) -> Result<Vec<AggregateTriple>> {
        let (mut facts, _) = aggregate::aggregate(self.extract(text, opts)?);
        for fact in &mut facts {
            fact.spans.dedup();
        }
        Ok(facts)
    }

    fn extract(&self, text: &str, opts: &Options) -> Result<Vec<Emitted>> {
        let spans: Vec<_> = segment::split_sentences(text, &ENGLISH)
            .into_iter()
            .map(|(sentence, start)| [start, start + sentence.len()])
            .collect();
        let predicted = self.predict(text, &spans, opts)?;
        let mut emitted = collect_candidates(text, opts, &Context::default(), None)?;
        emitted.extend(predicted);
        emitted.sort_by_key(|e| e.candidate.span);
        Ok(emitted)
    }

    /// Score only the requested sentence byte ranges, with document context.
    pub fn predict_spans(
        &self,
        text: &str,
        spans: &[[usize; 2]],
        opts: &Options,
    ) -> Result<Vec<AggregateTriple>> {
        let mut emitted = self.predict(text, spans, opts)?;
        emitted.sort_by_key(|e| e.candidate.span);
        let (facts, _) = aggregate::aggregate(emitted);
        Ok(facts)
    }

    fn predict(&self, text: &str, spans: &[[usize; 2]], opts: &Options) -> Result<Vec<Emitted>> {
        ensure!(
            opts.language == Language::English,
            "the local student requires English text"
        );
        let threshold = opts.min_confidence.unwrap_or(0.0);
        if !(0.0..=1.0).contains(&threshold) {
            return Err(NarrativeGraphError::InvalidConfidenceThreshold(threshold).into());
        }
        let threshold = threshold.max(self.threshold);
        let rejected: BTreeSet<_> = opts
            .rejections
            .iter()
            .map(|r| (r.subject.as_str(), r.relation.as_str(), r.object.as_str()))
            .collect();
        let mut emitted = Vec::new();
        self.tokenizer
            .for_batches(text, spans, opts, BATCH, |inputs| {
                let inputs: Vec<_> = inputs
                    .iter()
                    .filter(|input| input.names.len() >= 2)
                    .collect();
                if inputs.is_empty() {
                    return Ok(());
                }
                let scores = self.score(&inputs)?;
                let stride = self.labels.len() + 1;
                let windows = scores.chunks_exact(MAX_NAMES * MAX_NAMES * stride);
                for (input, scores) in inputs.into_iter().zip(windows) {
                    let mut pairs = Vec::new();
                    for (subject_index, subject) in input.names.iter().enumerate() {
                        for (object_index, object) in input.names.iter().enumerate() {
                            if subject_index != object_index {
                                let offset = (subject_index * MAX_NAMES + object_index) * stride;
                                let row = &scores[offset..offset + stride];
                                pairs.push((subject, object, row));
                            }
                        }
                    }
                    // The student compares each pair to a no-relation choice with logit zero.
                    let links = pairs
                        .iter()
                        .map(|(_, _, row)| row[0])
                        .chain(std::iter::once(0.0));
                    let (link_max, link_sum) = softmax_scale(links)?;
                    for (subject, object, row) in pairs {
                        let link = (row[0] - link_max).exp() / link_sum;
                        if link < threshold {
                            continue;
                        }
                        let (relation_max, relation_sum) = softmax_scale(row[1..].iter().copied())?;
                        for (label, &score) in self.labels.iter().zip(&row[1..]) {
                            let confidence = link * (score - relation_max).exp() / relation_sum;
                            if confidence < threshold {
                                continue;
                            }
                            let relation = normalize_relation(&label.relation, &opts.ontology);
                            if rejected.contains(&(
                                subject.as_str(),
                                relation.as_str(),
                                object.as_str(),
                            )) {
                                continue;
                            }
                            emitted.push(Emitted {
                                candidate: TripleCandidate {
                                    subject: subject.clone(),
                                    relation,
                                    object: object.clone(),
                                    confidence,
                                    span: input.span,
                                    rule: "student-model".into(),
                                },
                                polarity: label.polarity,
                                single_filler: ENGLISH
                                    .single_filler_relations
                                    .contains(&label.relation.as_str()),
                                state: RelationshipState::from_relation(&label.relation),
                                ends_state: false,
                            });
                        }
                    }
                }
                Ok(())
            })?;
        Ok(emitted)
    }

    fn score(&self, inputs: &[&Input]) -> Result<Vec<f32>> {
        let mut ids = vec![i64::from(self.tokenizer.pad); BATCH * MAX_TOKENS];
        let mut mask = vec![0_i64; BATCH * MAX_TOKENS];
        let mut positions = vec![0_i64; BATCH * MAX_NAMES];
        let mut targets = vec![0_i64; BATCH];
        for (row, input) in inputs.iter().enumerate() {
            let base = row * MAX_TOKENS;
            for (slot, &id) in ids[base..].iter_mut().zip(&input.ids) {
                *slot = i64::from(id);
            }
            mask[base..base + input.ids.len()].fill(1);
            for (k, &position) in input.positions.iter().enumerate() {
                positions[row * MAX_NAMES + k] = i64::from(position);
            }
            targets[row] = i64::from(input.target);
        }
        let scores = self.network.run(tvec!(
            Tensor::from_shape(&[BATCH, MAX_TOKENS], &ids)?.into(),
            Tensor::from_shape(&[BATCH, MAX_TOKENS], &mask)?.into(),
            Tensor::from_shape(&[BATCH, MAX_NAMES], &positions)?.into(),
            Tensor::from_shape(&[BATCH], &targets)?.into(),
        ))?;
        Ok(scores[0]
            .to_plain_array_view::<f32>()?
            .iter()
            .copied()
            .collect())
    }
}

impl Manifest {
    fn labels(&self) -> Result<Vec<Label>> {
        ensure!(self.version == 2, "unsupported student checkpoint version");
        ensure!(
            self.threshold > 0.0 && self.threshold <= 1.0,
            "invalid student threshold"
        );
        ensure!(
            (1..=256).contains(&self.labels.len()),
            "invalid student label count"
        );
        let mut seen = BTreeSet::new();
        self.labels
            .iter()
            .map(|value| {
                let (relation, polarity) = value
                    .strip_prefix("not ")
                    .map_or((value.as_str(), Polarity::Asserted), |name| {
                        (name, Polarity::Denied)
                    });
                ensure!(
                    !relation.is_empty()
                        && relation.len() <= 64
                        && relation
                            .bytes()
                            .all(|c| c.is_ascii_lowercase() || c == b'_')
                        && seen.insert(value),
                    "invalid or duplicate student label"
                );
                Ok(Label {
                    relation: relation.into(),
                    polarity,
                })
            })
            .collect()
    }
}

fn softmax_scale(logits: impl Iterator<Item = f32> + Clone) -> Result<(f32, f32)> {
    let max = logits.clone().fold(f32::NEG_INFINITY, f32::max);
    let sum = logits.map(|logit| (logit - max).exp()).sum::<f32>();
    ensure!(sum.is_finite() && sum > 0.0, "non-finite student scores");
    Ok((max, sum))
}

fn read_artifact(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    open_artifact(path, limit)?.read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "{} exceeds the size limit",
        path.display()
    );
    Ok(bytes)
}

fn open_artifact(path: &Path, limit: u64) -> Result<impl Read> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let info = file.metadata()?;
    ensure!(
        info.is_file() && info.len() <= limit,
        "invalid size or type for {}",
        path.display()
    );
    Ok(file.take(limit + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestBundle(PathBuf);

    impl Drop for TestBundle {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn bundle() -> Result<TestBundle> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path =
            std::env::temp_dir().join(format!("narrative-student-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)?;
        let bundle = TestBundle(path);

        fs::write(
            bundle.0.join("spm.model"),
            include_bytes!("../../tests/fixtures/student/spm.model"),
        )?;
        fs::write(
            bundle.0.join("model.onnx"),
            include_bytes!("../../tests/fixtures/student/model.onnx"),
        )?;
        let manifest = r#"{"version":2,"labels":["admires","not helps"],"threshold":0.10}"#;
        fs::write(bundle.0.join("student.json"), manifest)?;
        Ok(bundle)
    }

    #[test]
    fn student_bundle_preserves_spans_polarity_and_options() -> Result<()> {
        let bundle = bundle()?;
        let model = StudentModel::load(&bundle.0)?;
        let chinese = Options {
            language: Language::Chinese,
            ..Options::default()
        };
        let error = model
            .extract_candidate_triples("賈寶玉", &chinese)
            .unwrap_err();
        assert_eq!(error.to_string(), "the local student requires English text");
        let text = "Elena is Marco’s sister. Marco thanks Elena.";
        let start = text.find("Marco thanks").unwrap();
        let span = [start, text.len()];
        let inside_apostrophe = text.find('’').unwrap() + 1;
        assert!(model
            .predict_spans(
                text,
                &[[inside_apostrophe, text.len()]],
                &Options::default()
            )
            .is_err());

        let confidence = 1.0 / 6.0;
        for (threshold, claims) in [
            (confidence - 0.001, 4),
            (confidence, 4),
            (confidence + 0.001, 0),
        ] {
            let opts = Options {
                min_confidence: Some(threshold),
                ..Options::default()
            };
            let facts = model.predict_spans(text, &[span], &opts)?;
            assert_eq!(facts.len(), claims);
            assert!(facts
                .iter()
                .all(|fact| fact.spans == [span] && fact.confidence == confidence));
        }

        let mut opts = Options::default();
        opts.ontology.insert("admires".into(), "respects".into());
        opts.rejections.push(crate::Rejection {
            subject: "elena".into(),
            relation: "respects".into(),
            object: "marco".into(),
        });
        let facts = model.extract_aggregates(text, &opts)?;
        assert!(facts.iter().any(|fact| fact.relation == "sister_of"));
        assert!(facts
            .iter()
            .any(|fact| fact.relation == "helps" && fact.polarity == Polarity::Denied));
        assert!(facts
            .iter()
            .any(|fact| fact.subject == "marco" && fact.relation == "respects"));
        assert!(!facts
            .iter()
            .any(|fact| fact.subject == "elena" && fact.relation == "respects"));
        assert!(!model
            .extract_candidate_triples(text, &opts)?
            .iter()
            .any(|fact| fact.relation == "helps"));
        Ok(())
    }
}
