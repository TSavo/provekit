package com.provekit.lift;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.github.javaparser.ast.expr.AnnotationExpr;
import com.github.javaparser.ast.expr.MethodCallExpr;
import com.provekit.ir.Jcs;

public final class JavaImplicationLifter {
    private JavaImplicationLifter() {}

    private record ContractBinding(String contractCid) {}

    private record SourceFile(String relPath, Path fullPath) {}

    private record Callsite(String callee, String file, int line, int col) {}

    public static Jcs.Obj lift(Jcs.Obj params) {
        Path workspaceRoot = Paths.get(orDefault(params.stringFieldOrNull("workspace_root"), "."));
        List<String> sourcePaths = sourcePaths(params.get("source_paths"));
        Map<String, ContractBinding> contractsByCallee = contractBindings(params.get("contract_bindings"));

        List<Jcs.Json> ir = new ArrayList<>();
        List<Jcs.Json> diagnostics = new ArrayList<>();
        for (SourceFile sourceFile : resolveJavaSourceFiles(workspaceRoot, sourcePaths)) {
            CompilationUnit cu = parseSource(sourceFile.fullPath());
            if (cu == null) continue;
            for (Callsite callsite : collectCallsites(cu, sourceFile.relPath())) {
                ContractBinding binding = contractsByCallee.get(callsite.callee());
                if (binding == null) {
                    diagnostics.add(liftGap("no-contract-for-callee", callsite));
                    continue;
                }
                if (binding.contractCid() == null || binding.contractCid().isBlank()) {
                    diagnostics.add(liftGap("binding-missing-contract-cid", callsite));
                    continue;
                }
                ir.add(bridge(callsite, binding.contractCid()));
            }
        }

        return Jcs.object(
            "diagnostics", Jcs.array(diagnostics),
            "ir", Jcs.array(ir),
            "kind", Jcs.string("ir-document")
        );
    }

    private static List<String> sourcePaths(Jcs.Json json) {
        List<String> paths = new ArrayList<>();
        if (json instanceof Jcs.Arr arr) {
            for (Jcs.Json item : arr.values()) {
                if (item instanceof Jcs.Str s && !s.value().isBlank()) {
                    paths.add(s.value());
                }
            }
        }
        if (paths.isEmpty()) paths.add(".");
        return paths;
    }

    private static Map<String, ContractBinding> contractBindings(Jcs.Json json) {
        Map<String, ContractBinding> bindings = new LinkedHashMap<>();
        if (!(json instanceof Jcs.Arr arr)) return bindings;
        for (Jcs.Json item : arr.values()) {
            if (!(item instanceof Jcs.Obj obj)) continue;
            String name = obj.stringFieldOrNull("name");
            if (name == null || name.isBlank()) continue;
            String callee = name.split("@", 2)[0].trim();
            if (callee.isEmpty()) continue;
            bindings.putIfAbsent(callee, new ContractBinding(obj.stringFieldOrNull("contract_cid")));
        }
        return bindings;
    }

    private static List<SourceFile> resolveJavaSourceFiles(Path workspaceRoot, List<String> sourcePaths) {
        Path root = workspaceRoot.toAbsolutePath().normalize();
        List<SourceFile> files = new ArrayList<>();
        for (String sourcePath : sourcePaths) {
            Path fullPath = root.resolve(sourcePath).normalize();
            if (Files.isDirectory(fullPath)) {
                Path scanRoot = ".".equals(sourcePath) && Files.isDirectory(fullPath.resolve("src"))
                    ? fullPath.resolve("src")
                    : fullPath;
                collectJavaFiles(root, scanRoot, files);
            } else if (Files.isRegularFile(fullPath) && fullPath.toString().endsWith(".java")) {
                files.add(new SourceFile(relativePath(root, fullPath), fullPath));
            }
        }
        files.sort(Comparator.comparing(SourceFile::relPath));
        return files;
    }

    private static void collectJavaFiles(Path root, Path scanRoot, List<SourceFile> out) {
        try (var paths = Files.walk(scanRoot)) {
            paths
                .filter(Files::isRegularFile)
                .filter(path -> path.toString().endsWith(".java"))
                .forEach(path -> out.add(new SourceFile(relativePath(root, path), path)));
        } catch (IOException ignored) {
            // Missing or unreadable source roots are not implication lift errors.
        }
    }

    private static String relativePath(Path root, Path path) {
        Path rel = root.relativize(path.toAbsolutePath().normalize());
        return rel.toString().replace('\\', '/');
    }

    private static CompilationUnit parseSource(Path path) {
        try {
            ParseResult<CompilationUnit> result = new JavaParser().parse(path);
            return result.isSuccessful() && result.getResult().isPresent()
                ? result.getResult().get()
                : null;
        } catch (IOException ignored) {
            return null;
        }
    }

    private static List<Callsite> collectCallsites(CompilationUnit cu, String relPath) {
        List<Callsite> callsites = new ArrayList<>();
        for (MethodDeclaration method : cu.findAll(MethodDeclaration.class)) {
            if (isTestMethod(method) || method.getBody().isEmpty()) continue;
            for (MethodCallExpr call : method.getBody().get().findAll(MethodCallExpr.class)) {
                call.getName().getRange().ifPresent(range ->
                    callsites.add(new Callsite(
                        call.getNameAsString(),
                        relPath,
                        range.begin.line,
                        range.begin.column
                    ))
                );
            }
        }
        return callsites;
    }

    private static boolean isTestMethod(MethodDeclaration method) {
        for (AnnotationExpr ann : method.getAnnotations()) {
            String name = ann.getNameAsString();
            if (name.equals("Test")
                    || name.equals("RepeatedTest")
                    || name.equals("ParameterizedTest")
                    || name.endsWith(".Test")
                    || name.endsWith(".ParameterizedTest")) {
                return true;
            }
        }
        return method.getNameAsString().startsWith("test");
    }

    private static Jcs.Obj bridge(Callsite callsite, String targetCid) {
        return Jcs.object(
            "callsite", Jcs.object(
                "file", Jcs.string(callsite.file()),
                "start_col", Jcs.integer(callsite.col()),
                "start_line", Jcs.integer(callsite.line())
            ),
            "kind", Jcs.string("bridge"),
            "name", Jcs.string("intra-body:java:" + callsite.callee() + "@"
                + callsite.file() + ":" + callsite.line() + ":" + callsite.col()),
            "schemaVersion", Jcs.string("1"),
            "sourceContractCid", Jcs.string(targetCid),
            "sourceLayer", Jcs.string("java"),
            "sourceSymbol", Jcs.string(callsite.callee()),
            "target", Jcs.object("cid", Jcs.string(targetCid), "kind", Jcs.string("contract")),
            "targetContractCid", Jcs.string(targetCid),
            "targetLayer", Jcs.string("java-tests")
        );
    }

    private static Jcs.Obj liftGap(String reason, Callsite callsite) {
        return Jcs.object(
            "callee", Jcs.string(callsite.callee()),
            "col", Jcs.integer(callsite.col()),
            "file", Jcs.string(callsite.file()),
            "kind", Jcs.string("lift-gap"),
            "line", Jcs.integer(callsite.line()),
            "reason", Jcs.string(reason)
        );
    }

    private static String orDefault(String value, String fallback) {
        return value == null || value.isBlank() ? fallback : value;
    }
}
