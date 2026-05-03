package com.provekit.lift;

import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.ServiceLoader;
import com.github.javaparser.*;
import com.github.javaparser.ast.*;
import com.provekit.ir.CallEdgeDecl;

public class LiftHandler {
    private final List<Extractor> extractors = new ArrayList<>();

    public LiftHandler() {
        List<Extractor> loaded = new ArrayList<>();
        ServiceLoader.load(Extractor.class).forEach(loaded::add);
        // Sort by name() for deterministic ordering across builds.
        loaded.sort(java.util.Comparator.comparing(Extractor::name));
        extractors.addAll(loaded);
    }

    /**
     * Parse a single Java source file and return the canonical NDJSON parse result.
     *
     * This is the daemon-conformant counterpart to lift(): it accepts {path, source}
     * and returns {declarations, callEdges, warnings} per the bridge-linkage protocol.
     *
     * @param path   absolute path of the file (used for locus in call edges)
     * @param source full Java source text
     * @return JSON string: {"declarations":[...],"callEdges":[...],"warnings":[]}
     */
    public String parseSource(String path, String source) {
        List<ContractDecl> decls = new ArrayList<>();
        List<CallEdgeDecl> callEdges = new ArrayList<>();

        ParseResult<CompilationUnit> result = new JavaParser().parse(source);
        if (result.isSuccessful() && result.getResult().isPresent()) {
            CompilationUnit cu = result.getResult().get();
            for (Extractor ex : extractors) {
                decls.addAll(ex.extract(cu, source));
            }
            Map<String, String> contractIndex = buildContractIndex(decls);
            callEdges.addAll(JniResolver.resolve(cu, path, contractIndex));
        }

        // Emit declarations array.
        StringBuilder sb = new StringBuilder();
        sb.append("{\"declarations\":[");
        for (int i = 0; i < decls.size(); i++) {
            if (i > 0) sb.append(",");
            sb.append(decls.get(i).toJson());
        }
        sb.append("],\"callEdges\":[");
        for (int i = 0; i < callEdges.size(); i++) {
            if (i > 0) sb.append(",");
            sb.append(callEdges.get(i).toJson());
        }
        sb.append("],\"warnings\":[]}");
        return sb.toString();
    }

    public String lift(String workspace, String surface) {
        List<ContractDecl> decls = new ArrayList<>();
        List<CallEdgeDecl> callEdges = new ArrayList<>();
        Path root = Paths.get(workspace);
        try {
            Files.walk(root)
                .filter(p -> p.toString().endsWith(".java"))
                .forEach(p -> scanFile(p, decls, callEdges));
        } catch (IOException e) {
            System.err.println("Walk error: " + e.getMessage());
        }

        StringBuilder ir = new StringBuilder();
        ir.append("{\"kind\":\"ir-document\",\"ir\":[");
        for (int i = 0; i < decls.size(); i++) {
            if (i > 0) ir.append(",");
            ir.append(decls.get(i).toJson());
        }
        ir.append("],\"callEdges\":[");
        for (int i = 0; i < callEdges.size(); i++) {
            if (i > 0) ir.append(",");
            ir.append(callEdges.get(i).toJson());
        }
        ir.append("],\"diagnostics\":[]}");
        return ir.toString();
    }

    private void scanFile(Path path, List<ContractDecl> decls, List<CallEdgeDecl> callEdges) {
        try {
            String source = Files.readString(path);
            ParseResult<CompilationUnit> result = new JavaParser().parse(source);
            if (!result.isSuccessful() || result.getResult().isEmpty()) return;
            CompilationUnit cu = result.getResult().get();
            for (Extractor ex : extractors) {
                decls.addAll(ex.extract(cu, source));
            }

            // Build contract index from accumulated decls so far (including this file).
            // We use all decls accumulated up to this point; for single-file fixtures
            // the index is built from the current file's declarations.
            Map<String, String> contractIndex = buildContractIndex(decls);

            // Walk for JNI call edges per spec #114 R1/R3.
            List<CallEdgeDecl> jniEdges = JniResolver.resolve(
                cu, path.toString(), contractIndex);
            callEdges.addAll(jniEdges);
        } catch (IOException e) {
            System.err.println("Parse error " + path + ": " + e.getMessage());
        }
    }

    /**
     * Build a contract index from a list of ContractDecls.
     * Maps "symbol" -> CID (SHA-256 hex of the JSON bytes, as a stable stand-in).
     *
     * For the JNI resolver the CID content does not need to be cryptographically
     * correct; it must be stable and deterministic for a given decl. We use the
     * blake3-512 prefix convention from the IR spec but substitute a simple
     * stable hash derived from the contract JSON bytes since the Java kit does
     * not link against a blake3 library.
     *
     * The JNI tests build the contractIndex directly with known values; this
     * index is used when lifting a real workspace.
     */
    static Map<String, String> buildContractIndex(List<ContractDecl> decls) {
        Map<String, String> index = new LinkedHashMap<>();
        for (ContractDecl d : decls) {
            String json = d.toJson();
            // Stable CID: "blake3-512:<hex of java hashCode, zero-padded>".
            // This is a placeholder matching the zero-config Java lift context;
            // the full BLAKE3-512 CID is computed by the Rust verifier.
            String fakeCid = "blake3-512:" + String.format("%016x", (long) json.hashCode() & 0xFFFFFFFFFFFFFFFFL);
            index.put(d.symbol, fakeCid);
        }
        return index;
    }
}
