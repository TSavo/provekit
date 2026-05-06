package com.provekit.lift.jml;

import java.util.*;
import java.util.regex.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.provekit.lift.*;

public class JmlExtractor implements Extractor {
    private static final Pattern REQUIRES = Pattern.compile("//@\\s*requires\\s+(.+)");
    private static final Pattern ENSURES = Pattern.compile("//@\\s*ensures\\s+(.+)");
    private static final Pattern INVARIANT = Pattern.compile("//@\\s*invariant\\s+(.+)");

    public String name() { return "jml"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        String[] lines = rawSource.split("\n");
        Map<Integer, List<String>> pres = new HashMap<>(), posts = new HashMap<>(), invs = new HashMap<>();

        for (int i = 0; i < lines.length; i++) {
            String line = lines[i];
            Matcher m;
            if ((m = REQUIRES.matcher(line)).find()) pres.computeIfAbsent(i, k -> new ArrayList<>()).add(m.group(1));
            if ((m = ENSURES.matcher(line)).find()) posts.computeIfAbsent(i, k -> new ArrayList<>()).add(m.group(1));
            if ((m = INVARIANT.matcher(line)).find()) invs.computeIfAbsent(i, k -> new ArrayList<>()).add(m.group(1));
        }

        for (TypeDeclaration<?> type : cu.getTypes()) {
            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof MethodDeclaration method) {
                    int line = method.getBegin().map(p -> p.line).orElse(0);
                    String symbol = method.getNameAsString();
                    List<String> p = gather(line, pres), po = gather(line, posts), inv = gather(line, invs);
                    if (!p.isEmpty() || !po.isEmpty() || !inv.isEmpty()) {
                        out.add(new ContractDecl(symbol, p, po, inv));
                    }
                }
            }
        }
        return out;
    }

    private List<String> gather(int methodLine, Map<Integer, List<String>> map) {
        List<String> result = new ArrayList<>();
        for (int i = methodLine - 1; i >= Math.max(0, methodLine - 20); i--) {
            if (map.containsKey(i)) {
                for (String expr : map.get(i)) result.add(jmlToIr(expr));
                break;
            }
        }
        return result;
    }

    private String jmlToIr(String expr) {
        String normalized = expr.trim().replace("\\result", "result");
        return ContractExpressionParser.parseOrFallback(normalized, "jml_predicate");
    }
}
