package com.provekit.lift.cofoja;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

public class CofojaExtractor implements Extractor {
    public String name() { return "cofoja"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        for (TypeDeclaration<?> type : cu.getTypes()) {
            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof MethodDeclaration m) extractMethod(m, out);
            }
        }
        return out;
    }

    private void extractMethod(MethodDeclaration method, List<ContractDecl> out) {
        String symbol = method.getNameAsString();
        List<String> pres = new ArrayList<>(), posts = new ArrayList<>(), invs = new ArrayList<>();
        for (AnnotationExpr ann : method.getAnnotations()) {
            String name = simpleName(ann.getNameAsString());
            switch (name) {
                case "Requires" -> extractString(ann).ifPresent(s -> pres.add(toIr(s)));
                case "Ensures" -> extractString(ann).ifPresent(s -> posts.add(toIr(s)));
                case "Invariant" -> extractString(ann).ifPresent(s -> invs.add(toIr(s)));
            }
        }
        if (!pres.isEmpty()||!posts.isEmpty()||!invs.isEmpty()) {
            out.add(new ContractDecl(symbol, pres, posts, invs));
        }
    }

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.');
        return dot >= 0 ? fq.substring(dot + 1) : fq;
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
        String e = expr.trim();
        return "{\"kind\":\"atomic\",\"name\":\"cofoja_predicate\",\"args\":[{\"kind\":\"const\",\"value\":\""+esc(e)+"\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}]}";
    }

    private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }
}
