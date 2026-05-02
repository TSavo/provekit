package com.provekit.lift;

import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.ServiceLoader;
import com.github.javaparser.*;
import com.github.javaparser.ast.*;

public class LiftHandler {
    private final List<Extractor> extractors = new ArrayList<>();

    public LiftHandler() {
        ServiceLoader.load(Extractor.class).forEach(extractors::add);
    }

    public String lift(String workspace, String surface) {
        List<ContractDecl> decls = new ArrayList<>();
        Path root = Paths.get(workspace);
        try {
            Files.walk(root)
                .filter(p -> p.toString().endsWith(".java"))
                .forEach(p -> scanFile(p, decls));
        } catch (IOException e) {
            System.err.println("Walk error: " + e.getMessage());
        }

        StringBuilder ir = new StringBuilder();
        ir.append("{\"kind\":\"ir-document\",\"ir\":[");
        for (int i = 0; i < decls.size(); i++) {
            if (i > 0) ir.append(",");
            ir.append(decls.get(i).toJson());
        }
        ir.append("],\"diagnostics\":[]}");
        return ir.toString();
    }

    private void scanFile(Path path, List<ContractDecl> decls) {
        try {
            String source = Files.readString(path);
            ParseResult<CompilationUnit> result = new JavaParser().parse(source);
            if (!result.isSuccessful() || result.getResult().isEmpty()) return;
            CompilationUnit cu = result.getResult().get();
            for (Extractor ex : extractors) {
                decls.addAll(ex.extract(cu, source));
            }
        } catch (IOException e) {
            System.err.println("Parse error " + path + ": " + e.getMessage());
        }
    }
}
