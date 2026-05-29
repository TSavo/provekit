package com.provekit.lift;

import java.util.*;

import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.github.javaparser.ast.expr.MethodCallExpr;
import com.provekit.ir.CallEdgeDecl;

/**
 * Same-language Java call-edge resolver for the LSP parse path.
 *
 * JNI edges are handled by {@link JniResolver}; this resolver handles ordinary
 * Java method calls where both caller and callee have lifted contracts in the
 * current compilation unit.
 */
public final class JavaCallEdgeResolver {
    private JavaCallEdgeResolver() {}

    public static List<CallEdgeDecl> resolve(
            CompilationUnit cu,
            String path,
            Map<String, String> contractIndex) {
        if (contractIndex.isEmpty()) return List.of();

        List<CallEdgeDecl> edges = new ArrayList<>();
        Set<String> seen = new LinkedHashSet<>();

        for (MethodDeclaration caller : cu.findAll(MethodDeclaration.class)) {
            if (caller.getBody().isEmpty()) continue;

            String callerSymbol = caller.getNameAsString();
            String sourceCid = contractIndex.get(callerSymbol);
            if (sourceCid == null || sourceCid.isBlank()) continue;

            for (MethodCallExpr call : caller.getBody().get().findAll(MethodCallExpr.class)) {
                String targetSymbol = call.getNameAsString();
                if (targetSymbol.equals(callerSymbol)) continue;

                String targetCid = contractIndex.get(targetSymbol);
                if (targetCid == null || targetCid.isBlank()) continue;

                int line = call.getBegin().map(pos -> pos.line).orElse(0);
                int column = call.getBegin().map(pos -> pos.column).orElse(0);
                String key = sourceCid + "\u0000" + targetCid + "\u0000" + line + "\u0000" + column;
                if (!seen.add(key)) continue;

                edges.add(new CallEdgeDecl(
                    sourceCid,
                    targetCid,
                    "java-kit:" + targetSymbol,
                    path,
                    line,
                    column,
                    "{\"args\":[],\"kind\":\"atomic\",\"name\":\"call-site-obligation\"}"
                ));
            }
        }

        return edges;
    }
}
