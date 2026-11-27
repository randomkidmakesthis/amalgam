# Shared helpers for raw analysis of game_scarlett_master.exe (read-only).
import mmap, struct, bisect, numpy as np, pefile, capstone

EXE = r'C:\GTAVI\GTA6\game_scarlett_master.exe'
_f = open(EXE, 'rb')
MM = mmap.mmap(_f.fileno(), 0, access=mmap.ACCESS_READ)
PE = pefile.PE(data=MM[:0x1000], fast_load=True)
BASE = PE.OPTIONAL_HEADER.ImageBase
SECS = []
for s in PE.sections:
    SECS.append((s.Name.rstrip(b'\0').decode(), BASE + s.VirtualAddress, s.Misc_VirtualSize, s.PointerToRawData, s.SizeOfRawData))

def sec_of(va):
    for n, v, vs, r, rs in SECS:
        if v <= va < v + max(vs, rs):
            return n
    return None

def va2off(va):
    for n, v, vs, r, rs in SECS:
        if v <= va < v + rs:
            return r + (va - v)
    return None  # bss / not in file

def read(va, n):
    o = va2off(va)
    if o is None:
        return None
    return MM[o:o + n]

TEXT = [s for s in SECS if s[0] == '.text'][0]
TEXT_VA, TEXT_RAW, TEXT_RS = TEXT[1], TEXT[3], TEXT[4]

# .pdata
_pd = [s for s in SECS if s[0] == '.pdata'][0]
_raw = MM[_pd[3]:_pd[3] + _pd[2]]
_arr = np.frombuffer(_raw[:len(_raw) // 12 * 12], dtype='<u4').reshape(-1, 3)
_arr = _arr[_arr[:, 0] != 0]
_arr = _arr[np.argsort(_arr[:, 0])]
FB = (_arr[:, 0].astype(np.int64) + BASE)
FE = (_arr[:, 1].astype(np.int64) + BASE)
FU = _arr[:, 2]
FBL = FB.tolist()

def func_of(va):
    """Return (begin,end) of the RUNTIME_FUNCTION containing va. Chained (child) entries
    are resolved to their primary entry via unwind info chain flag."""
    i = bisect.bisect_right(FBL, va) - 1
    if i < 0 or not (FB[i] <= va < FE[i]):
        return None
    b, e = int(FB[i]), int(FE[i])
    # follow chained unwind info to primary
    u = int(FU[i]) + BASE
    for _ in range(16):
        d = read(u, 4)
        if d is None: break
        flags = d[0] >> 3
        if not (flags & 4):  # UNW_FLAG_CHAININFO
            break
        cnt = d[2]
        off = 4 + ((cnt + 1) & ~1) * 2
        pb, pe_, pu = struct.unpack('<III', read(u + off, 12))
        b = pb + BASE
        u = pu + BASE
    return (b, e)

def primary_extent(b):
    """end of primary entry plus all chained fragments pointing to it (approx: chained entries are separate)"""
    i = bisect.bisect_left(FBL, b)
    return int(FE[i]) if i < len(FBL) and FBL[i] == b else None

CS = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
CS.detail = True

def dis(va, n=None, end=None, maxi=100000):
    if end is None: end = va + (n or 0x100)
    code = read(va, end - va)
    out = []
    for ins in CS.disasm(code, va):
        out.append(ins)
        if len(out) >= maxi: break
    return out

def rip_target(ins):
    for op in ins.operands:
        if op.type == capstone.x86.X86_OP_MEM and op.mem.base == capstone.x86.X86_REG_RIP:
            return ins.address + ins.size + op.mem.disp
    return None

_TEXT_NP = None
def text_np():
    global _TEXT_NP
    if _TEXT_NP is None:
        _TEXT_NP = np.frombuffer(MM, dtype=np.uint8, count=TEXT_RS, offset=TEXT_RAW)
    return _TEXT_NP

def scan_rip(lo, hi, chunk=1 << 24):
    """candidate offsets where va+4+disp32+k in [lo,hi) for k in 0..4. Returns sorted unique VAs of disp field."""
    t = text_np()
    res = set()
    n = len(t) - 4
    for s in range(0, n, chunk):
        e = min(n, s + chunk)
        b = t[s:e + 3].astype(np.int64)
        d = b[:-3] | (b[1:-2] << 8) | (b[2:-1] << 16) | (b[3:] << 24)
        d = np.where(d >= 1 << 31, d - (1 << 32), d)
        va = TEXT_VA + np.arange(s, e, dtype=np.int64)
        tg = va + 4 + d
        m = (tg >= lo - 4) & (tg < hi)
        for x in va[m].tolist():
            res.add(x)
    return sorted(res)

def confirm_refs(lo, hi):
    """Return list of (ins, target) for real instructions referencing [lo,hi) via RIP."""
    out = []
    seen = set()
    for dva in scan_rip(lo, hi):
        for back in range(1, 12):
            st = dva - back
            if st in seen: continue
            ins = dis(st, 16, maxi=1)
            if not ins: continue
            ins = ins[0]
            t = rip_target(ins)
            if t is not None and lo <= t < hi and ins.address + ins.size > dva and ins.address < dva:
                # check disp location matches
                if ins.disp_offset and ins.address + ins.disp_offset == dva:
                    seen.add(st)
                    out.append((ins, t))
                    break
    return out

def access(ins, t):
    """classify read/write/addr"""
    m = ins.mnemonic
    if m == 'lea': return 'addr'
    if len(ins.operands) >= 1 and ins.operands[0].type == capstone.x86.X86_OP_MEM and \
       ins.operands[0].mem.base == capstone.x86.X86_REG_RIP:
        if m.startswith('cmp') or m.startswith('test') or m.startswith('ucomis') or m.startswith('comis'):
            return 'read'
        if m in ('inc', 'dec', 'add', 'sub', 'or', 'and', 'xor', 'lock'):
            return 'rmw'
        return 'write'
    return 'read'
