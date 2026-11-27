# Verified xrefs: only keep hits that appear on a linear sweep from the containing pdata function start.
import sys; sys.path.insert(0, r'C:\GTAVI\Tools\ghidra-scripts')
from peutil import *
from collections import defaultdict
lo = int(sys.argv[1], 16); hi = int(sys.argv[2], 16)
verbose = len(sys.argv) > 3
refs = confirm_refs(lo, hi)
sweep_cache = {}
def sweep(f):
    if f not in sweep_cache:
        sweep_cache[f] = {i.address: i for i in dis(f[0], end=f[1])}
    return sweep_cache[f]
byt = defaultdict(list); bad = []
for ins, t in refs:
    f = func_of(ins.address)
    if f is None:
        # leaf w/o pdata: sweep from previous func end
        bad.append((ins, t, 'nofunc')); continue
    sw = sweep(f)
    if ins.address in sw:
        byt[t].append((ins, f))
    else:
        bad.append((ins, t, 'offpath'))
for t in sorted(byt):
    fs = defaultdict(list)
    for ins, f in byt[t]: fs[f].append(ins)
    print('=== %x sec=%s nrefs=%d nfuncs=%d' % (t, sec_of(t), len(byt[t]), len(fs)))
    for f, L in fs.items():
        print('   func %x-%x: %s' % (f[0], f[1], ', '.join('%x %s(%s %s)' % (i.address, access(i, t), i.mnemonic, i.op_str.split(',')[0] if access(i,t)!='write' else i.op_str.split(',')[-1].strip()) for i in L)))
if verbose:
    print('--- rejected')
    for ins, t, why in bad:
        print('   %x -> %x %s %s %s' % (ins.address, t, why, ins.mnemonic, ins.op_str))
