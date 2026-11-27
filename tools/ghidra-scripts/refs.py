import sys; sys.path.insert(0, r'C:\GTAVI\Tools\ghidra-scripts')
from peutil import *
from collections import defaultdict
lo = int(sys.argv[1], 16); hi = int(sys.argv[2], 16)
refs = confirm_refs(lo, hi)
byt = defaultdict(list)
for ins, t in refs:
    byt[t].append((ins, func_of(ins.address)))
for t in sorted(byt):
    init = read(t, 4)
    print('=== %x sec=%s init=%s nrefs=%d' % (t, sec_of(t), init.hex() if init else 'bss', len(byt[t])))
    for ins, f in byt[t]:
        print('   %x  %-6s %-8s %s  func=%s' % (ins.address, access(ins, t), ins.mnemonic, ins.op_str,
              ('%x-%x' % f) if f else '?'))
