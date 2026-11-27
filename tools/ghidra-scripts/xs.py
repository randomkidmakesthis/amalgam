# xrefs to strings by file offset or substring: python xs.py <substr>...
import sys; sys.path.insert(0, r'C:\GTAVI\Tools\ghidra-scripts')
from peutil import *
def off2va(o):
    for n,v,vs,r,rs in SECS:
        if r<=o<r+rs: return v+o-r
import re
for pat in sys.argv[1:]:
    for m in re.finditer(re.escape(pat.encode()), MM):
        # extend to string start
        s=m.start()
        while MM[s-1]>=0x20 and MM[s-1]<0x7f: s-=1
        e=MM.find(b'\0',s)
        va=off2va(s)
        refs=confirm_refs(va,va+1)
        print('%x "%s": %s'%(va,MM[s:e].decode(), ', '.join('%x(f=%x)'%(i.address,(func_of(i.address) or (0,))[0]) for i,t in refs)))
