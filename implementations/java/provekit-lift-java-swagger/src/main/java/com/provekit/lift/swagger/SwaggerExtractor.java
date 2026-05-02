package com.provekit.lift.swagger;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

/**
 * Extracts API contracts from Swagger/OpenAPI annotations.
 *
 * Coq-relevant contracts:
 *   - @ApiResponse(code=200, response=User.class) → status-code postcondition
 *   - @ApiParam(required=true) → non-null precondition on parameter
 *   - @Schema(minimum="0", maximum="100") → numeric range precondition
 *   - @ApiOperation(value="...", notes="...") → metadata (non-normative)
 *
 * When combined with a Coq model of HTTP, @ApiResponse becomes a
 * disjunctive postcondition: status=200 ∧ result:User  ∨  status=404.
 */
public class SwaggerExtractor implements Extractor {
    public String name() { return "swagger"; }

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
        List<String> posts = new ArrayList<>();
        List<String> responses = new ArrayList<>();

        for (AnnotationExpr ann : method.getAnnotations()) {
            String name = simpleName(ann.getNameAsString());
            switch (name) {
                case "ApiOperation","Operation" -> {
                    // Metadata — non-normative, but valuable for documentation
                    extractString(ann, "value").ifPresent(v ->
                        pres.add(metaAtom("api_description", v)));
                    extractString(ann, "notes").ifPresent(v ->
                        pres.add(metaAtom("api_notes", v)));
                }
                case "ApiResponses","ApiResponse" -> {
                    if ("ApiResponses".equals(name)) {
                        responses.addAll(extractApiResponses(ann));
                    } else {
                        extractApiResponse(ann).ifPresent(responses::add);
                    }
                }
            }
        }

        // Parameter-level annotations
        for (Parameter param : method.getParameters()) {
            String var = param.getNameAsString();
            for (AnnotationExpr ann : param.getAnnotations()) {
                String pname = simpleName(ann.getNameAsString());
                switch (pname) {
                    case "ApiParam","Parameter" -> {
                        boolean required = extractBoolean(ann, "required").orElse(false);
                        if (required) pres.add(atom("neq", var(var), cNull()));
                        extractString(ann, "name").ifPresent(n ->
                            pres.add(metaAtom("param_name", n)));
                        extractString(ann, "description").ifPresent(d ->
                            pres.add(metaAtom("param_desc", d)));
                    }
                    case "Schema" -> {
                        extractString(ann, "minimum").ifPresent(min ->
                            pres.add(atom("gte", var(var), cReal(Double.parseDouble(min)))));
                        extractString(ann, "maximum").ifPresent(max ->
                            pres.add(atom("lte", var(var), cReal(Double.parseDouble(max)))));
                        extractBoolean(ann, "required").ifPresent(req -> {
                            if (req) pres.add(atom("neq", var(var), cNull()));
                        });
                        extractString(ann, "pattern").ifPresent(pat ->
                            pres.add(atom("matches", var(var), cStr(pat))));
                    }
                }
            }
        }

        // Build disjunctive postcondition from response codes
        if (!responses.isEmpty()) {
            if (responses.size() == 1) {
                posts.add(responses.get(0));
            } else {
                posts.add(buildOr(responses));
            }
        }

        if (!pres.isEmpty() || !posts.isEmpty()) {
            out.add(new ContractDecl(symbol, pres, posts));
        }
    }

    private List<String> extractApiResponses(AnnotationExpr ann) {
        List<String> result = new ArrayList<>();
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if ("value".equals(p.getNameAsString()) && p.getValue() instanceof ArrayInitializerExpr aie) {
                    for (Expression e : aie.getValues()) {
                        if (e instanceof NormalAnnotationExpr ne) {
                            extractResponse(ne).ifPresent(result::add);
                        }
                    }
                }
            }
        }
        return result;
    }

    private Optional<String> extractApiResponse(AnnotationExpr ann) {
        return extractResponse(ann);
    }

    private Optional<String> extractResponse(AnnotationExpr ann) {
        if (!(ann instanceof NormalAnnotationExpr na)) return Optional.empty();
        Integer code = null;
        String responseType = null;
        for (MemberValuePair p : na.getPairs()) {
            switch (p.getNameAsString()) {
                case "code" -> {
                    Expression e = p.getValue();
                    if (e instanceof IntegerLiteralExpr ile) code = ile.asNumber().intValue();
                }
                case "response" -> {
                    Expression e = p.getValue();
                    if (e instanceof ClassExpr ce) responseType = ce.getType().asString();
                }
            }
        }
        if (code == null) return Optional.empty();
        StringBuilder sb = new StringBuilder();
        sb.append("{\"kind\":\"atomic\",\"name\":\"http_response\",\"args\":[")
          .append("{\"kind\":\"const\",\"value\":").append(code)
          .append(",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}");
        if (responseType != null) {
            sb.append(",{\"kind\":\"const\",\"value\":\"").append(esc(responseType))
              .append("\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}");
        }
        sb.append("]}");
        return Optional.of(sb.toString());
    }

    private Optional<String> extractString(AnnotationExpr ann, String key) {
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

    private Optional<Boolean> extractBoolean(AnnotationExpr ann, String key) {
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

    private String buildOr(List<String> operands) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"or\",\"operands\":[");
        for (int i=0;i<operands.size();i++) {
            if (i>0) sb.append(",");
            sb.append(operands.get(i));
        }
        sb.append("]}"); return sb.toString();
    }

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.'); return dot >= 0 ? fq.substring(dot + 1) : fq;
    }
    private String var(String n) { return "{\"kind\":\"var\",\"name\":\""+n+"\"}"; }
    private String cNull() { return "{\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}}"; }
    private String cReal(double v) { return "{\"kind\":\"const\",\"value\":"+v+",\"sort\":{\"kind\":\"primitive\",\"name\":\"Real\"}}"; }
    private String cStr(String s) { return "{\"kind\":\"const\",\"value\":\""+esc(s)+"\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}"; }
    private String atom(String name, String... args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\""+name+"\",\"args\":[");
        for (int i=0;i<args.length;i++) { if(i>0) sb.append(","); sb.append(args[i]); }
        sb.append("]}"); return sb.toString();
    }
    private String metaAtom(String name, String value) {
        return "{\"kind\":\"atomic\",\"name\":\""+name+"\",\"args\":[{\"kind\":\"const\",\"value\":\""+esc(value)+"\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}]}";
    }
    private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }
}
