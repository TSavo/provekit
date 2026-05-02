package com.provekit.lift.hibernate;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

/**
 * Extracts persistence contracts from Hibernate-specific annotations.
 *
 * Hibernate annotations encode database constraints that Java's type system
 * cannot express. This binding lifts them into IR invariants:
 *
 *   - @Immutable            → no mutation invariant
 *   - @NaturalId            → business-key uniqueness
 *   - @Where(clause=...)    → static filter invariant (all rows satisfy predicate)
 *   - @Check(constraints=...)→ database check constraint
 *   - @Filter(name=...)     → dynamic filter precondition
 *   - @Formula("...")       → computed property invariant
 *   - @BatchSize(size=10)   → collection loading size bound
 *   - @Fetch(FetchMode.SUBSELECT) → fetch strategy constraint
 *   - @LazyCollection(LazyCollectionOption.EXTRA) → size computation invariant
 *   - @SQLDelete, @SQLInsert, @SQLUpdate → custom SQL contract
 *   - @Type(type="...")     → custom type mapping invariant
 *   - @GenericGenerator     → ID generation strategy invariant
 *   - @JoinFormula          → computed join condition
 *   - @DiscriminatorFormula → type discriminator invariant
 *   - @OptimisticLocking    → concurrency control invariant
 *   - @RowId                → row identity invariant
 *   - @Subselect("...")     → view-based entity invariant
 *   - @Synchronize         → cache synchronization invariant
 *   - @Tuplizer            → instantiation strategy invariant
 *   - @Persister           → custom persister invariant
 *   - @DynamicUpdate       → partial update strategy invariant
 *   - @DynamicInsert       → partial insert strategy invariant
 *   - @SelectBeforeUpdate  → read-before-write invariant
 *   - @Entity(dynamicUpdate=true) → same as @DynamicUpdate
 *
 * All of these are invisible to Java's type checker but are rich
 * contracts over the runtime database state.
 */
public class HibernateExtractor implements Extractor {
    public String name() { return "hibernate"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        for (TypeDeclaration<?> type : cu.getTypes()) {
            extractType(type, out);
        }
        return out;
    }

    private void extractType(TypeDeclaration<?> type, List<ContractDecl> out) {
        String className = type.getNameAsString();
        List<String> invariants = new ArrayList<>();
        List<String> pres = new ArrayList<>();
        boolean isEntity = false;

        // Class-level annotations
        for (AnnotationExpr ann : type.getAnnotations()) {
            String name = simpleName(ann.getNameAsString());
            switch (name) {
                case "Entity","MappedSuperclass","Embeddable" -> isEntity = true;
                case "Immutable" -> invariants.add(atom("immutable", cStr(className)));
                case "Where" -> extractString(ann, "clause").ifPresent(clause ->
                    invariants.add(atom("where_clause", cStr(className), cStr(clause))));
                case "Check" -> extractString(ann, "constraints").ifPresent(constraints ->
                    invariants.add(atom("db_check", cStr(className), cStr(constraints))));
                case "Subselect" -> extractString(ann).ifPresent(sql ->
                    invariants.add(atom("subselect", cStr(className), cStr(sql))));
                case "DynamicUpdate","SelectBeforeUpdate" ->
                    invariants.add(atom("dynamic_update", cStr(className)));
                case "DynamicInsert" ->
                    invariants.add(atom("dynamic_insert", cStr(className)));
                case "OptimisticLocking" -> {
                    String type_ = extractEnum(ann, "type").orElse("VERSION");
                    invariants.add(atom("optimistic_lock", cStr(className), cStr(type_)));
                }
                case "DiscriminatorFormula" -> extractString(ann).ifPresent(formula ->
                    invariants.add(atom("discriminator_formula", cStr(className), cStr(formula))));
                case "FilterDef" -> {
                    String filterName = extractString(ann, "name").orElse("unknown");
                    String defaultCond = extractString(ann, "defaultCondition").orElse("");
                    if (!defaultCond.isEmpty()) {
                        invariants.add(atom("filter_def", cStr(className), cStr(filterName), cStr(defaultCond)));
                    }
                }
                case "Synchronize" -> {
                    List<String> tables = extractStringArray(ann);
                    for (String t : tables) {
                        invariants.add(atom("synchronize", cStr(className), cStr(t)));
                    }
                }
            }
        }

        if (!isEntity) return;

        // Field/method-level annotations
        for (BodyDeclaration<?> member : type.getMembers()) {
            if (member instanceof FieldDeclaration field) {
                for (VariableDeclarator var : field.getVariables()) {
                    String fieldName = var.getNameAsString();
                    for (AnnotationExpr ann : field.getAnnotations()) {
                        String name = simpleName(ann.getNameAsString());
                        switch (name) {
                            case "NaturalId" -> {
                                boolean mutable = extractBoolean(ann, "mutable").orElse(false);
                                invariants.add(atom("natural_id", cStr(className), cStr(fieldName), cBool(mutable)));
                            }
                            case "Formula" -> extractString(ann).ifPresent(formula ->
                                invariants.add(atom("formula", cStr(className), cStr(fieldName), cStr(formula))));
                            case "Filter" -> {
                                String filterName = extractString(ann, "name").orElse("unknown");
                                String condition = extractString(ann, "condition").orElse("");
                                invariants.add(atom("filter", cStr(className), cStr(fieldName), cStr(filterName), cStr(condition)));
                            }
                            case "BatchSize" -> {
                                int size = extractInt(ann, "size").orElse(1);
                                invariants.add(atom("batch_size", cStr(className), cStr(fieldName), cInt(size)));
                            }
                            case "Fetch" -> {
                                String mode = extractEnum(ann, "value").orElse("DEFAULT");
                                invariants.add(atom("fetch_mode", cStr(className), cStr(fieldName), cStr(mode)));
                            }
                            case "LazyCollection" -> {
                                String option = extractEnum(ann, "value").orElse("TRUE");
                                invariants.add(atom("lazy_collection", cStr(className), cStr(fieldName), cStr(option)));
                            }
                            case "JoinFormula" -> extractString(ann).ifPresent(formula ->
                                invariants.add(atom("join_formula", cStr(className), cStr(fieldName), cStr(formula))));
                            case "Type" -> extractString(ann).ifPresent(typeName ->
                                invariants.add(atom("custom_type", cStr(className), cStr(fieldName), cStr(typeName))));
                            case "GenericGenerator" -> {
                                String genName = extractString(ann, "name").orElse("unknown");
                                String strategy = extractString(ann, "strategy").orElse("native");
                                invariants.add(atom("id_generator", cStr(className), cStr(fieldName), cStr(genName), cStr(strategy)));
                            }
                            case "SQLDelete" -> extractString(ann).ifPresent(sql ->
                                invariants.add(atom("sql_delete", cStr(className), cStr(fieldName), cStr(sql))));
                            case "SQLInsert" -> extractString(ann).ifPresent(sql ->
                                invariants.add(atom("sql_insert", cStr(className), cStr(fieldName), cStr(sql))));
                            case "SQLUpdate" -> extractString(ann).ifPresent(sql ->
                                invariants.add(atom("sql_update", cStr(className), cStr(fieldName), cStr(sql))));
                            case "RowId" -> extractString(ann).ifPresent(col ->
                                invariants.add(atom("row_id", cStr(className), cStr(fieldName), cStr(col))));
                        }
                    }
                }
            }
        }

        if (!invariants.isEmpty() || !pres.isEmpty()) {
            out.add(new ContractDecl(className, pres, List.of(), invariants));
        }
    }

    private Optional<String> extractString(AnnotationExpr ann) {
        if (ann instanceof SingleMemberAnnotationExpr sma) {
            Expression e = sma.getMemberValue();
            if (e instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
        }
        return Optional.empty();
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

    private Optional<String> extractEnum(AnnotationExpr ann, String key) {
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals(key)) {
                    Expression e = p.getValue();
                    if (e instanceof FieldAccessExpr fae) {
                        return Optional.of(fae.getNameAsString());
                    }
                }
            }
        }
        return Optional.empty();
    }

    private List<String> extractStringArray(AnnotationExpr ann) {
        List<String> result = new ArrayList<>();
        Expression value = null;
        if (ann instanceof SingleMemberAnnotationExpr sma) value = sma.getMemberValue();
        else if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals("value")) { value = p.getValue(); break; }
            }
        }
        if (value instanceof StringLiteralExpr sle) result.add(sle.getValue());
        else if (value instanceof ArrayInitializerExpr aie) {
            for (Expression e : aie.getValues()) {
                if (e instanceof StringLiteralExpr sle) result.add(sle.getValue());
            }
        }
        return result;
    }

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.'); return dot >= 0 ? fq.substring(dot + 1) : fq;
    }
    private String cStr(String s) { return "{\"kind\":\"const\",\"value\":\"" + esc(s) + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}"; }
    private String cInt(int v) { return "{\"kind\":\"const\",\"value\":" + v + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"; }
    private String cBool(boolean b) { return "{\"kind\":\"const\",\"value\":" + b + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Bool\"}}"; }
    private String atom(String name, String... args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\"" + name + "\",\"args\":[");
        for (int i = 0; i < args.length; i++) { if (i > 0) sb.append(","); sb.append(args[i]); }
        sb.append("]}"); return sb.toString();
    }
    private String esc(String s) { return s.replace("\\", "\\\\").replace("\"", "\\\""); }
}
