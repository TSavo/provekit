package com.provekit.lift.provekitnative;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

public class ProvekitNativeExtractor implements Extractor {
    private static final String PACKAGE_NAME = "com.provekit.contract";
    private static final Set<String> ANNOTATIONS = Set.of("Requires", "Ensures", "Invariant", "NotNull");
    private static final Set<String> COMPETING_PACKAGES = Set.of("com.google.java.contract");

    public String name() { return "provekit-native"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        for (TypeDeclaration<?> type : cu.getTypes()) {
            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof MethodDeclaration method) extractMethod(cu, method, out);
            }
        }
        return out;
    }

    private void extractMethod(CompilationUnit cu, MethodDeclaration method, List<ContractDecl> out) {
        String symbol = method.getNameAsString();
        List<String> pres = new ArrayList<>(), posts = new ArrayList<>(), invs = new ArrayList<>();
        for (AnnotationExpr ann : method.getAnnotations()) {
            if (!AnnotationSupport.belongsToFamily(cu, ann, PACKAGE_NAME, ANNOTATIONS, COMPETING_PACKAGES)) continue;
            switch (simpleName(ann.getNameAsString())) {
                case "Requires" -> extractString(ann).ifPresent(s -> pres.add(toIr(s)));
                case "Ensures" -> extractString(ann).ifPresent(s -> posts.add(toIr(s)));
                case "Invariant" -> extractString(ann).ifPresent(s -> invs.add(toIr(s)));
            }
        }
        for (Parameter param : method.getParameters()) {
            for (AnnotationExpr ann : param.getAnnotations()) {
                if (!AnnotationSupport.belongsToFamily(cu, ann, PACKAGE_NAME, ANNOTATIONS, COMPETING_PACKAGES)) continue;
                if (simpleName(ann.getNameAsString()).equals("NotNull")) {
                    pres.add(nonNullIr(param.getNameAsString()));
                }
            }
        }
        if (!pres.isEmpty() || !posts.isEmpty() || !invs.isEmpty()) {
            out.add(new ContractDecl(symbol, pres, posts, invs));
        }
    }

    private Optional<String> extractString(AnnotationExpr ann) {
        if (ann instanceof SingleMemberAnnotationExpr sma) {
            Expression e = sma.getMemberValue();
            if (e instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
        }
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals("value")) {
                    Expression e = p.getValue();
                    if (e instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
                }
            }
        }
        return Optional.empty();
    }

    private String toIr(String expr) {
        return ContractExpressionParser.parseOrFallback(expr, "provekit_native_predicate");
    }

    private String nonNullIr(String varName) {
        return "{\"kind\":\"atomic\",\"name\":\"neq\",\"args\":[{\"kind\":\"var\",\"name\":\""
            + escape(varName)
            + "\"},{\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}}]}";
    }

    private String escape(String value) {
        return value.replace("\\", "\\\\").replace("\"", "\\\"");
    }

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.');
        return dot >= 0 ? fq.substring(dot + 1) : fq;
    }
}
