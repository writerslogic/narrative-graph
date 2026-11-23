"""Label eval items with a local model through ollama, for scoring by
`tests/real_text_eval.rs` (`NG_PREDICTIONS` / `NG_PRIVATE_PREDICTIONS`).

    uv run --no-project tests/corpus/tools/teacher.py tests/corpus out.txt --half even

The model sees the labelling guide (the header of labels.txt), a cast of the
book's frequent names, the text before the sentence, and the sentence. It
answers in the label format; a bracketed side is reduced to the name after
`=`, and one without a name is dropped, since the matcher cannot use it.
Tune prompts on even items only.

`--provider openai` sends the prompt to the OpenAI Responses API with
`OPENAI_API_KEY`. It refuses any corpus but the public one beside this
script: the private set is copyrighted and never leaves the machine.
"""

import argparse
import json
import logging
import os
import re
import time
import urllib.error
import urllib.request
from collections import Counter
from pathlib import Path

from revise_relations import vocabulary

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


PUBLIC = (Path(__file__).resolve().parent.parent).resolve()


def ask_openai(model: str, prompt: str) -> str:
    body = json.dumps({"model": model, "input": prompt}).encode()
    request = urllib.request.Request(
        "https://api.openai.com/v1/responses",
        body,
        {"Content-Type": "application/json", "Authorization": f"Bearer {os.environ['OPENAI_API_KEY']}"},
    )
    for attempt in range(4):
        try:
            with urllib.request.urlopen(request, timeout=600) as response:
                data = json.load(response)
            break
        except urllib.error.HTTPError as error:
            if error.code not in (429, 500, 502, 503) or attempt == 3:
                raise
            time.sleep(10 * (attempt + 1))
    return "".join(
        c["text"]
        for o in data["output"]
        if o.get("type") == "message"
        for c in o["content"]
        if c.get("type") == "output_text"
    )


def ask(model: str, prompt: str, host: str, num_ctx: int, think: bool) -> str:
    body = json.dumps(
        {
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": False,
            "think": think,
            "options": {"temperature": 0, "num_ctx": num_ctx, "num_predict": 4000 if think else 400},
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


def claims(answer: str, vocab):
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
        if subject and object_ and relation in vocab:
            yield ("not " if negated else "") + subject, relation, object_


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path, help="directory with labels.txt and the books")
    parser.add_argument("out", type=Path)
    parser.add_argument("--half", choices=["even", "odd", "all"], default="even")
    parser.add_argument("--provider", choices=["ollama", "openai"], default="ollama")
    parser.add_argument("--model", default="qwen3:14b")
    parser.add_argument("--host", default="http://127.0.0.1:11434")
    parser.add_argument("--window", type=int, default=3000, help="characters of text before the sentence")
    parser.add_argument("--num-ctx", type=int, default=8192)
    parser.add_argument("--think", action="store_true", help="let the model reason before answering")
    parser.add_argument(
        "--batch",
        choices=["submit", "collect"],
        help="openai only: submit the items as one batch, or collect its answers",
    )
    parser.add_argument("--raw", type=Path, help="also write each answer here")
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    if args.batch and args.provider != "openai":
        parser.error("--batch needs --provider openai")
    if args.provider == "openai" and args.root.resolve() != PUBLIC:
        parser.error(f"--provider openai sends text off the machine; only {PUBLIC} may be sent")

    labels = (args.root / "labels.txt").read_text()
    guide, items = parse_items(labels)
    vocab = vocabulary(labels.split("\n@ ", 1)[0])
    keep = {"even": lambda n: n % 2 == 0, "odd": lambda n: n % 2 == 1, "all": lambda n: True}[args.half]
    books, casts = {}, {}
    chosen = [item for item in items if keep(item["number"])]
    prompts = []
    for item in chosen:
        book = item["book"]
        if book not in books:
            books[book] = (args.root / f"{book}.txt").read_bytes()
            casts[book] = cast(books[book].decode("utf-8", "replace"))
        # The label offsets are bytes; the books are full of three-byte quotes.
        start = item["start"]
        before = books[book][max(0, start - args.window * 2) : start].decode("utf-8", "ignore")
        before = " ".join(before[-args.window :].split()[1:])
        # The guide comes first and is the same for every item, so a provider
        # that caches prompt prefixes bills it once.
        prompts.append(
            "You label relations between people in a novel, following this guide exactly.\n\n"
            f"=== GUIDE ===\n{guide}\n=== END GUIDE ===\n\n"
            "Answer with one `A | relation | B` line per relation the sentence states "
            "(prefix `implied ` for what a careful reader infers), nothing else. Use only these "
            f"relations, never another: {' '.join(sorted(vocab))}. Each side must be a person's "
            "or place's name: resolve he, she, I, you, me and descriptions (`her sister`, `the "
            "old man`) to the name the text gives them, writing `[description = Name]`; if the "
            "text never names them, leave that relation out. In dialogue, the speaker of each "
            "line `speaks_to` the person addressed, even when the line has no tag: follow the "
            "turns of the conversation. If the sentence states none, answer `none`.\n\n"
            f"Frequent names in this book: {', '.join(casts[book])}\n\n"
            "Text before the sentence, for knowing who is who (do not label it):\n"
            f"<<<{before}>>>\n\n"
            f"The sentence just before it:\n<<<{item['context']}>>>\n\n"
            f"Sentence to label:\n<<<{item['sentence']}>>>"
        )

    state = args.out.with_name(args.out.name + ".batch.json")
    if args.batch == "submit":
        submit_batch(args.model, chosen, prompts, state)
        return
    if args.batch == "collect":
        answers = collect_batch(state)
        if answers is None:
            raise SystemExit(1)
    else:
        answers = {}
        for k, (item, prompt) in enumerate(zip(chosen, prompts), 1):
            if args.provider == "openai":
                answers[item["number"]] = ask_openai(args.model, prompt)
            else:
                answers[item["number"]] = ask(args.model, prompt, args.host, args.num_ctx, args.think)
            logging.info("%d/%d item %d", k, len(chosen), item["number"])
    write(args, chosen, answers, vocab)


def write(args, chosen, answers, vocab):
    out, raw = [], []
    for item in chosen:
        answer = answers.get(item["number"], "")
        raw.append(f"@ {item['number']}\n{answer}\n")
        for subject, relation, object_ in claims(answer, vocab):
            out.append(f"{item['number']} | {subject} | {relation} | {object_}")
    args.out.write_text("\n".join(out) + "\n")
    if args.raw:
        args.raw.write_text("\n".join(raw))
    logging.info("%d answers, %d claims", len(answers), len(out))


def openai(method: str, path: str, body: bytes | None = None, content_type: str = "application/json"):
    headers = {"Authorization": f"Bearer {os.environ['OPENAI_API_KEY']}"}
    if body is not None:
        headers["Content-Type"] = content_type
    request = urllib.request.Request(f"https://api.openai.com/v1/{path}", body, headers, method=method)
    with urllib.request.urlopen(request, timeout=600) as response:
        return response.read()


def submit_batch(model: str, chosen, prompts, state: Path):
    """Upload one Responses request per item and start a batch: half the
    price of live calls, finished within 24 hours."""
    lines = "".join(
        json.dumps(
            {
                "custom_id": f"item-{item['number']}",
                "method": "POST",
                "url": "/v1/responses",
                "body": {"model": model, "input": prompt},
            }
        )
        + "\n"
        for item, prompt in zip(chosen, prompts)
    ).encode()
    boundary = "teacher-batch-boundary"
    form = (
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nbatch\r\n"
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"requests.jsonl\"\r\n"
        "Content-Type: application/jsonl\r\n\r\n"
    ).encode() + lines + f"\r\n--{boundary}--\r\n".encode()
    uploaded = json.loads(openai("POST", "files", form, f"multipart/form-data; boundary={boundary}"))
    batch = json.loads(
        openai(
            "POST",
            "batches",
            json.dumps(
                {"input_file_id": uploaded["id"], "endpoint": "/v1/responses", "completion_window": "24h"}
            ).encode(),
        )
    )
    state.write_text(json.dumps({"batch": batch["id"], "model": model}))
    logging.info("batch %s submitted, %d requests; state in %s", batch["id"], len(prompts), state)


def collect_batch(state: Path):
    """The answers by item number, or None while the batch is unfinished."""
    batch_id = json.loads(state.read_text())["batch"]
    batch = json.loads(openai("GET", f"batches/{batch_id}"))
    logging.info("batch %s: %s %s", batch_id, batch["status"], batch.get("request_counts"))
    if batch["status"] != "completed":
        return None
    answers = {}
    for line in openai("GET", f"files/{batch['output_file_id']}/content").decode().splitlines():
        record = json.loads(line)
        number = int(record["custom_id"].removeprefix("item-"))
        response = record.get("response") or {}
        if response.get("status_code") != 200:
            logging.warning("item %d failed: %s", number, record.get("error"))
            continue
        answers[number] = "".join(
            c["text"]
            for o in response["body"]["output"]
            if o.get("type") == "message"
            for c in o["content"]
            if c.get("type") == "output_text"
        )
    return answers

if __name__ == "__main__":
    main()
