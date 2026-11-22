# Merges a model review into a labels file: each item's label lines become the
# reviewer's `final` set, existing `#` notes are kept, and every change is
# recorded as a `# review:` note so it can be audited or reverted.
# usage: apply_review.py <labels.txt> <dir of chunk_*.json> <model name>
import glob, json, re, sys
labels, review_dir, model = sys.argv[1:4]
s = open(labels, encoding="utf-8").read()
reviews = {}
for path in sorted(glob.glob(f"{review_dir}/chunk_*.json")):
    d = json.load(open(path))
    text = next(c["text"] for o in d["output"] if o.get("type") == "message"
                for c in o["content"] if c.get("type") == "output_text")
    for item in json.loads(text)["items"]:
        reviews[str(item["number"])] = item
changed = 0
def merge(m):
    global changed
    head, number, body = m.group(1), m.group(2), m.group(3)
    item = reviews.get(number)
    if item is None:
        return m.group(0)
    notes = [l for l in body.splitlines() if l.startswith("#")]
    old = [l for l in body.splitlines() if l.strip() and not l.startswith("#")]
    final = [l.strip() for l in item["final"] if l.strip()]
    if final != old:
        changed += 1
    notes += [f"# review ({model}): {c['action']} `{c['label']}`: {c['reason']}" for c in item["changes"]]
    lines = final + notes
    return head + "=\n" + "".join(l + "\n" for l in lines) + "\n"
s = re.sub(r"^(@ (\d+) .*\n~ .*\n.*\n)=\n((?:(?!@ ).*\n)*?)(?:\n|\Z)", merge, s, flags=re.M)
open(labels, "w", encoding="utf-8").write(s.rstrip("\n") + "\n")
print(f"{len(reviews)} reviewed, {changed} items changed")
