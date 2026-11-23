"""Bring a labels file onto the guide's relation vocabulary.

    uv run --no-project tests/corpus/tools/revise_relations.py <labels.txt> [--dry-run]

A label whose relation the guide lists is kept. One in MAP is rewritten to
the guide relation that says the same thing. Any other is commented out as
`# outside the vocabulary: ...`, kept for the record but no longer scored,
since the matcher needs the exact relation and an invented name used once
cannot be learned or ruled for. Duplicates a rewrite creates are dropped.
"""

import argparse
import logging
import re
from pathlib import Path

# invented -> (guide relation, denied). Speech acts aimed at a listener are
# `speaks_to`; `distrusts` is a denied `trusts`.
MAP = {
    "accompanies": ("companion_of", False),
    "admires_taste_of": ("admires", False),
    "advises": ("speaks_to", False),
    "bears_ill_will_toward": ("hates", False),
    "bids_goodnight_to": ("greets", False),
    "courts": ("dating", False),
    "discloses_own_methods_to": ("speaks_to", False),
    "distrusts": ("trusts", True),
    "has_home_in": ("lives_in", False),
    "interrupts": ("speaks_to", False),
    "moved_to": ("lives_in", False),
    "possesses": ("owns", False),
    "rebukes": ("speaks_to", False),
    "scolds": ("speaks_to", False),
    "supports": ("helps", False),
    "threatens": ("speaks_to", False),
    "travels_with": ("companion_of", False),
    "warns": ("speaks_to", False),
    "works_in": ("works_at", False),
}

# `visits` a person is `meets`; a visit to a place has no guide relation.
VISITED_PEOPLE = {"Mrs. Bates", "Mr. Woodhouse", "Werner"}

RELATION = re.compile(r"[a-z_]+")


def vocabulary(header: str):
    listed = [
        w
        for line in header.split("Relations the library knows", 1)[1].splitlines()
        if line.startswith("#   ")
        for w in line[4:].split()
        if RELATION.fullmatch(w)
    ]
    quoted = []
    for q in header.split("`")[1::2]:
        parts = [p.strip() for p in q.split("|")]
        one = parts[0] if len(parts) == 1 else parts[1] if len(parts) == 3 else ""
        if RELATION.fullmatch(one):
            quoted.append(one)
    return set(listed) | set(quoted)


def name_of(side: str) -> str:
    side = side.strip()
    if side.startswith("[") and side.endswith("]"):
        return side[1:-1].partition("=")[2].strip()
    return side


def revise(line: str, vocab):
    """The revised label line, or None to comment it out."""
    body = line.rstrip()
    unsure = body.endswith("?")
    body = body.rstrip("?").rstrip()
    implied = body.startswith("implied ")
    body = body.removeprefix("implied ")
    denied = body.startswith("not ")
    body = body.removeprefix("not ")
    subject, relation, object_ = [p.strip() for p in body.split("|")]
    if relation in vocab:
        return line.rstrip()
    if relation == "visits" and name_of(object_) in VISITED_PEOPLE:
        relation = "meets"
    elif relation in MAP:
        relation, flip = MAP[relation]
        denied ^= flip
    else:
        return None
    out = ("implied " if implied else "") + ("not " if denied else "") + f"{subject} | {relation} | {object_}"
    return out + (" ?" if unsure else "")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("labels", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    text = args.labels.read_text(encoding="utf-8")
    header = text.split("\n@ ", 1)[0]
    vocab = vocabulary(header)

    lines = text.splitlines()
    out, seen = [], set()
    in_item, rewritten, removed = False, 0, 0
    for i, line in enumerate(lines):
        if line.startswith("@ "):
            in_item, seen = False, set()
        elif line == "=" and i >= 3 and lines[i - 3].startswith("@ "):
            in_item = True
        elif in_item and line.strip() and not line.startswith("#") and "|" in line:
            new = revise(line, vocab)
            key = None if new is None else new.rstrip(" ?")
            if new is None or key in seen:
                if args.dry_run:
                    logging.info("- %s", line)
                removed += 1
                line = f"# outside the vocabulary: {line.strip()}" if new is None else f"# duplicate after revision: {line.strip()}"
            else:
                seen.add(key)
                if new != line.rstrip():
                    if args.dry_run:
                        logging.info("~ %s  ->  %s", line, new)
                    rewritten += 1
                line = new
        out.append(line)
    if not args.dry_run:
        args.labels.write_text("\n".join(out) + "\n", encoding="utf-8")
    logging.info("%d rewritten, %d commented out", rewritten, removed)


if __name__ == "__main__":
    main()
