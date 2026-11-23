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
from itertools import pairwise
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


def sentence_windows(text):
    def byte_positions():
        byte_at, done = 0, 0
        for offset, sentence in sentences(text):
            byte_at += len(text[done:offset].encode())
            done = offset
            yield byte_at, sentence

    for (previous_at, previous), (start, sentence) in pairwise(byte_positions()):
        yield start, (min(start - WINDOW, previous_at), start + len(sentence.encode()), previous, sentence)


def overlaps(lo, hi, protected):
    return any(lo < end and start < hi for start, end in protected)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out", type=Path)
    parser.add_argument("--per-book", type=int, default=334)
    args = parser.parse_args()

    labels = (ROOT / "labels.txt").read_text(encoding="utf-8")
    header = labels.split("\n@ ", 1)[0]
    evaluated = {}
    for book, start in re.findall(r"^@ \d+ (\d+) (\d+) ", labels, re.M):
        evaluated.setdefault(book, []).append(int(start))

    rng = random.Random(20261123)
    items = []
    for book in BOOKS:
        text = (ROOT / f"{book}.txt").read_bytes().decode("utf-8")
        windows = dict(sentence_windows(text))
        protected = [windows[start][:2] for start in evaluated.get(book, [])]
        eligible = []
        for start, (lo, hi, _, s) in windows.items():
            if not 40 <= len(s) <= 350 or s.isupper() or re.match(r"\s*(CHAPTER|Chapter|VOLUME)", s):
                continue
            if not (len(names(s)) >= 2 or re.search(r"[“\"‘]", s) or (REL.search(s) and names(s))):
                continue
            if not overlaps(lo, hi, protected):
                eligible.append(start)
        for start in sorted(rng.sample(eligible, min(args.per_book, len(eligible)))):
            _, _, previous, sentence = windows[start]
            items.append((book, start, " ".join(previous.split()), " ".join(sentence.split())))

    body = "".join(
        f"@ {n} {book} {start} training | {book}\n~ {prev}\n{s}\n=\n\n"
        for n, (book, start, prev, s) in enumerate(items, 1001)
    )
    args.out.write_text(header + "\n" + body, encoding="utf-8")
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    logging.info("%d training items", len(items))


if __name__ == "__main__":
    main()
