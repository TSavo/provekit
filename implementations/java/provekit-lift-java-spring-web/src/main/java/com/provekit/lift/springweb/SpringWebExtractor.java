package com.provekit.lift.springweb;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

/**
 * Extracts runtime HTTP contracts from Spring Web annotations.
 *
 * Coq-relevant contracts (runtime values, state):
 *   - @RequestParam(required=true/false) → param presence precondition
 *   - @RequestParam(defaultValue=...) → param value witness for the defaulted branch
 *   - @PathVariable → path segment extraction (non-null precondition)
 *   - @RequestBody → body deserialization precondition
 *   - @ResponseStatus → response code postcondition
 *   - @RequestMapping(method=...) → HTTP method precondition
 *
 * Static metadata (non-normative, attached as evidence):
 *   - path patterns, consumes/produces media types
 */
public class SpringWebExtractor implements Extractor {
    public String name() { return "spring-web"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        for (TypeDeclaration<?> type : cu.getTypes()) {
            String basePath = extractBasePath(type);
            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof MethodDeclaration m) {
                    extractMethod(m, basePath, out);
                }
            }
        }
        return out;
    }

    private String extractBasePath(TypeDeclaration<?> type) {
        for (AnnotationExpr ann : type.getAnnotations()) {
            String name = simpleName(ann.getNameAsString());
            if ("RequestMapping".equals(name)) {
                return extractStringValue(ann, "value").orElse("");
            }
        }
        return "";
    }

    private void extractMethod(MethodDeclaration method, String basePath, List<ContractDecl> out) {
        String symbol = method.getNameAsString();
        List<String> pres = new ArrayList<>();
        List<String> posts = new ArrayList<>();
        List<String> invs = new ArrayList<>();

        // Handle method-level mapping annotations
        for (AnnotationExpr ann : method.getAnnotations()) {
            String name = simpleName(ann.getNameAsString());
            switch (name) {
                case "RequestMapping","GetMapping","PostMapping","PutMapping","DeleteMapping","PatchMapping" -> {
                    String path = basePath + extractStringValue(ann, "value").orElse("");
                    String methodStr = switch(name) {
                        case "GetMapping" -> "GET"; case "PostMapping" -> "POST";
                        case "PutMapping" -> "PUT"; case "DeleteMapping" -> "DELETE";
                        case "PatchMapping" -> "PATCH"; default -> "ANY";
                    };
                    pres.add(atom("http_method", cStr(methodStr)));
                    pres.add(atom("http_path", cStr(path)));
                }
                case "ResponseStatus" -> {
                    int code = extractIntValue(ann, "code").orElse(
                        extractIntValue(ann, "value").orElse(200));
                    posts.add(atom("http_status", cInt(code)));
                }
            }
        }

        // Handle parameter annotations (runtime value contracts)
        for (Parameter param : method.getParameters()) {
            String var = param.getNameAsString();
            for (AnnotationExpr ann : param.getAnnotations()) {
                String pname = simpleName(ann.getNameAsString());
                switch (pname) {
                    case "PathVariable" -> pres.add(atom("neq", var(var), cNull()));
                    case "RequestParam" -> {
                        Optional<String> defaultValue = extractStringValue(ann, "defaultValue");
                        if (defaultValue.isPresent()) {
                            invs.add(atom("eq", var(var), defaultValueTerm(param, defaultValue.get())));
                            break;
                        }
                        boolean required = extractBooleanValue(ann, "required").orElse(true);
                        if (required) pres.add(atom("neq", var(var), cNull()));
                    }
                    case "RequestBody" -> pres.add(atom("deserializable", var(var)));
                    case "RequestHeader" -> {
                        boolean required = extractBooleanValue(ann, "required").orElse(true);
                        if (required) pres.add(atom("neq", var(var), cNull()));
                    }
                }
            }
        }

        if (!pres.isEmpty() || !posts.isEmpty() || !invs.isEmpty()) {
            out.add(new ContractDecl(symbol, pres, posts, invs));
        }
    }

    private Optional<String> extractStringValue(AnnotationExpr ann, String key) {
        if (ann instanceof SingleMemberAnnotationExpr sma) {
            Expression e = sma.getMemberValue();
            if (e instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
            if (e instanceof ArrayInitializerExpr aie && !aie.getValues().isEmpty()) {
                Expression first = aie.getValues().get(0);
                if (first instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
            }
        }
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals(key)) {
                    Expression e = p.getValue();
                    if (e instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
                }
            }
        }
        return Optional.empty();
    }

    private Optional<Integer> extractIntValue(AnnotationExpr ann, String key) {
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals(key)) {
                    Expression e = p.getValue();
                    if (e instanceof FieldAccessExpr fae) {
                        // e.g. HttpStatus.CREATED
                        String statusName = fae.getNameAsString();
                        return Optional.of(httpStatusCode(statusName));
                    }
                    if (e instanceof IntegerLiteralExpr ile) return Optional.of(ile.asNumber().intValue());
                }
            }
        }
        return Optional.empty();
    }

    private Optional<Boolean> extractBooleanValue(AnnotationExpr ann, String key) {
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals(key)) {
                    Expression e = p.getValue();
                    if (e instanceof BooleanLiteralExpr ble) return Optional.of(ble.getValue());
                }
            }
        }
        return Optional.empty();
    }

    private int httpStatusCode(String name) {
        return switch(name) {
            case "OK" -> 200; case "CREATED" -> 201; case "ACCEPTED" -> 202;
            case "NO_CONTENT" -> 204; case "BAD_REQUEST" -> 400;
            case "UNAUTHORIZED" -> 401; case "FORBIDDEN" -> 403;
            case "NOT_FOUND" -> 404; case "CONFLICT" -> 409;
            case "INTERNAL_SERVER_ERROR" -> 500; case "SERVICE_UNAVAILABLE" -> 503;
            default -> 200;
        };
    }

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.'); return dot >= 0 ? fq.substring(dot + 1) : fq;
    }
    private String var(String n) { return "{\"kind\":\"var\",\"name\":\""+n+"\"}"; }
    private String defaultValueTerm(Parameter param, String raw) {
        String type = param.getType().asString();
        if (type.equals("int") || type.equals("Integer") || type.equals("long") || type.equals("Long")) {
            try {
                return cInt(Long.parseLong(raw));
            } catch (NumberFormatException ignored) {
                return cStr(raw);
            }
        }
        if (type.equals("boolean") || type.equals("Boolean")) {
            if (raw.equals("true") || raw.equals("false")) {
                return cBool(Boolean.parseBoolean(raw));
            }
        }
        return cStr(raw);
    }

    private String cInt(long v) { return "{\"kind\":\"const\",\"value\":"+v+",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"; }
    private String cStr(String s) { return "{\"kind\":\"const\",\"value\":\""+esc(s)+"\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}"; }
    private String cBool(boolean b) { return "{\"kind\":\"const\",\"value\":"+b+",\"sort\":{\"kind\":\"primitive\",\"name\":\"Bool\"}}"; }
    private String cNull() { return "{\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}}"; }
    private String atom(String name, String... args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\""+name+"\",\"args\":[");
        for (int i=0;i<args.length;i++) { if(i>0) sb.append(","); sb.append(args[i]); }
        sb.append("]}"); return sb.toString();
    }
    private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }
}
