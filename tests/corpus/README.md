# Corpus

Public-domain novels used by `tests/corpus.rs` (whole-book output snapshots)
and `tests/real_text_eval.rs` (hand-labelled precision and recall). Line
endings are LF and the distributor's header, footer and licence text are
removed, so these files are the novels' text only.

| File | Title | Author |
|---|---|---|
| 1342.txt | Pride and Prejudice | Jane Austen |
| 1260.txt | Jane Eyre | Charlotte Brontë |
| 98.txt | A Tale of Two Cities | Charles Dickens |
| 2701.txt | Moby-Dick | Herman Melville |
| 768.txt | Wuthering Heights | Emily Brontë |
| 158.txt | Emma | Jane Austen |

The test files key labels by byte offset into these files, so editing a
novel means regenerating `snapshots/` and re-checking `labels.tsv`.

## Local student

`sample_training.py` excludes windows that overlap either evaluation half.
`teacher.py --items` verifies the sentence, its preceding sentence, and its
byte offset against the public novels. Batch collection writes the claims and
the raw answers, including each explicit `none` used as a training negative.

Train from the sample and raw answers in the batch directory. The trainer
holds out the last fifth of each novel's sampled windows for validation and
drops training windows that overlap that split. It downloads the pinned
DeBERTa-v3-small base weights on first use and trains locally with PyTorch.

```bash
student_run=/path/to/teacher-batch
cargo build --release --features model --example student_corpus
uv run tests/corpus/tools/train_student.py train \
  "$student_run/training-items.txt" "$student_run/teacher-labels.txt.raw.txt" \
  "$student_run/deberta-v3-small"
```

The `model` feature loads `model.onnx`, `spm.model`, and `student.json`
locally and uses the same candidate names and tokenization used in training.
The checkpoint keeps `model.safetensors` and `config.json` for re-export.

```bash
uv run tests/corpus/tools/train_student.py predict \
  "$student_run/deberta-v3-small" "$student_run/even.predictions.txt" --half even --with-rules
NG_HALF=even NG_PREDICTIONS="$student_run/even.predictions.txt" \
  cargo test --release --test real_text_eval real_text_precision_and_recall \
  -- --include-ignored --nocapture
```
