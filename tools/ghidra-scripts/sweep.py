# python sweep.py <minhits> <regex1> <regex2> ... : pdata functions whose disassembly matches ALL regexes (on "mnemonic op_str")
# prints func start and first match address per regex. Uses 4 worker processes.
import sys, re
sys.path.insert(0, r'C:\GTAVI\Tools\ghidra-scripts')
from multiprocessing import Pool
def work(args):
    lo,hi,pats=args
    import peutil as P, capstone
    cs=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64)
    rx=[re.compile(p) for p in pats]
    out=[]
    for i in range(lo,hi):
        b=int(P.FB[i]); e=int(P.FE[i])
        code=P.read(b,e-b)
        if not code: continue
        hit=[None]*len(rx)
        for a,s,m,o in cs.disasm_lite(code,b):
            t=m+' '+o
            for k,r in enumerate(rx):
                if hit[k] is None and r.search(t): hit[k]=a
        if all(h is not None for h in hit): out.append((b,hit))
    return out
if __name__=='__main__':
    import peutil as P
    pats=sys.argv[1:]
    n=len(P.FBL); step=2000
    jobs=[(i,min(n,i+step),pats) for i in range(0,n,step)]
    with Pool(4) as p:
        for res in p.imap(work,jobs):
            for b,hit in res: print('%x %s'%(b,' '.join('%x'%h for h in hit)),flush=True)
