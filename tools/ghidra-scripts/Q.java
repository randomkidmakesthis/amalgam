// Headless post-script for the FULL project (read-only). args: outDir cmd...
//   x:ADDR  -> xrefs to ADDR (from-address, type, containing function) appended to outDir/xrefs.txt
//   d:ADDR  -> decompile function containing ADDR to outDir/<entry>.c
//   c:ADDR  -> callers of function containing ADDR (xrefs to entry), plus callees list
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.util.*;

public class Q extends GhidraScript {
    void up(PrintWriter xw, Address ad, int lvl, Set<String> seen) {
        Function tf = getFunctionContaining(ad);
        Address t = tf != null ? tf.getEntryPoint() : ad;
        if (lvl > 4 || !seen.add(t.toString())) return;
        TreeSet<String> fs = new TreeSet<>(); StringBuilder data = new StringBuilder();
        for (Reference r : getReferencesTo(t)) {
            Function f = getFunctionContaining(r.getFromAddress());
            if (f == null) { if (!r.getFromAddress().toString().startsWith("14a")) data.append(" @").append(r.getFromAddress()); }
            else fs.add(f.getEntryPoint().toString());
        }
        String ind = "  ".repeat(lvl + 1);
        xw.println(ind + t + " callers=" + fs.size() + data);
        int n = 0;
        for (String f : fs) { if (n++ >= 6) { xw.println(ind + "  ..."); break; }
            up(xw, currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(Long.parseUnsignedLong(f, 16)), lvl + 1, seen); }
    }
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        File out = new File(a[0]);
        out.mkdirs();
        DecompInterface di = new DecompInterface();
        di.setOptions(new DecompileOptions());
        di.openProgram(currentProgram);
        PrintWriter xw = new PrintWriter(new FileWriter(new File(out, "xrefs.txt"), true));
        for (int i = 1; i < a.length; i++) {
            String cmd = a[i].substring(0, 1);
            Address ad = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(Long.parseUnsignedLong(a[i].substring(2), 16));
            if (cmd.equals("x") || cmd.equals("c")) {
                Address t = ad;
                Function tf = getFunctionContaining(ad);
                if (cmd.equals("c") && tf != null) t = tf.getEntryPoint();
                xw.println("== refs to " + t + (tf != null ? " (in " + tf.getEntryPoint() + ")" : ""));
                TreeMap<String, List<String>> byF = new TreeMap<>();
                for (Reference r : getReferencesTo(t)) {
                    Function f = getFunctionContaining(r.getFromAddress());
                    String k = f == null ? "nofunc" : f.getEntryPoint().toString();
                    byF.computeIfAbsent(k, z -> new ArrayList<>()).add(r.getFromAddress() + ":" + r.getReferenceType().getName());
                }
                for (Map.Entry<String, List<String>> e : byF.entrySet()) xw.println("  " + e.getKey() + " <- " + String.join(" ", e.getValue()));
                if (cmd.equals("c") && tf != null) {
                    StringBuilder sb = new StringBuilder("  callees:");
                    for (Function c : tf.getCalledFunctions(monitor)) sb.append(" ").append(c.getEntryPoint());
                    xw.println(sb);
                }
            } else if (cmd.equals("u")) {
                // u:ADDR -> caller tree upward, 4 levels, max 6 callers per node; data refs shown as @addr
                xw.println("== up-tree " + ad);
                up(xw, ad, 0, new HashSet<String>());
            } else if (cmd.equals("d")) {
                Function f = getFunctionContaining(ad);
                if (f == null) { println("NOFUNC " + a[i]); continue; }
                File o = new File(out, f.getEntryPoint().toString() + ".c");
                if (o.exists()) { println("HAVE " + f.getEntryPoint()); continue; }
                DecompileResults r = di.decompileFunction(f, 180, monitor);
                try (PrintWriter pw = new PrintWriter(o)) {
                    if (r.decompileCompleted()) pw.print(r.getDecompiledFunction().getC());
                    else pw.print("FAILED: " + r.getErrorMessage());
                }
                println("OK " + f.getEntryPoint() + " " + f.getBody().getNumAddresses());
            }
        }
        xw.close();
        di.dispose();
    }
}
