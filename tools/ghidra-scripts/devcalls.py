# functions referencing a global object pointer and the vtable offsets they call: python devcalls.py <globalVA>
import sys,re; sys.path.insert(0, r'C:\GTAVI\Tools\ghidra-scripts')
from peutil import *
from collections import defaultdict
g=int(sys.argv[1],16)
refs=confirm_refs(g,g+1)
funcs=defaultdict(list)
for ins,t in refs:
    f=func_of(ins.address)
    if f: funcs[f].append(ins.address)
res=defaultdict(list)
for f in sorted(funcs):
    offs=set()
    for ins in dis(f[0],end=f[1]):
        if ins.mnemonic in('call','jmp'):
            m=re.match(r'qword ptr \[r\w+ \+ (0x[0-9a-f]+)\]$',ins.op_str)
            if m: offs.add(int(m.group(1),16))
    for o in offs: res[o].append(f[0])
for o in sorted(res): print('%#x n=%d: %s'%(o,len(res[o]),' '.join('%x'%x for x in res[o][:25])))
