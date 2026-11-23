"""Sample training windows from the six public-domain novels for the teacher.

    uv run --no-project tests/corpus/tools/sample_training.py <out.txt> [--per-book 334]

Writes items in the labels.txt format (the guide header, then `@`, `~`, the
sentence and an empty `=` block) for `teacher.py --items`. A sentence is
eligible when it names two people, holds speech, or has a relation word, and
never when the window the teacher reads for it (6000 bytes before it, through
its end) overlaps the window of an evaluated item, so no labelled sentence or
its context reaches training. Fixed seed.
"""

import argparse
import logging
import random
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BOOKS = ["1342", "1260", "98", "2701", "768", "158"]
WINDOW = 6000
STOP = set("I Mr Mrs Miss Sir Lady Lord God Captain Dr Colonel Madame Monsieur Mademoiselle Master Heaven Oh O Ah Yes No Well The A".split())
REL = re.compile(
    r"\b(sister|brother|mother|father|daughter|son|wife|husband|aunt|uncle|niece|nephew|cousin|widow|friend|"
    r"servant|master|mistress|guardian|engaged|married|lover|enemy|governess|pupil|landlord|tenant|maid)s?\b",
    re.I,
)
END = re.compile(
    r"(?<!\bMr)(?<!\bMrs)(?<!\bDr)(?<!\bSt)(?<!\bMessrs)(?<!\bMme)(?<!\bMlle)(?<!\b[A-Z])[.!?][\"”’)]*\s+(?=[\"“‘(]?[A-Z])"
)


def sentences(text):
    for para in re.finditer(r"(?:[^\n]+\n?)+", text):
        p, base, start = para.group(0), para.start(), 0
        for m in END.finditer(p):
            yield base + start, p[start : m.end()].rstrip()
            start = m.end()
        tail = p[start:].rstrip()
        if tail:
            yield base + start, tail


def names(s):
    return {t for t in re.findall(r"(?<![.!?] )(?<!^)\b[A-Z][a-z]+", s) if t not in STOP}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out", type=Path)
    parser.add_argument("--per-book", type=int, default=334)
    args = parser.parse_args()

    labels = (ROOT / "labels.txt").read_text(encoding="utf-8")
    header = labels.split("\n@ ", 1)[0]
    evaluated = {}
    for m in re.finditer(r"^@ \d+ (\d+) (\d+) .*\n~ .*\n(.*)\n", labels, re.M):
        start = int(m.group(2))
        evaluated.setdefault(m.group(1), []).append((start - WINDOW, start + len(m.group(3).encode())))

    rng = random.Random(20261123)
    items = []
    for book in BOOKS:
        text = (ROOT / f"{book}.txt").read_text(encoding="utf-8")
        sents = list(sentences(text))
        eligible = []
        byte_at, done = 0, 0
        for i, (offset, s) in enumerate(sents):
            byte_at += len(text[done:offset].encode())
            done = offset
            if i == 0 or not 40 <= len(s) <= 350 or s.isupper() or re.match(r"\s*(CHAPTER|Chapter|VOLUME)", s):
                continue
            if not (len(names(s)) >= 2 or re.search(r"[“\"‘]", s) or (REL.search(s) and names(s))):
                continue
            lo, hi = byte_at - WINDOW, byte_at + len(s.encode())
            if any(lo < e_hi and e_lo < hi for e_lo, e_hi in evaluated.get(book, [])):
                continue
            eligible.append((i, byte_at))
        for i, byte_at in sorted(rng.sample(eligible, min(args.per_book, len(eligible)))):
            s = sents[i][1]
            assert text.encode()[byte_at : byte_at + len(s.encode())].decode() == s
            items.append((book, byte_at, " ".join(sents[i - 1][1].split()), " ".join(s.split())))

    body = "".join(
        f"@ {n} {book} {start} training | {book}\n~ {prev}\n{s}\n=\n\n"
        for n, (book, start, prev, s) in enumerate(items, 1001)
    )
    args.out.write_text(header + "\n" + body, encoding="utf-8")
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    logging.info("%d training items", len(items))


if __name__ == "__main__":
    main()
