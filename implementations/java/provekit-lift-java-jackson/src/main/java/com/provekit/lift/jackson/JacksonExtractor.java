package com.provekit.lift.jackson;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

/**
 * Extracts serialization contracts from Jackson annotations.
 *
 * Coq-relevant: round-trip properties and field presence invariants.
 * @JsonProperty("user_name") on field 'username' →
 *   invariant: serialize(obj)["user_name"] = obj.username
 * @JsonIgnore → field absent from serialized output
 * @JsonFormat(pattern="yyyy-MM-dd") → string format precondition
 */
public class JacksonExtractor implements Extractor {
    public String name() { return "jackson"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        for (TypeDeclaration<?> type : cu.getTypes()) {
            String className = type.getNameAsString();
            List<String> invariants = new ArrayList<>();

            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof FieldDeclaration field) {
                    for (VariableDeclarator var : field.getVariables()) {
                        String fieldName = var.getNameAsString();
                        String jsonKey = fieldName;
                        boolean ignored = false;
                        String format = null;

                        for (AnnotationExpr ann : field.getAnnotations()) {
                            String name = simpleName(ann.getNameAsString());
                            switch (name) {
                                case "JsonProperty" -> {
                                    Optional<String> val = extractStringValue(ann);
                                    if (val.isPresent()) jsonKey = val.get();
                                }
                                case "JsonIgnore","JsonIgnoreProperties" -> ignored = true;
                                case "JsonFormat" -> format = extractStringValue(ann, "pattern").orElse(null);
                            }
                        }

                        if (ignored) {
                            invariants.add(atom("json_absent", cStr(className), cStr(fieldName)));
                        } else {
                            invariants.add(atom("json_key", cStr(className), cStr(fieldName), cStr(jsonKey)));
                        }
                        if (format != null) {
                            invariants.add(atom("json_format", cStr(className), cStr(fieldName), cStr(format)));
                        }
                    }
                }
            }

            if (!invariants.isEmpty()) {
                out.add(new ContractDecl(className, List.of(), List.of(), invariants));
            }
        }
        return out;
    }

    private Optional<String> extractStringValue(AnnotationExpr ann) {
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

    private Optional<String> extractStringValue(AnnotationExpr ann, String key) {
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

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.'); return dot >= 0 ? fq.substring(dot + 1) : fq;
    }
    private String cStr(String s) { return "{\"kind\":\"const\",\"value\":\""+esc(s)+"\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}"; }
    private String atom(String name, String... args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\""+name+"\",\"args\":[");
        for (int i=0;i<args.length;i++) { if(i>0) sb.append(","); sb.append(args[i]); }
        sb.append("]}"); return sb.toString();
    }
    private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }
}
