# Regenerates the sampled items of tests/corpus/labels.txt (fixed seed).
# usage: python3 tests/corpus/tools/sample_public.py <out.txt>, from the repo root
import re, random, sys
BOOKS = ["1342", "1260", "98", "2701", "768", "158"]
TITLES = {"1342":"Pride and Prejudice","1260":"Jane Eyre","98":"A Tale of Two Cities","2701":"Moby-Dick","768":"Wuthering Heights","158":"Emma"}
STOP = set("I Mr Mrs Miss Sir Lady Lord God Captain Dr Colonel Madame Monsieur Mademoiselle Master Heaven Oh O Ah Yes No Well The A".split())
REL = re.compile(r"\b(sister|brother|mother|father|daughter|son|wife|husband|aunt|uncle|niece|nephew|cousin|widow|friend|servant|master|mistress|guardian|betrothed|engaged|married|lover|enemy|rival|governess|pupil|tutor|heir|grandmother|grandfather|landlord|tenant|maid|housekeeper|ward)s?\b", re.I)
END = re.compile(r'(?<!\bMr)(?<!\bMrs)(?<!\bDr)(?<!\bSt)(?<!\bMessrs)(?<!\bMme)(?<!\bMlle)(?<!\b[A-Z])[.!?]["”’)]*\s+(?=["“‘(]?[A-Z])')
def sentences(text):
    pos = 0
    for para in re.finditer(r"(?:[^\n]+\n?)+", text):
        p, base = para.group(0), para.start()
        start = 0
        for m in END.finditer(p):
            yield base + start, p[start:m.end()].rstrip()
            start = m.end()
        tail = p[start:].rstrip()
        if tail: yield base + start, tail
def names(s):
    toks = re.findall(r"(?<![.!?] )(?<!^)\b[A-Z][a-z]+", s)
    return {t for t in toks if t not in STOP}
rng = random.Random(20261122)
items = []
for b in BOOKS:
    text = open(f"tests/corpus/{b}.txt", encoding="utf-8").read()
    sents = [(o, s) for o, s in sentences(text)]
    ok = [i for i, (o, s) in enumerate(sents) if 40 <= len(s) <= 350 and re.search(r'[.!?]["”’)]*$', s) and not s.isupper() and not re.match(r"\s*(CHAPTER|Chapter|VOLUME)", s) and i > 0]
    pick = {}
    namey = [i for i in ok if len(names(sents[i][1])) >= 2]
    rely = [i for i in ok if REL.search(sents[i][1]) and names(sents[i][1])]
    for i in rng.sample(rely, 7): pick.setdefault(i, "relation word")
    for i in rng.sample([i for i in namey if i not in pick], 12): pick[i] = "names"
    for i in rng.sample([i for i in ok if i not in pick], 6): pick[i] = "random"
    for i in sorted(pick):
        o, s = sents[i]
        bo = len(text[:o].encode())
        assert text.encode()[bo:bo+len(s.encode())].decode() == s
        prev = sents[i-1][1]
        items.append((b, bo, pick[i], " ".join(prev.split()), " ".join(s.split())))
out = []
for n, (b, bo, why, prev, s) in enumerate(items, 1):
    out.append(f"@ {n} {b} {bo} {why} | {TITLES[b]}\n~ {prev}\n{s}\n=\n\n")
open(sys.argv[1], "w", encoding="utf-8").write("".join(out))
print(len(items))
