import re, sys, glob
# usage: apply_labels.py <dir of '== N' label batches> <labels.txt>
p=sys.argv[2]
s=open(p,encoding='utf-8').read()
for f in sorted(glob.glob(sys.argv[1]+'/*.txt')):
    blocks=re.split(r'^== (\d+)\n', open(f,encoding='utf-8').read(), flags=re.M)[1:]
    for n, body in zip(blocks[::2], blocks[1::2]):
        body=body.strip('\n')
        body=(body+'\n') if body else ''
        m=re.search(rf'^@ {n} .*\n~ .*\n.*\n=\n((?:(?!@ ).*\n)*?)\n', s, re.M)
        assert m, n
        s=s[:m.start(1)]+body+s[m.end(1):]
open(p,'w',encoding='utf-8').write(s)
