"""Label eval items with a local model through ollama, for scoring by
`tests/real_text_eval.rs` (`NG_PREDICTIONS` / `NG_PRIVATE_PREDICTIONS`).

    uv run --no-project tests/corpus/tools/teacher.py tests/corpus out.txt --half even

The model sees the labelling guide (the header of labels.txt), a cast of the
book's frequent names, the text before the sentence, and the sentence. It
answers in the label format; a bracketed side is reduced to the name after
`=`, and one without a name is dropped, since the matcher cannot use it.
Tune prompts on even items only.
"""

import argparse
import json
import logging
import re
import urllib.request
from collections import Counter
from pathlib import Path

HONORIFIC = r"(?:Mr|Mrs|Miss|Ms|Dr|Lady|Lord|Sir|Captain|Colonel|Aunt|Uncle)\.?"
NAME = re.compile(rf"\b(?:{HONORIFIC} )?[A-Z][a-z]+(?: [A-Z][a-z]+)?\b")
COMMON = set(
    "The A An And But I It He She They We You His Her Their Our My Your This That "
    "There Then When What Who Why How If In On At Of For To With As So No Yes Not "
    "Oh Well Now Here All One Chapter CHAPTER Illustration Sir Madam".split()
)


def parse_items(labels: str):
    items, lines, i = [], labels.splitlines(), 0
    header = []
    while i < len(lines) and not lines[i].startswith("@ "):
        if lines[i].startswith("#"):
            header.append(lines[i][1:].removeprefix(" "))
        i += 1
    while i < len(lines):
        line = lines[i]
        if not line.startswith("@ "):
            i += 1
            continue
        fields = line[2:].split()
        number, book, start = int(fields[0]), fields[1], int(fields[2])
        context = lines[i + 1].removeprefix("~ ")
        sentence = lines[i + 2]
        items.append(dict(number=number, book=book, start=start, context=context, sentence=sentence))
        i += 4
    return "\n".join(header), items


def cast(text: str, size: int = 60):
    counts = Counter(
        m.group(0) for m in NAME.finditer(text) if m.group(0).split()[0] not in COMMON or " " in m.group(0)
    )
    return [name for name, n in counts.most_common(size * 2) if n >= 3 and name not in COMMON][:size]


def ask(model: str, prompt: str, host: str, num_ctx: int) -> str:
    body = json.dumps(
        {
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": False,
            "think": False,
            "options": {"temperature": 0, "num_ctx": num_ctx, "num_predict": 400},
        }
    ).encode()
    request = urllib.request.Request(f"{host}/api/chat", body, {"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=600) as response:
        return json.load(response)["message"]["content"]


def side(raw: str):
    raw = raw.strip()
    if raw.startswith("[") and raw.endswith("]"):
        _, eq, name = raw[1:-1].partition("=")
        return name.strip() or None if eq else None
    return raw or None


def claims(answer: str):
    for line in answer.splitlines():
        line = line.strip().strip("`").strip()
        line = re.sub(r"^[-*\d.)\s]+", "", line).rstrip("?").strip()
        line = line.removeprefix("implied ")
        parts = [p.strip() for p in line.split("|")]
        if len(parts) != 3:
            continue
        negated = parts[0].startswith("not ")
        subject = side(parts[0].removeprefix("not "))
        object_ = side(parts[2])
        relation = parts[1].strip().lower().replace(" ", "_")
        if subject and object_ and re.fullmatch(r"[a-z_]+", relation):
            yield ("not " if negated else "") + subject, relation, object_


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path, help="directory with labels.txt and the books")
    parser.add_argument("out", type=Path)
    parser.add_argument("--half", choices=["even", "odd", "all"], default="even")
    parser.add_argument("--model", default="qwen3:14b")
    parser.add_argument("--host", default="http://127.0.0.1:11434")
    parser.add_argument("--window", type=int, default=3000, help="characters of text before the sentence")
    parser.add_argument("--num-ctx", type=int, default=8192)
    parser.add_argument("--raw", type=Path, help="also write each answer here")
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(message)s")

    guide, items = parse_items((args.root / "labels.txt").read_text())
    keep = {"even": lambda n: n % 2 == 0, "odd": lambda n: n % 2 == 1, "all": lambda n: True}[args.half]
    books, casts = {}, {}
    out, raw = [], []
    chosen = [item for item in items if keep(item["number"])]
    for k, item in enumerate(chosen, 1):
        book = item["book"]
        if book not in books:
            books[book] = (args.root / f"{book}.txt").read_text()
            casts[book] = cast(books[book])
        text = books[book]
        before = text[max(0, item["start"] - args.window) : item["start"]]
        before = " ".join(before.split()[1:])
        prompt = (
            "You label relations between people in a novel, following this guide exactly.\n\n"
            f"=== GUIDE ===\n{guide}\n=== END GUIDE ===\n\n"
            f"Frequent names in this book: {', '.join(casts[book])}\n\n"
            "Text before the sentence, for knowing who is who (do not label it):\n"
            f"<<<{before}>>>\n\n"
            f"Sentence to label:\n<<<{item['sentence']}>>>\n\n"
            "Answer with one `A | relation | B` line per relation the sentence states "
            "(prefix `implied ` for what a careful reader infers), nothing else. For a person "
            "the sentence only describes, write `[description = Name]` with their name from the "
            "text when you know it. Remember `speaks_to` for anyone who says something to another. "
            "If the sentence states none, answer `none`."
        )
        answer = ask(args.model, prompt, args.host, args.num_ctx)
        raw.append(f"@ {item['number']}\n{answer}\n")
        for subject, relation, object_ in claims(answer):
            out.append(f"{item['number']} | {subject} | {relation} | {object_}")
        logging.info("%d/%d item %d", k, len(chosen), item["number"])

    args.out.write_text("\n".join(out) + "\n")
    if args.raw:
        args.raw.write_text("\n".join(raw))


if __name__ == "__main__":
    main()
