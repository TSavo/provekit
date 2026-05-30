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
        List<ImplicationDecl> implications = new ArrayList<>();

        ParseResult<CompilationUnit> result = new JavaParser().parse(source);
        if (result.isSuccessful() && result.getResult().isPresent()) {
            CompilationUnit cu = result.getResult().get();
            for (Extractor ex : extractors) {
                decls.addAll(ex.extract(cu, source));
            }
            ProductionWalk.Result productionWalk = ProductionWalk.lift(cu, sourceFileName(path));
            decls.addAll(productionWalk.declarations());
            implications.addAll(productionWalk.implications());
            decls = mergeDeclsBySymbol(decls);
            Map<String, String> contractIndex = buildContractIndex(decls);
            callEdges.addAll(JavaCallEdgeResolver.resolve(cu, path, contractIndex));
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
        sb.append("],\"implications\":[");
        for (int i = 0; i < implications.size(); i++) {
            if (i > 0) sb.append(",");
            sb.append(implications.get(i).toJson());
        }
        sb.append("],\"warnings\":[]}");
        return sb.toString();
    }

    public String lift(String workspace, String surface) {
        return lift(workspace, surface, "ir-document");
    }

    public String lift(String workspace, String surface, String emitMode) {
        List<ContractDecl> accumulatedDecls = new ArrayList<>();
        List<CallEdgeDecl> callEdges = new ArrayList<>();
        List<ImplicationDecl> implications = new ArrayList<>();
        Path root = Paths.get(workspace);
        try (var paths = Files.walk(root)) {
            paths
                .filter(p -> p.toString().endsWith(".java"))
                .sorted(Comparator.comparing(Path::toString))
                .forEach(p -> scanFile(
                    p,
                    accumulatedDecls,
                    callEdges,
                    implications,
                    "ir-document".equals(emitMode)
                ));
        } catch (IOException e) {
            System.err.println("Walk error: " + e.getMessage());
        }
        List<ContractDecl> decls = mergeDeclsBySymbol(accumulatedDecls);

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
        ir.append("],\"implications\":[");
        for (int i = 0; i < implications.size(); i++) {
            if (i > 0) ir.append(",");
            ir.append(implications.get(i).toJson());
        }
        ir.append("],\"diagnostics\":[]}");
        return ir.toString();
    }

    private void scanFile(
            Path path,
            List<ContractDecl> decls,
            List<CallEdgeDecl> callEdges,
            List<ImplicationDecl> implications,
            boolean emitVerifyContracts) {
        try {
            String source = Files.readString(path);
            ParseResult<CompilationUnit> result = new JavaParser().parse(source);
            if (!result.isSuccessful() || result.getResult().isEmpty()) return;
            CompilationUnit cu = result.getResult().get();
            List<ContractDecl> fileDecls = new ArrayList<>();
            for (Extractor ex : extractors) {
                fileDecls.addAll(ex.extract(cu, source));
            }
            ProductionWalk.Result productionWalk = ProductionWalk.lift(cu, path.getFileName().toString());
            fileDecls.addAll(productionWalk.declarations());
            implications.addAll(productionWalk.implications());
            if (emitVerifyContracts) {
                fileDecls.addAll(VerifyLift.lift(cu, path.getFileName().toString()));
            }
            decls.addAll(mergeDeclsBySymbol(fileDecls));

            // Build contract index from accumulated decls so far (including this file).
            // We use all decls accumulated up to this point; for single-file fixtures
            // the index is built from the current file's declarations.
            Map<String, String> contractIndex = buildContractIndex(decls);

            // Walk for same-language and JNI call edges per spec #114 R1/R3.
            callEdges.addAll(JavaCallEdgeResolver.resolve(cu, path.toString(), contractIndex));
            List<CallEdgeDecl> jniEdges = JniResolver.resolve(
                cu, path.toString(), contractIndex);
            callEdges.addAll(jniEdges);
        } catch (IOException e) {
            System.err.println("Parse error " + path + ": " + e.getMessage());
        }
    }

    private static String sourceFileName(String path) {
        Path file = Paths.get(path);
        Path name = file.getFileName();
        return name == null ? path : name.toString();
    }

    private static List<ContractDecl> mergeDeclsBySymbol(List<ContractDecl> decls) {
        Map<String, ContractDecl> merged = new LinkedHashMap<>();
        for (ContractDecl decl : decls) {
            ContractDecl existing = merged.get(decl.symbol);
            if (existing == null) {
                merged.put(decl.symbol, copyDecl(decl));
            } else {
                merged.put(decl.symbol, mergeDecl(existing, decl));
            }
        }
        return new ArrayList<>(merged.values());
    }

    private static ContractDecl copyDecl(ContractDecl decl) {
        if (isRawJsonDecl(decl)) {
            return decl;
        }
        return new ContractDecl(
            decl.symbol,
            new ArrayList<>(decl.preconditions),
            new ArrayList<>(decl.postconditions),
            new ArrayList<>(decl.invariants)
        );
    }

    private static ContractDecl mergeDecl(ContractDecl left, ContractDecl right) {
        if (isRawJsonDecl(left) || isRawJsonDecl(right)) {
            return mergePossiblyRawDecl(left, right);
        }
        List<String> pres = new ArrayList<>(left.preconditions);
        List<String> posts = new ArrayList<>(left.postconditions);
        List<String> invs = new ArrayList<>(left.invariants);
        appendUnique(pres, right.preconditions);
        appendUnique(posts, right.postconditions);
        appendUnique(invs, right.invariants);
        return new ContractDecl(left.symbol, pres, posts, invs);
    }

    private static boolean isRawJsonDecl(ContractDecl decl) {
        return decl.getClass() != ContractDecl.class;
    }

    private static ContractDecl mergePossiblyRawDecl(ContractDecl left, ContractDecl right) {
        if (left.toJson().equals(right.toJson())) {
            return left;
        }
        if (isRawJsonDecl(left)) {
            return left;
        }
        if (isRawJsonDecl(right)) {
            return right;
        }
        return mergeDecl(left, right);
    }

    private static void appendUnique(List<String> target, List<String> additions) {
        for (String item : additions) {
            if (!target.contains(item)) {
                target.add(item);
            }
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
