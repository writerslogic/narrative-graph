# /// script
# requires-python = ">=3.11,<3.14"
# dependencies = [
#     "onnx==1.23.2",
#     "safetensors>=0.6,<1",
#     "torch==2.10.0",
#     "transformers==4.57.6",
# ]
# ///
"""Fine-tune a local DeBERTa student and export the ONNX checkpoint."""

import argparse
import hashlib
import json
import os
import random
import re
import shutil
import subprocess
import tempfile
from collections import Counter, defaultdict
from pathlib import Path

import torch
from huggingface_hub import hf_hub_download
from safetensors.torch import load_file, save_file
from torch import nn
from torch.nn import functional as F
from transformers import AutoConfig, AutoModel

from teacher import PUBLIC, claims, parse_items, training_books, vocabulary
from sample_training import sentence_windows

BASE = "microsoft/deberta-v3-small"
REVISION = "a59be8aa63396e73dbb45a1487e4cde4be98bfa4"
SEED = 20261123
EXPORT_BATCH, MAX_TOKENS, MAX_NAMES = 2, 320, 16
RAW_HEADER = re.compile(r"^@ ([0-9]+)\n", re.M)
TITLES = set("mr mrs miss ms lady lord sir captain colonel doctor dr aunt uncle".split())


def requests(items, books):
    grouped = defaultdict(list)
    for item in items:
        grouped[item["book"]].append(item)
    result = []
    bounds = {}
    for book, entries in grouped.items():
        path = PUBLIC / f"{book}.txt"
        windows = dict(sentence_windows(books[book].decode("utf-8")))
        spans = []
        for item in entries:
            number, start = item["number"], item["start"]
            lo, end, _, sentence = windows[start]
            if " ".join(sentence.split()) != item["sentence"]:
                raise ValueError(f"item {number} differs from {path.name} at byte {start}")
            bounds[number] = (lo, end)
            spans.append({"number": number, "span": [start, end]})
        result.append({"path": str(path), "items": spans})
    return json.dumps(result), bounds


def training_answers(raw, items):
    fields = RAW_HEADER.split(raw)
    answers = {int(number): answer.strip() for number, answer in zip(fields[1::2], fields[2::2])}
    numbers = {item["number"] for item in items}
    if fields[0].strip() or len(answers) != len(fields[1::2]) or answers.keys() != numbers:
        raise ValueError("raw teacher answers must cover each training item exactly once")
    return answers


def name_parts(value):
    return tuple(re.split(r"[\s._]+", value.casefold().strip(" ._")))


def candidate_index(name, candidates):
    wanted = name_parts(name)
    if wanted in candidates:
        return candidates.index(wanted)
    matches = []
    for index, candidate in enumerate(candidates):
        if wanted[0] in TITLES and candidate[0] in TITLES and wanted[0] != candidate[0]:
            continue
        short, long = sorted((wanted, candidate), key=len)
        if short == long[:len(short)] or short == long[-len(short):]:
            matches.append(index)
    return matches[0] if len(matches) == 1 else None


def datasets(inputs, answers, vocab, items, bounds):
    by_book = defaultdict(list)
    for item in items:
        by_book[item["book"]].append((item["start"], item["number"]))
    cuts = {}
    for book_items in by_book.values():
        book_items.sort()
        validation = max(1, len(book_items) // 5)
        separation = min(bounds[number][0] for _, number in book_items[-validation:])
        cuts.update((number, "valid") for _, number in book_items[-validation:])
        cuts.update((number, "train") for _, number in book_items[:-validation]
                    if bounds[number][1] <= separation)

    train, valid = [], []
    coverage, dropped = Counter(), Counter()
    for row in inputs:
        number = row["number"]
        if number not in cuts:
            dropped["split boundary"] += 1
            continue
        answer = answers[number]
        gold = set(claims(answer, vocab))
        if not gold and answer.strip(" `.*").casefold() != "none":
            dropped["unparseable answer"] += 1
            continue
        names = [name_parts(name) for name in row["names"]]
        pairs = set()
        for subject, label, object_ in gold:
            if subject.startswith("not "):
                subject = subject.removeprefix("not ")
                label = f"not {label}"
            i, j = candidate_index(subject, names), candidate_index(object_, names)
            if i is not None and j is not None and i != j:
                pairs.add((i, j, label))
                coverage["matched"] += 1
        coverage["claims"] += len(gold)
        row.update(pairs=sorted(pairs), gold=len(gold))
        (valid if cuts[number] == "valid" else train).append(row)

    counts = Counter(label for row in train for _, _, label in row["pairs"])
    labels = {label: index for index, label in enumerate(sorted(label for label in counts if counts[label] >= 3))}
    for row in train + valid:
        row["pairs"] = [(i, j, labels[label]) for i, j, label in row["pairs"] if label in labels]
    train = [row for row in train if len(row["positions"]) >= 2 and (row["pairs"] or not row["gold"])]
    if not train or not valid or not labels:
        raise ValueError("training and validation need aligned positive labels")
    print(f"Train {len(train)}, validation {len(valid)}, labels {len(labels)}; "
          f"aligned {coverage['matched']}/{coverage['claims']} claims, excluded {dict(dropped)}", flush=True)
    return train, valid, labels


class Student(nn.Module):
    def __init__(self, labels, backbone):
        super().__init__()
        self.backbone = backbone
        self.backbone.embeddings.requires_grad_(False)
        self.backbone.encoder.layer[:2].requires_grad_(False)
        hidden = self.backbone.config.hidden_size
        self.subject = nn.Linear(2 * hidden, 128)
        self.object = nn.Linear(2 * hidden, 128)
        self.classifier = nn.Linear(128, labels + 1)

    def forward(self, ids, mask, positions, targets):
        hidden = self.backbone(input_ids=ids, attention_mask=mask).last_hidden_state
        width = hidden.shape[-1]
        names = hidden.gather(1, positions[:, :, None].expand(-1, -1, width))
        context = hidden.gather(1, targets[:, None, None].expand(-1, 1, width)).expand_as(names)
        features = torch.cat((names, context), dim=-1)
        pairs = self.subject(features).unsqueeze(2) + self.object(features).unsqueeze(1)
        return self.classifier(F.gelu(pairs))


def collate(rows, pad, labels, device):
    batch, length = len(rows), max(len(row["ids"]) for row in rows)
    width = max(2, max(len(row["positions"]) for row in rows))
    ids = torch.full((batch, length), pad, dtype=torch.long)
    mask = torch.zeros_like(ids)
    positions = torch.zeros((batch, width), dtype=torch.long)
    targets = torch.tensor([row["target"] for row in rows])
    expected = torch.zeros((batch, width, width, labels))
    for index, row in enumerate(rows):
        length, names = len(row["ids"]), len(row["positions"])
        ids[index, :length] = torch.tensor(row["ids"])
        mask[index, :length] = 1
        positions[index, :names] = torch.tensor(row["positions"], dtype=torch.long)
        for i, j, label in row["pairs"]:
            expected[index, i, j, label] = 1
    named = positions != 0
    pairs = named[:, :, None] & named[:, None, :] & ~torch.eye(width, dtype=torch.bool)
    return tuple(t.to(device) for t in (ids, mask, positions, targets, expected, pairs))


def pair_log_probs(scores, pairs):
    logits = scores[..., 0].masked_fill(~pairs, -1e4).flatten(1)
    return F.log_softmax(torch.cat((logits, torch.zeros_like(logits[:, :1])), dim=1), dim=1)


def loss_for(scores, expected, pairs):
    positive = expected.any(dim=-1).flatten(1)
    weights = torch.cat((positive, ~positive.any(dim=1, keepdim=True)), dim=1).float()
    link_loss = -(weights * pair_log_probs(scores, pairs)).sum(dim=1) / weights.sum(dim=1)
    relation_loss = -(expected * F.log_softmax(scores[..., 1:], dim=-1)).sum() / expected.sum().clamp_min(1)
    return link_loss.mean() + relation_loss


@torch.inference_mode()
def validate(model, rows, pad, labels, device, batch_size):
    model.eval()
    thresholds = torch.linspace(0.01, 0.98, 98)
    count = torch.zeros(len(thresholds) + 1, dtype=torch.long)
    correct = torch.zeros_like(count)
    gold = sum(row["gold"] for row in rows)
    for k in range(0, len(rows), batch_size):
        *inputs, expected, pairs = collate(rows[k:k + batch_size], pad, labels, device)
        scores = model(*inputs)
        link = pair_log_probs(scores, pairs)[:, :-1].exp().reshape(pairs.shape)
        confidence = (link.unsqueeze(-1) * scores[..., 1:].softmax(dim=-1)).cpu()
        present = torch.bucketize(confidence[pairs.cpu()], thresholds, right=True)
        count += present.flatten().bincount(minlength=len(count))
        correct += present[expected.cpu()[pairs.cpu()].bool()].bincount(minlength=len(count))
    predicted = count.flip(0).cumsum(0).flip(0)[1:]
    matched = correct.flip(0).cumsum(0).flip(0)[1:]
    f1 = 2 * matched / (predicted + gold).clamp_min(1)
    index = int(f1.argmax())
    return {"threshold": thresholds[index].item(), "f1": f1[index].item(),
            "precision": int(matched[index]) / max(1, int(predicted[index])),
            "recall": int(matched[index]) / max(1, gold),
            "matched": int(matched[index]), "predicted": int(predicted[index]), "gold": gold}


def copy_tokenizer(path):
    if not path.exists():
        source = hf_hub_download(repo_id=BASE, revision=REVISION, filename="spm.model")
        pending = path.with_suffix(".tmp")
        shutil.copyfile(source, pending)
        pending.replace(path)


def export_onnx(bundle, model):
    model.load_state_dict(load_file(bundle / "model.safetensors"))
    model.to("cpu").eval()
    ids = torch.ones((EXPORT_BATCH, MAX_TOKENS), dtype=torch.long)
    positions = torch.ones((EXPORT_BATCH, MAX_NAMES), dtype=torch.long)
    targets = torch.ones(EXPORT_BATCH, dtype=torch.long)
    pending = bundle / "model.onnx.tmp"
    with torch.inference_mode():
        torch.onnx.export(model, (ids, torch.ones_like(ids), positions, targets), pending, dynamo=False,
                          opset_version=17, external_data=False,
                          input_names=("ids", "mask", "positions", "targets"), output_names=("scores",))
    pending.replace(bundle / "model.onnx")


def train_model(args, labels_file, evaluated):
    items_bytes = args.items.read_bytes()
    raw_bytes = args.answers.read_bytes()
    _, items = parse_items(items_bytes.decode("utf-8"))
    books = training_books(PUBLIC, items, evaluated)
    request, bounds = requests(items, books)
    answers = training_answers(raw_bytes.decode("utf-8"), items)
    vocab = vocabulary(labels_file.split("\n@ ", 1)[0])
    device = torch.device(args.device or ("mps" if torch.backends.mps.is_available() else "cpu"))
    torch.manual_seed(SEED)
    rng = random.Random(SEED)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    if args.out.exists():
        raise FileExistsError(args.out)
    with tempfile.TemporaryDirectory(prefix=f".{args.out.name}-", dir=args.out.parent) as directory:
        bundle = Path(directory)
        tokenizer_path = bundle / "spm.model"
        copy_tokenizer(tokenizer_path)
        encoded = subprocess.run([args.helper, "inputs", tokenizer_path], input=request,
                                 capture_output=True, text=True, check=False)
        if encoded.returncode:
            raise RuntimeError(f"student input generation failed: {encoded.stderr.strip()}")
        inputs = [json.loads(line) for line in encoded.stdout.splitlines()]
        if len(inputs) != len(answers) or {row["number"] for row in inputs} != answers.keys():
            raise ValueError("student inputs must cover each teacher answer exactly once")
        train, valid, labels = datasets(inputs, answers, vocab, items, bounds)
        backbone = AutoModel.from_pretrained(BASE, revision=REVISION, use_safetensors=True)
        model = Student(len(labels), backbone).to(device)
        optimizer = torch.optim.AdamW([
            {"params": [p for p in model.backbone.parameters() if p.requires_grad], "lr": 2e-5},
            {"params": [p for name, p in model.named_parameters() if not name.startswith("backbone.")], "lr": 3e-4},
        ])
        pad = backbone.config.pad_token_id
        best = {"f1": 0.0}
        print(f"Training on {device} for {args.epochs} epochs", flush=True)
        for epoch in range(1, args.epochs + 1):
            model.train()
            rng.shuffle(train)
            loss_sum = 0.0
            for k in range(0, len(train), args.batch_size):
                *features, expected, pairs = collate(train[k:k + args.batch_size], pad, len(labels), device)
                optimizer.zero_grad(set_to_none=True)
                loss = loss_for(model(*features), expected, pairs)
                loss.backward()
                nn.utils.clip_grad_norm_(model.parameters(), 1.0)
                optimizer.step()
                loss_sum += loss.detach().item()
            result = validate(model, valid, pad, len(labels), device, args.batch_size)
            print(f"Epoch {epoch}: loss {loss_sum / ((len(train) + args.batch_size - 1) // args.batch_size):.4f}, "
                  f"validation {json.dumps(result, sort_keys=True)}", flush=True)
            if result["f1"] > best["f1"]:
                best = dict(result, epoch=epoch)
                weights = {name: tensor.detach().to(device="cpu", dtype=torch.float16).contiguous()
                           for name, tensor in model.state_dict().items()}
                save_file(weights, bundle / "model.safetensors")
        if "epoch" not in best:
            raise ValueError("student matched no teacher labels on validation")
        model.backbone.config.to_json_file(bundle / "config.json", use_diff=False)
        manifest = {"version": 2, "labels": list(labels), "threshold": best["threshold"],
                    "base": BASE, "revision": REVISION, "seed": SEED, "validation": best,
                    "training_items_sha256": hashlib.sha256(items_bytes).hexdigest(),
                    "teacher_answers_sha256": hashlib.sha256(raw_bytes).hexdigest()}
        (bundle / "student.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        export_onnx(bundle, model)
        bundle.rename(args.out)
    print(f"Exported epoch {best['epoch']} to {args.out}", flush=True)


def main():
    target = Path(os.environ.get("CARGO_TARGET_DIR", PUBLIC.parent.parent / "target"))
    parser = argparse.ArgumentParser()
    parser.add_argument("--helper", type=Path, default=target / "release/examples/student_corpus")
    commands = parser.add_subparsers(dest="command", required=True)
    train = commands.add_parser("train")
    train.add_argument("items", type=Path)
    train.add_argument("answers", type=Path)
    train.add_argument("out", type=Path)
    train.add_argument("--epochs", type=int, default=6)
    train.add_argument("--batch-size", type=int, default=8)
    train.add_argument("--device", choices=("cpu", "mps", "cuda"))
    predict = commands.add_parser("predict")
    predict.add_argument("checkpoint", type=Path)
    predict.add_argument("out", type=Path)
    predict.add_argument("--half", choices=("even", "odd", "all"), default="even")
    predict.add_argument("--with-rules", action="store_true")
    export = commands.add_parser("export")
    export.add_argument("checkpoint", type=Path)
    args = parser.parse_args()
    if args.command == "export":
        manifest = json.loads((args.checkpoint / "student.json").read_text(encoding="utf-8"))
        if manifest["version"] != 2:
            parser.error("unsupported student checkpoint version")
        config = AutoConfig.from_pretrained(args.checkpoint, local_files_only=True)
        model = Student(len(manifest["labels"]), AutoModel.from_config(config))
        copy_tokenizer(args.checkpoint / "spm.model")
        export_onnx(args.checkpoint, model)
        return
    labels_file = (PUBLIC / "labels.txt").read_text(encoding="utf-8")
    _, evaluated = parse_items(labels_file)
    if args.command == "train":
        if args.epochs <= 0 or args.batch_size <= 0:
            parser.error("epochs and batch size must be positive")
        train_model(args, labels_file, evaluated)
    else:
        parity = ("even", "odd")
        items = [item for item in evaluated if args.half in ("all", parity[item["number"] % 2])]
        books = {book: (PUBLIC / f"{book}.txt").read_bytes() for book in {item["book"] for item in items}}
        request, _ = requests(items, books)
        mode = "predict-with-rules" if args.with_rules else "predict"
        with args.out.open("w", encoding="utf-8") as out:
            subprocess.run([args.helper, mode, args.checkpoint], input=request,
                           stdout=out, text=True, check=True)


if __name__ == "__main__":
    main()
