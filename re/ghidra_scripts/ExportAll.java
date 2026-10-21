// Exports decompiled C, imports, exports and defined strings for the current program.
//@category HM1
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.data.*;
import ghidra.program.model.address.*;
import java.io.*;

public class ExportAll extends GhidraScript {
    @Override
    public void run() throws Exception {
        String outDir = getScriptArgs().length > 0 ? getScriptArgs()[0] : ".";
        String name = currentProgram.getName();
        new File(outDir).mkdirs();
        // symbols: imports/exports
        try (PrintWriter w = new PrintWriter(new File(outDir, name + ".symbols.txt"))) {
            SymbolTable st = currentProgram.getSymbolTable();
            w.println("## EXTERNAL (imports)");
            for (Symbol s : st.getExternalSymbols()) {
                w.println(s.getParentNamespace().getName() + "!" + s.getName());
            }
            w.println("## ENTRY POINTS (exports)");
            AddressIterator it = st.getExternalEntryPointIterator();
            while (it.hasNext()) {
                Address a = it.next();
                Symbol s = st.getPrimarySymbol(a);
                w.println(a + " " + (s != null ? s.getName() : "?"));
            }
        }
        // strings
        try (PrintWriter w = new PrintWriter(new File(outDir, name + ".strings.txt"))) {
            DataIterator di = currentProgram.getListing().getDefinedData(true);
            while (di.hasNext()) {
                Data d = di.next();
                if (d.hasStringValue()) {
                    w.println(d.getAddress() + "\t" + String.valueOf(d.getValue()).replace("\n", "\\n"));
                }
            }
        }
        // decompile
        DecompInterface di = new DecompInterface();
        DecompileOptions opts = new DecompileOptions();
        di.setOptions(opts);
        di.openProgram(currentProgram);
        try (PrintWriter w = new PrintWriter(new File(outDir, name + ".c"))) {
            FunctionIterator fi = currentProgram.getFunctionManager().getFunctions(true);
            int n = 0;
            while (fi.hasNext() && !monitor.isCancelled()) {
                Function f = fi.next();
                if (f.isThunk() || f.isExternal()) continue;
                DecompileResults r = di.decompileFunction(f, 60, monitor);
                w.println("// ===== " + f.getEntryPoint() + " " + f.getName() + " size=" + f.getBody().getNumAddresses());
                if (r != null && r.decompileCompleted()) {
                    w.println(r.getDecompiledFunction().getC());
                } else {
                    w.println("// decompile failed: " + (r != null ? r.getErrorMessage() : "null"));
                }
                n++;
            }
            println("Decompiled " + n + " functions from " + name);
        }
        di.dispose();
    }
}
