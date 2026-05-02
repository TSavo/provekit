package com.provekit.lift.bean;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

public class BeanValidationExtractor implements Extractor {
    private static final Set<String> CONSTRAINTS = Set.of(
        "NotNull","NotEmpty","NotBlank","Email","Pattern","Size",
        "Min","Max","DecimalMin","DecimalMax","Digits",
        "Positive","Negative","PositiveOrZero","NegativeOrZero",
        "AssertTrue","AssertFalse","Future","Past","FutureOrPresent","PastOrPresent"
    );

    public String name() { return "bean-validation"; }

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
        List<String> pres = new ArrayList<>();
        for (Parameter param : method.getParameters()) {
            for (AnnotationExpr ann : param.getAnnotations()) {
                String name = simpleName(ann.getNameAsString());
                if (!CONSTRAINTS.contains(name)) continue;
                String cond = toIr(name, ann, param.getNameAsString());
                if (cond != null) pres.add(cond);
            }
        }
        if (!pres.isEmpty()) out.add(new ContractDecl(symbol, pres, List.of()));
    }

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.');
        return dot >= 0 ? fq.substring(dot + 1) : fq;
    }

    private String toIr(String name, AnnotationExpr ann, String var) {
        return switch (name) {
            case "NotNull" -> atom("neq", var(""+var), cNull());
            case "NotEmpty","NotBlank" -> atom("gt", ctor("strlen", var(var)), cInt(0));
            case "Email" -> atom("matches", var(var), cStr("^[^@]+@[^@]+$"));
            case "Positive" -> atom("gt", var(var), cInt(0));
            case "Negative" -> atom("lt", var(var), cInt(0));
            case "PositiveOrZero" -> atom("gte", var(var), cInt(0));
            case "NegativeOrZero" -> atom("lte", var(var), cInt(0));
            case "AssertTrue" -> atom("eq", var(var), cBool(true));
            case "AssertFalse" -> atom("eq", var(var), cBool(false));
            case "Min" -> atom("gte", var(var), cInt(extractLong(ann, "value")));
            case "Max" -> atom("lte", var(var), cInt(extractLong(ann, "value")));
            case "Size" -> {
                long min = extractLong(ann, "min"), max = extractLong(ann, "max");
                yield and(atom("gte", ctor("strlen", var(var)), cInt(min)),
                          atom("lte", ctor("strlen", var(var)), cInt(max)));
            }
            default -> null;
        };
    }

    private long extractLong(AnnotationExpr ann, String key) {
        if (ann instanceof SingleMemberAnnotationExpr sma) {
            Expression e = sma.getMemberValue();
            if (e instanceof IntegerLiteralExpr ile) return ile.asNumber().longValue();
        }
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals(key)) {
                    Expression e = p.getValue();
                    if (e instanceof IntegerLiteralExpr ile) return ile.asNumber().longValue();
                }
            }
        }
        return 0;
    }

    private String var(String n) { return "{\"kind\":\"var\",\"name\":\""+n+"\"}"; }
    private String cInt(long v) { return "{\"kind\":\"const\",\"value\":"+v+",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"; }
    private String cStr(String s) { return "{\"kind\":\"const\",\"value\":\""+esc(s)+"\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}"; }
    private String cBool(boolean b) { return "{\"kind\":\"const\",\"value\":"+b+",\"sort\":{\"kind\":\"primitive\",\"name\":\"Bool\"}}"; }
    private String cNull() { return "{\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}}"; }
    private String ctor(String name, String arg) { return "{\"kind\":\"ctor\",\"name\":\""+name+"\",\"args\":["+arg+"]}"; }
    private String atom(String name, String... args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\""+name+"\",\"args\":[");
        for (int i=0;i<args.length;i++) { if(i>0) sb.append(","); sb.append(args[i]); }
        sb.append("]}"); return sb.toString();
    }
    private String and(String a, String b) { return "{\"kind\":\"and\",\"operands\":["+a+","+b+"]}"; }
    private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }
}
