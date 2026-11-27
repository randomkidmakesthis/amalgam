// Headless post-script for the FULLY ANALYSED project. Exports, per topic, one text file with:
//   the seed functions, their callers two levels up (addresses), decompiled C of seeds + callers (capped),
//   string cross-references for topic regexes, and functions making indirect calls through given vtable offsets.
// args: outDir [capPerTopic=250] [onlyTopic e.g. T3]
// Run (read-only, no analysis):
//   analyzeHeadless C:\GTAVI\Tools\ghidra-proj scarlett -process game_scarlett_master.exe -readOnly -noanalysis
//       -scriptPath C:\GTAVI\Tools\ghidra-scripts -postScript ExportTopics.java C:\GTAVI\Tools\ghidra-scripts\out\topics
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.util.*;
import java.util.regex.*;

public class ExportTopics extends GhidraScript {
    DecompInterface di;
    int cap = 250;

    Address A(long v) { return currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(v); }

    Function fn(long v) {
        Address a = A(v);
        Function f = getFunctionContaining(a);
        return f;
    }

    // functions that reference address a (calls, jumps, data reads, pointers)
    Set<Function> refFuncs(Address a) {
        Set<Function> s = new LinkedHashSet<>();
        for (Reference r : getReferencesTo(a)) {
            Function f = getFunctionContaining(r.getFromAddress());
            if (f != null) s.add(f);
        }
        return s;
    }

    Set<Function> callersOf(Function f) {
        Set<Function> s = refFuncs(f.getEntryPoint());
        // through thunks
        Address[] th = f.getFunctionThunkAddresses(true);
        if (th != null) for (Address t : th) s.addAll(refFuncs(t));
        s.remove(f);
        return s;
    }

    void decomp(PrintWriter pw, Function f, String why) {
        pw.println("\n//==== " + f.getEntryPoint() + " " + f.getName() + " size=" + f.getBody().getNumAddresses() + " (" + why + ")");
        if (f.getBody().getNumAddresses() > 60000) { pw.println("// skipped: too large"); return; }
        try {
            DecompileResults r = di.decompileFunction(f, 60, monitor);
            if (r.decompileCompleted()) pw.print(r.getDecompiledFunction().getC());
            else pw.println("// FAILED: " + r.getErrorMessage());
        } catch (Exception e) { pw.println("// EXC " + e); }
    }

    // seeds: function addresses (any address inside) ; data: addresses whose referencing functions become seeds
    void topic(File dir, String name, String title, long[] seeds, long[] data, String strRegex, int[] vtOffsets,
               Map<Integer, List<Function>> vtHits) throws Exception {
        long t0 = System.currentTimeMillis();
        try (PrintWriter pw = new PrintWriter(new File(dir, name + ".txt"))) {
            pw.println("// " + name + ": " + title);
            LinkedHashMap<Function, String> lvl0 = new LinkedHashMap<>();
            for (long s : seeds) {
                Function f = fn(s);
                if (f == null) pw.println("// seed " + Long.toHexString(s) + ": NO FUNCTION");
                else lvl0.put(f, "seed " + Long.toHexString(s));
            }
            for (long d : data) {
                Set<Function> fs = refFuncs(A(d));
                pw.println("// data/IAT " + Long.toHexString(d) + " referenced by " + fs.size() + " functions");
                for (Function f : fs) lvl0.putIfAbsent(f, "refs " + Long.toHexString(d));
            }
            if (strRegex != null) {
                Pattern p = Pattern.compile(strRegex);
                pw.println("\n// ---- strings matching /" + strRegex + "/ and the functions that reference them");
                DataIterator it = currentProgram.getListing().getDefinedData(true);
                int n = 0;
                while (it.hasNext() && !monitor.isCancelled()) {
                    Data dt = it.next();
                    if (!dt.hasStringValue()) continue;
                    Object v = dt.getValue();
                    if (v == null) continue;
                    String s = v.toString();
                    if (!p.matcher(s).find()) continue;
                    Set<Function> fs = refFuncs(dt.getAddress());
                    StringBuilder sb = new StringBuilder();
                    for (Function f : fs) { sb.append(' ').append(f.getEntryPoint()); if (n < 400) lvl0.putIfAbsent(f, "string \"" + s + "\""); }
                    pw.println("// " + dt.getAddress() + " \"" + s + "\" <-" + sb);
                    n++;
                }
            }
            if (vtOffsets != null) for (int o : vtOffsets) {
                List<Function> fs = vtHits.getOrDefault(o, Collections.emptyList());
                pw.println("\n// ---- functions with an indirect CALL [reg+0x" + Integer.toHexString(o) + "] (" + fs.size() + "; unfiltered, any interface)");
                StringBuilder sb = new StringBuilder("//");
                for (Function f : fs) sb.append(' ').append(f.getEntryPoint());
                pw.println(sb);
            }
            LinkedHashMap<Function, String> lvl1 = new LinkedHashMap<>(), lvl2 = new LinkedHashMap<>();
            for (Function f : lvl0.keySet()) for (Function c : callersOf(f)) if (!lvl0.containsKey(c)) lvl1.putIfAbsent(c, "calls " + f.getEntryPoint());
            for (Function f : lvl1.keySet()) for (Function c : callersOf(f)) if (!lvl0.containsKey(c) && !lvl1.containsKey(c)) lvl2.putIfAbsent(c, "calls " + f.getEntryPoint());
            pw.println("\n// ---- call tree (callee <- callers)");
            for (Function f : lvl0.keySet()) { StringBuilder sb = new StringBuilder(); for (Function c : callersOf(f)) sb.append(' ').append(c.getEntryPoint()); pw.println("// L0 " + f.getEntryPoint() + " [" + lvl0.get(f) + "] <-" + sb); }
            for (Function f : lvl1.keySet()) { StringBuilder sb = new StringBuilder(); for (Function c : callersOf(f)) sb.append(' ').append(c.getEntryPoint()); pw.println("// L1 " + f.getEntryPoint() + " [" + lvl1.get(f) + "] <-" + sb); }
            pw.println("// L2 count " + lvl2.size());
            int n = 0;
            pw.println("\n// ---- decompiled: seeds, then level-1 callers, then level-2 callers, capped at " + cap);
            for (Map.Entry<Function, String> e : lvl0.entrySet()) { if (n++ >= cap || monitor.isCancelled()) break; decomp(pw, e.getKey(), "L0 " + e.getValue()); }
            for (Map.Entry<Function, String> e : lvl1.entrySet()) { if (n++ >= cap || monitor.isCancelled()) break; decomp(pw, e.getKey(), "L1 " + e.getValue()); }
            for (Map.Entry<Function, String> e : lvl2.entrySet()) { if (n++ >= cap || monitor.isCancelled()) break; decomp(pw, e.getKey(), "L2 " + e.getValue()); }
            pw.println("\n// done: L0 " + lvl0.size() + " L1 " + lvl1.size() + " L2 " + lvl2.size() + " decompiled " + Math.min(n, cap) + " in " + (System.currentTimeMillis() - t0) / 1000 + " s");
        }
        println(name + " done in " + (System.currentTimeMillis() - t0) / 1000 + " s");
    }

    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        File dir = new File(a.length > 0 ? a[0] : "C:\\GTAVI\\Tools\\ghidra-scripts\\out\\topics");
        dir.mkdirs();
        if (a.length > 1) cap = Integer.parseInt(a[1]);
        String only = a.length > 2 ? a[2] : null;
        di = new DecompInterface();
        di.setOptions(new DecompileOptions());
        di.openProgram(currentProgram);

        // one pass over all instructions: indirect calls through the vtable offsets we care about
        int[] offs = { 0xe8 /* queue: page-mapping batch */, 0x1d8 /* list: ExecuteIndirect */, 0x118 /* queue: execute with frame token */,
                       0x280 /* device: wait frame event */, 0x100 /* queue: present */ };
        Map<Integer, List<Function>> vt = new HashMap<>();
        Map<String, Integer> pat = new HashMap<>();
        for (int o : offs) { pat.put(" + 0x" + Integer.toHexString(o) + "]", o); vt.put(o, new ArrayList<>()); }
        InstructionIterator ii = currentProgram.getListing().getInstructions(true);
        Function lastF = null; Set<Integer> seen = new HashSet<>();
        while (ii.hasNext() && !monitor.isCancelled()) {
            Instruction in = ii.next();
            if (!in.getMnemonicString().equals("CALL")) continue;
            String s = in.toString();
            if (s.indexOf('[') < 0) continue;
            for (Map.Entry<String, Integer> e : pat.entrySet()) if (s.endsWith(e.getKey())) {
                Function f = getFunctionContaining(in.getAddress());
                if (f == null) continue;
                if (f != lastF) { lastF = f; seen.clear(); }
                if (seen.add(e.getValue())) vt.get(e.getValue()).add(f);
            }
        }
        println("indirect-call scan done");

        Object[][] T = {
          { "T1", "GPU read-backs: read-back ring class, generic buffer lock/unlock and their users",
            new long[]{0x141adc2c0L,0x141d40f90L,0x145121550L,0x144e8abe0L,0x14236e090L,0x145aa2440L}, new long[]{}, "(?i)readback|read back|histogram|exposure", null },
          { "T2", "How GPU-visible memory is filled: buffer create/alloc, alias region, multi-frame buffer index helpers",
            new long[]{0x1406ebaa0L,0x142474690L,0x14006dc20L,0x1455ac2b0L,0x14524df10L,0x144f244b0L,0x144f244d0L,0x142216459L},
            new long[]{0x149e1dac0L,0x149e1dac8L,0x149e1dad0L,0x146793a00L,0x146793a58L}, "(?i)growbuffer|atMemoryPool|sharedpools|upload", null },
          { "T3", "Page mapping churn: XMem thunks/IAT, D3D12ConfigureVirtualMemoryX, 0x140f2c470, queue+0xe8 callers",
            new long[]{0x140f2c470L,0x141e1cf60L},
            new long[]{0x146793a40L,0x146793a48L,0x146793a10L,0x146793a28L,0x146793a50L,0x146793c08L,0x145f3fce0L,0x145f3fd20L,0x145f3fcd0L,0x145f3fd10L,0x145f3fcc0L,0x145f3fd60L}, null, new int[]{0xe8} },
          { "T4", "Submission/pacing: game submit thread, batch enqueue, stream flush, frame interval/wait/present, thread creation, Sleep",
            new long[]{0x142481e90L,0x1401f6330L,0x1450bb280L,0x14247ab10L,0x1424845b0L,0x142490080L,0x140cd7320L,0x1424826a0L,0x142478440L,0x141ad0fd0L,0x1443c38a0L,0x1443c4140L,0x145eb2a00L},
            new long[]{0x146793b98L,0x14092eca0L,0x144fb0e90L}, null, new int[]{0x118,0x280,0x100} },
          { "T5", "Machine queries: console type, CPU topology, thread table, memory status",
            new long[]{0x140075140L,0x1455deb88L,0x141f67680L,0x141b67cd0L,0x1455fde00L}, new long[]{0x146793bb0L,0x146793bd0L,0x146793a30L}, null, null },
          { "T6", "Indirect draws: argument-buffer names and ExecuteIndirect (list+0x1d8) callers",
            new long[]{0x14247af60L}, new long[]{}, "(?i)IndirectArgs|culled meshlets|meshlets$|visibility_buffer", new int[]{0x1d8} },
          { "T7", "Streamer detail-level choice (low-LOD stand-ins): DirectStorage factory users, LOD/streaming/mip strings",
            new long[]{}, new long[]{0x146793fb8L}, "(?i)DirectStorage|Prestream|strStreaming|streaming|_lod|lod_|imposter|mipLevels|ttdb|hd_|_hd|standin|placeholder", null },
        };
        for (Object[] t : T) {
            if (only != null && !only.equals(t[0])) continue;
            if (monitor.isCancelled()) break;
            topic(dir, (String) t[0], (String) t[1], (long[]) t[2], (long[]) t[3], (String) t[4], (int[]) t[5], vt);
        }
        di.dispose();
    }
}
