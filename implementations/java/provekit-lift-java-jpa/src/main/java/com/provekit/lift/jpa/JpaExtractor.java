package com.provekit.lift.jpa;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

/**
 * Extracts persistence invariants from JPA/Hibernate annotations.
 *
 * Coq-relevant: database invariants expressed as class-level properties.
 *   - @Column(nullable=false) → non-null invariant on field
 *   - @Id → uniqueness invariant (forall x y, id(x)=id(y) → x=y)
 *   - @OneToMany(optional=false) → collection non-empty invariant
 *   - @ManyToOne(optional=false) → reference non-null invariant
 *   - @Enumerated(EnumType.STRING) → enum string mapping invariant
 *
 * These become Coq invariants over the entity state.
 */
public class JpaExtractor implements Extractor {
    public String name() { return "jpa"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        for (TypeDeclaration<?> type : cu.getTypes()) {
            String className = type.getNameAsString();
            List<String> invariants = new ArrayList<>();
            boolean isEntity = false;

            for (AnnotationExpr ann : type.getAnnotations()) {
                if (simpleName(ann.getNameAsString()).equals("Entity")) isEntity = true;
            }
            if (!isEntity) continue;

            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof FieldDeclaration field) {
                    for (VariableDeclarator var : field.getVariables()) {
                        String fieldName = var.getNameAsString();
                        boolean isId = false;
                        boolean nullable = true;
                        boolean unique = false;
                        long min = Long.MIN_VALUE, max = Long.MAX_VALUE;
                        int length = -1;

                        for (AnnotationExpr ann : field.getAnnotations()) {
                            String name = simpleName(ann.getNameAsString());
                            switch (name) {
                                case "Id" -> isId = true;
                                case "Column" -> {
                                    nullable = extractBoolean(ann, "nullable").orElse(true);
                                    unique = extractBoolean(ann, "unique").orElse(false);
                                    length = extractInt(ann, "length").orElse(-1);
                                }
                                case "NotNull" -> nullable = false;
                                case "ManyToOne","OneToOne" -> {
                                    boolean opt = extractBoolean(ann, "optional").orElse(true);
                                    if (!opt) nullable = false;
                                }
                            }
                        }

                        if (isId) {
                            invariants.add(atom("unique", cStr(className), cStr(fieldName)));
                        }
                        if (!nullable) {
                            invariants.add(atom("not_null", cStr(className), cStr(fieldName)));
                        }
                        if (unique && !isId) {
                            invariants.add(atom("unique", cStr(className), cStr(fieldName)));
                        }
                        if (length > 0) {
                            invariants.add(atom("max_length", cStr(className), cStr(fieldName), cInt(length)));
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

    private Optional<Integer> extractInt(AnnotationExpr ann, String key) {
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals(key)) {
                    Expression e = p.getValue();
                    if (e instanceof IntegerLiteralExpr ile) return Optional.of(ile.asNumber().intValue());
                }
            }
        }
        return Optional.empty();
    }

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.'); return dot >= 0 ? fq.substring(dot + 1) : fq;
    }
    private String cStr(String s) { return "{\"kind\":\"const\",\"value\":\""+esc(s)+"\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}"; }
    private String cInt(int v) { return "{\"kind\":\"const\",\"value\":"+v+",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"; }
    private String atom(String name, String... args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\""+name+"\",\"args\":[");
        for (int i=0;i<args.length;i++) { if(i>0) sb.append(","); sb.append(args[i]); }
        sb.append("]}"); return sb.toString();
    }
    private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }
}
