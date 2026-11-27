// Headless post-script: decompile the functions at the given addresses (hex), creating them if needed.
// args: outDir addr1 addr2 ...   -> writes outDir/<addr>.c
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import java.io.*;

public class Decomp extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        File out = new File(a[0]);
        out.mkdirs();
        DecompInterface di = new DecompInterface();
        DecompileOptions opt = new DecompileOptions();
        di.setOptions(opt);
        di.openProgram(currentProgram);
        for (int i = 1; i < a.length; i++) {
            Address ad = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(Long.parseUnsignedLong(a[i], 16));
            Function f = getFunctionContaining(ad);
            if (f == null) {
                if (getInstructionAt(ad) == null) {
                    new DisassembleCommand(ad, null, true).applyTo(currentProgram, monitor);
                }
                new CreateFunctionCmd(ad).applyTo(currentProgram, monitor);
                f = getFunctionAt(ad);
            }
            if (f == null) { println("NOFUNC " + a[i]); continue; }
            DecompileResults r = di.decompileFunction(f, 180, monitor);
            File o = new File(out, a[i] + ".c");
            try (PrintWriter pw = new PrintWriter(o)) {
                if (r.decompileCompleted()) pw.print(r.getDecompiledFunction().getC());
                else pw.print("FAILED: " + r.getErrorMessage());
            }
            println("OK " + a[i] + " " + f.getBody().getNumAddresses());
        }
        di.dispose();
    }
}
