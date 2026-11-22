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
