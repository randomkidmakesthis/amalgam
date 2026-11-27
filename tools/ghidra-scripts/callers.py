# python callers.py <va>... : direct call/jmp rel32 sites + rip-relative refs (lea/data pointers) to va
import sys; sys.path.insert(0, r'C:\GTAVI\Tools\ghidra-scripts')
from peutil import *
T=text_np()
def rel_sites(target):
    out=[]
    for opc in (0xE8,0xE9):
        idx=np.nonzero(T[:-5]==opc)[0]
        d=(T[idx+1].astype(np.int64)|(T[idx+2].astype(np.int64)<<8)|(T[idx+3].astype(np.int64)<<16)|(T[idx+4].astype(np.int64)<<24))
        d=np.where(d>=1<<31,d-(1<<32),d)
        tg=TEXT_VA+idx+5+d
        for i in idx[tg==target].tolist(): out.append((TEXT_VA+i,'call' if opc==0xE8 else 'jmp'))
    return sorted(out)
_Q=None
def ptr_sites(target):
    """8-byte pointers to target in .rdata/.data"""
    out=[]
    pat=struct.pack('<Q',target)
    for n,v,vs,r,rs in SECS:
        if n in('.rdata','.data'):
            p=r
            while True:
                p=MM.find(pat,p,r+rs)
                if p<0: break
                out.append(v+p-r); p+=1
    return out
if __name__=='__main__':
    for a in sys.argv[1:]:
        t=int(a,16)
        print('== %x'%t)
        for va,k in rel_sites(t):
            f=func_of(va); print('  %s at %x  func=%s'%(k,va,'%x'%f[0] if f else '?'))
        for ins,tt in confirm_refs(t,t+1):
            f=func_of(ins.address); print('  ref %s at %x func=%s'%(ins.mnemonic,ins.address,'%x'%f[0] if f else '?'))
        for p in ptr_sites(t): print('  ptr at %x'%p)
