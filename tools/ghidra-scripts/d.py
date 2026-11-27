# python d.py <va> [end|+len] : annotated disassembly of the pdata function containing va (or range)
import sys; sys.path.insert(0, r'C:\GTAVI\Tools\ghidra-scripts')
from peutil import *
import pefile
_pe=pefile.PE(EXE,fast_load=True); _pe.parse_data_directories(directories=[1])
IMP={}
for d in _pe.DIRECTORY_ENTRY_IMPORT:
    for i in d.imports: IMP[i.address]=(i.name.decode() if i.name else d.dll.decode()+'#%d'%i.ordinal)
def cstr(va):
    b=read(va,80)
    if not b: return None
    e=b.find(b'\0')
    if e>=4 and all(32<=c<127 for c in b[:e]): return b[:e].decode()
def ann(ins):
    t=rip_target(ins)
    a=''
    if t is not None:
        if t in IMP: a=' ; IMP '+IMP[t]
        else:
            s=cstr(t)
            if s: a=' ; "%s"'%s
            else:
                sec=sec_of(t)
                if sec=='.rdata':
                    q=read(t,8)
                    if q:
                        v=struct.unpack('<Q',q)[0]
                        a=' ; [%x]=%x'%(t,v) if ins.mnemonic!='lea' else ' ; &%x'%t
                else: a=' ; %s:%x'%(sec,t)
    elif ins.mnemonic=='call' and ins.operands[0].type==capstone.x86.X86_OP_IMM: pass
    return a
def dump(b,e):
    for ins in dis(b,end=e):
        print('%x  %-7s %s%s'%(ins.address,ins.mnemonic,ins.op_str,ann(ins)))
if __name__=='__main__':
    va=int(sys.argv[1],16)
    if len(sys.argv)>2:
        a=sys.argv[2]; e=va+int(a[1:],16) if a[0]=='+' else int(a,16); dump(va,e)
    else:
        f=func_of(va); print('func %x-%x'%f); dump(*f)
