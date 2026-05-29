package com.provekit.lift.testng;

import java.util.*;

import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.ImportDeclaration;
import com.github.javaparser.ast.body.BodyDeclaration;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.github.javaparser.ast.body.TypeDeclaration;
import com.github.javaparser.ast.body.VariableDeclarator;
import com.github.javaparser.ast.expr.*;
import com.github.javaparser.ast.stmt.BlockStmt;
import com.github.javaparser.ast.stmt.IfStmt;
import com.github.javaparser.ast.stmt.Statement;
import com.provekit.lift.AnnotationSupport;
import com.provekit.lift.ContractDecl;
import com.provekit.lift.Extractor;

public class TestNgExtractor implements Extractor {
    private static final Set<String> TESTNG_TESTS = Set.of("Test");
    private static final Set<String> ASSERTIONS = Set.of(
        "assertEquals", "assertNotEquals", "assertTrue", "assertFalse"
    );

    public String name() { return "testng"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        String sourceFile = sourceFileName(cu);
        for (TypeDeclaration<?> type : cu.getTypes()) {
            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof MethodDeclaration method && isTestNgTest(cu, method)) {
                    extractMethod(cu, method, sourceFile, out);
                }
            }
        }
        return out;
    }

    private boolean isTestNgTest(CompilationUnit cu, MethodDeclaration method) {
        for (AnnotationExpr ann : method.getAnnotations()) {
            if (AnnotationSupport.belongsToFamily(
                    cu, ann, "org.testng.annotations", TESTNG_TESTS, Set.of())) {
                return true;
            }
        }
        return false;
    }

    private void extractMethod(
            CompilationUnit cu,
            MethodDeclaration method,
            String sourceFile,
            List<ContractDecl> out) {
        if (method.getBody().isEmpty()) return;

        Map<String, Integer> versions = new LinkedHashMap<>();
        List<ValueScope> scopes = List.of(new ValueScope());

        for (Statement stmt : method.getBody().get().getStatements()) {
            List<LiftedAssertion> assertions = liftAssertion(cu, stmt, scopes, sourceFile);
            for (LiftedAssertion assertion : assertions) {
                out.add(new ContractDecl(
                    assertion.symbol(),
                    List.of(),
                    List.of(),
                    List.of(assertion.formula())
                ));
            }
            if (!assertions.isEmpty()) continue;
            scopes = applyStatement(stmt, scopes, versions);
        }
    }

    private List<LiftedAssertion> liftAssertion(
            CompilationUnit cu,
            Statement stmt,
            List<ValueScope> scopes,
            String sourceFile) {
        if (!stmt.isExpressionStmt()) return List.of();
        Expression expr = stmt.asExpressionStmt().getExpression();
        if (!(expr instanceof MethodCallExpr call)) return List.of();
        if (!isTestNgAssertion(cu, call)) return List.of();

        String name = call.getNameAsString();
        Map<String, List<String>> scopedByCallsite = new LinkedHashMap<>();
        for (ValueScope scope : scopes) {
            Optional<ObservedCall> observedCall = observedCallForAssertion(name, call, scope);
            if (observedCall.isEmpty()) return List.of();
            Optional<String> symbol = callsiteSymbol(sourceFile, observedCall.get().call());
            if (symbol.isEmpty()) return List.of();
            ValueScope assertionScope = scope.withTermOverrides(observedCall.get().termOverrides());
            Optional<String> consequent = switch (name) {
                case "assertEquals" -> liftBinaryAssertion(call, assertionScope, "eq");
                case "assertNotEquals" -> liftBinaryAssertion(call, assertionScope, "neq");
                case "assertTrue" -> liftTruthAssertion(call, assertionScope, true);
                case "assertFalse" -> liftTruthAssertion(call, assertionScope, false);
                default -> Optional.empty();
            };
            if (consequent.isEmpty()) return List.of();
            scopedByCallsite
                .computeIfAbsent(symbol.get(), ignored -> new ArrayList<>())
                .add(assertionScope.wrapExcluding(observedCall.get().sourceLocals(), consequent.get()));
        }
        List<LiftedAssertion> lifted = new ArrayList<>();
        for (Map.Entry<String, List<String>> entry : scopedByCallsite.entrySet()) {
            List<String> scoped = entry.getValue();
            if (!scoped.isEmpty()) {
                lifted.add(new LiftedAssertion(
                    entry.getKey(),
                    scoped.size() == 1 ? scoped.get(0) : and(scoped)
                ));
            }
        }
        return lifted;
    }

    private Optional<String> liftBinaryAssertion(MethodCallExpr call, ValueScope scope, String op) {
        if (call.getArguments().size() < 2) return Optional.empty();
        if (call.getArguments().size() > 2 && !(call.getArgument(2) instanceof StringLiteralExpr)) {
            return Optional.empty();
        }
        Optional<String> actual = liftTerm(call.getArgument(0), scope);
        Optional<String> expected = liftTerm(call.getArgument(1), scope);
        if (actual.isEmpty() || expected.isEmpty()) return Optional.empty();
        return Optional.of(atom(op, actual.get(), expected.get()));
    }

    private Optional<String> liftTruthAssertion(MethodCallExpr call, ValueScope scope, boolean expected) {
        if (call.getArguments().isEmpty()) return Optional.empty();
        if (call.getArguments().size() > 2) return Optional.empty();
        if (call.getArguments().size() == 2 && !(call.getArgument(1) instanceof StringLiteralExpr)) {
            return Optional.empty();
        }
        Optional<String> formula = liftFormula(call.getArgument(0), scope);
        if (formula.isEmpty()) return Optional.empty();
        return Optional.of(expected ? formula.get() : not(formula.get()));
    }

    private boolean isTestNgAssertion(CompilationUnit cu, MethodCallExpr call) {
        String name = call.getNameAsString();
        if (!ASSERTIONS.contains(name)) return false;

        if (call.getScope().isEmpty()) {
            return hasStaticAssertImport(cu, name);
        }

        String scope = call.getScope().get().toString();
        if (scope.equals("Assert") || scope.equals("org.testng.Assert")) {
            return true;
        }
        return scope.endsWith(".Assert") && scope.contains("testng");
    }

    private boolean hasStaticAssertImport(CompilationUnit cu, String testNgAssertionName) {
        for (ImportDeclaration importDecl : cu.getImports()) {
            if (!importDecl.isStatic()) continue;
            String imported = importDecl.getNameAsString();
            if (importDecl.isAsterisk() && imported.equals("org.testng.Assert")) {
                return true;
            }
            if (!importDecl.isAsterisk()
                    && imported.equals("org.testng.Assert." + testNgAssertionName)) {
                return true;
            }
        }
        return false;
    }

    private List<ValueScope> applyStatement(
            Statement stmt,
            List<ValueScope> scopes,
            Map<String, Integer> versions) {
        if (stmt.isBlockStmt()) {
            return applyBlock(stmt.asBlockStmt(), scopes, versions);
        }
        if (stmt.isIfStmt()) {
            return applyIf(stmt.asIfStmt(), scopes, versions);
        }
        if (!stmt.isExpressionStmt()) {
            return scopes;
        }

        Expression expr = stmt.asExpressionStmt().getExpression();
        if (expr instanceof VariableDeclarationExpr decl) {
            return applyVariableDeclaration(decl, scopes, versions);
        }
        if (expr instanceof AssignExpr assign && assign.getOperator() == AssignExpr.Operator.ASSIGN) {
            return applyAssignment(assign, scopes, versions);
        }
        return scopes;
    }

    private List<ValueScope> applyBlock(
            BlockStmt block,
            List<ValueScope> scopes,
            Map<String, Integer> versions) {
        List<ValueScope> current = scopes;
        for (Statement child : block.getStatements()) {
            current = applyStatement(child, current, versions);
        }
        return current;
    }

    private List<ValueScope> applyIf(
            IfStmt stmt,
            List<ValueScope> scopes,
            Map<String, Integer> versions) {
        List<ValueScope> out = new ArrayList<>();

        for (ValueScope scope : scopes) {
            String guard = liftFormula(stmt.getCondition(), scope)
                .orElseGet(() -> opaqueBranchCondition(stmt.getCondition()));
            ValueScope thenScope = scope.copy();
            thenScope.facts.add(new ScopeFact(guard, null));
            out.addAll(applyStatement(stmt.getThenStmt(), List.of(thenScope), versions));

            ValueScope elseScope = scope.copy();
            elseScope.facts.add(new ScopeFact(not(guard), null));
            if (stmt.getElseStmt().isPresent()) {
                out.addAll(applyStatement(stmt.getElseStmt().get(), List.of(elseScope), versions));
            } else {
                out.add(elseScope);
            }
        }

        return out;
    }

    private List<ValueScope> applyVariableDeclaration(
            VariableDeclarationExpr decl,
            List<ValueScope> scopes,
            Map<String, Integer> versions) {
        List<ValueScope> out = new ArrayList<>();
        for (ValueScope scope : scopes) {
            ValueScope next = scope.copy();
            for (VariableDeclarator varDecl : decl.getVariables()) {
                String name = varDecl.getNameAsString();
                next.locals.add(name);
                if (varDecl.getInitializer().isPresent()) {
                    bind(next, name, varDecl.getInitializer().get(), versions);
                } else {
                    next.current.remove(name);
                    next.calls.remove(name);
                }
            }
            out.add(next);
        }
        return out;
    }

    private List<ValueScope> applyAssignment(
            AssignExpr assign,
            List<ValueScope> scopes,
            Map<String, Integer> versions) {
        if (!(assign.getTarget() instanceof NameExpr target)) return scopes;

        List<ValueScope> out = new ArrayList<>();
        for (ValueScope scope : scopes) {
            ValueScope next = scope.copy();
            bind(next, target.getNameAsString(), assign.getValue(), versions);
            out.add(next);
        }
        return out;
    }

    private void bind(
            ValueScope scope,
            String name,
            Expression expr,
            Map<String, Integer> versions) {
        scope.locals.add(name);
        Optional<String> lifted = liftTerm(expr, scope);
        if (lifted.isEmpty()) {
            scope.current.remove(name);
            scope.calls.remove(name);
            return;
        }

        int version = versions.getOrDefault(name, 0);
        versions.put(name, version + 1);
        String ssaName = name + "$" + version;
        String ssaVar = var(ssaName);
        scope.current.put(name, ssaVar);
        scope.calls.remove(name);
        Expression unwrapped = unwrap(expr);
        if (unwrapped instanceof MethodCallExpr call) {
            scope.calls.put(name, new ObservedCall(call, lifted.get(), Map.of(), Set.of()));
        }
        scope.facts.add(new ScopeFact(atom("eq", ssaVar, lifted.get()), name));
    }

    private Optional<String> liftFormula(Expression expr, ValueScope scope) {
        expr = unwrap(expr);
        if (expr instanceof BinaryExpr binary) {
            Optional<String> op = binaryFormulaOp(binary.getOperator());
            if (op.isPresent()) {
                Optional<String> left = liftTerm(binary.getLeft(), scope);
                Optional<String> right = liftTerm(binary.getRight(), scope);
                if (left.isEmpty() || right.isEmpty()) return Optional.empty();
                return Optional.of(atom(op.get(), left.get(), right.get()));
            }
            if (binary.getOperator() == BinaryExpr.Operator.AND) {
                Optional<String> left = liftFormula(binary.getLeft(), scope);
                Optional<String> right = liftFormula(binary.getRight(), scope);
                if (left.isEmpty() || right.isEmpty()) return Optional.empty();
                return Optional.of(and(List.of(left.get(), right.get())));
            }
            if (binary.getOperator() == BinaryExpr.Operator.OR) {
                Optional<String> left = liftFormula(binary.getLeft(), scope);
                Optional<String> right = liftFormula(binary.getRight(), scope);
                if (left.isEmpty() || right.isEmpty()) return Optional.empty();
                return Optional.of(or(List.of(left.get(), right.get())));
            }
        }
        Optional<String> term = liftTerm(expr, scope);
        return term.map(t -> atom("eq", t, cBool(true)));
    }

    private Optional<String> liftTerm(Expression expr, ValueScope scope) {
        expr = unwrap(expr);
        if (expr instanceof NameExpr name) {
            String simple = name.getNameAsString();
            if (scope.termOverrides.containsKey(simple)) {
                return Optional.of(scope.termOverrides.get(simple));
            }
            if (scope.locals.contains(simple)) {
                return Optional.ofNullable(scope.current.get(simple));
            }
            return Optional.of(var(simple));
        }
        if (expr instanceof IntegerLiteralExpr intLit) {
            return Optional.of(cInt(intLit.asNumber().longValue()));
        }
        if (expr instanceof LongLiteralExpr longLit) {
            return Optional.of(cInt(longLit.asNumber().longValue()));
        }
        if (expr instanceof StringLiteralExpr strLit) {
            return Optional.of(cStr(strLit.getValue()));
        }
        if (expr instanceof BooleanLiteralExpr boolLit) {
            return Optional.of(cBool(boolLit.getValue()));
        }
        if (expr instanceof NullLiteralExpr) {
            return Optional.of(cNull());
        }
        if (expr instanceof UnaryExpr unary
                && unary.getOperator() == UnaryExpr.Operator.MINUS
                && unary.getExpression() instanceof IntegerLiteralExpr intLit) {
            return Optional.of(cInt(-intLit.asNumber().longValue()));
        }
        if (expr instanceof MethodCallExpr call) {
            return liftMethodCallTerm(call, scope);
        }
        if (expr instanceof FieldAccessExpr field) {
            return Optional.of(ctor(field.toString()));
        }
        if (expr instanceof BinaryExpr binary) {
            Optional<String> op = binaryTermOp(binary.getOperator());
            if (op.isEmpty()) return Optional.empty();
            Optional<String> left = liftTerm(binary.getLeft(), scope);
            Optional<String> right = liftTerm(binary.getRight(), scope);
            if (left.isEmpty() || right.isEmpty()) return Optional.empty();
            return Optional.of(ctor(op.get(), left.get(), right.get()));
        }
        return Optional.empty();
    }

    private Optional<ObservedCall> observedCallForAssertion(
            String TestNgAssertionName,
            MethodCallExpr assertion,
            ValueScope scope) {
        return switch (TestNgAssertionName) {
            case "assertEquals", "assertNotEquals" -> {
                if (assertion.getArguments().size() < 2) yield Optional.empty();
                yield observedCallForValue(assertion.getArgument(0), scope);
            }
            case "assertTrue", "assertFalse" -> {
                if (assertion.getArguments().size() != 1) yield Optional.empty();
                List<ObservedCall> calls = observedCallsInFormula(assertion.getArgument(0), scope);
                yield calls.size() == 1 ? Optional.of(calls.get(0)) : Optional.empty();
            }
            default -> Optional.empty();
        };
    }

    private Optional<ObservedCall> observedCallForValue(Expression expr, ValueScope scope) {
        expr = unwrap(expr);
        if (expr instanceof MethodCallExpr call) {
            Optional<String> term = liftTerm(call, scope);
            return term.map(t -> new ObservedCall(call, t, Map.of(), Set.of()));
        }
        if (expr instanceof NameExpr name) {
            ObservedCall bound = scope.calls.get(name.getNameAsString());
            if (bound != null) return Optional.of(bound.withLocalOverride(name.getNameAsString()));
        }
        return Optional.empty();
    }

    private List<ObservedCall> observedCallsInFormula(Expression expr, ValueScope scope) {
        Optional<ObservedCall> direct = observedCallForValue(expr, scope);
        if (direct.isPresent()) return List.of(direct.get());

        expr = unwrap(expr);
        if (expr instanceof UnaryExpr unary) {
            return observedCallsInFormula(unary.getExpression(), scope);
        }
        if (expr instanceof BinaryExpr binary) {
            List<ObservedCall> calls = new ArrayList<>();
            calls.addAll(observedCallsInFormula(binary.getLeft(), scope));
            calls.addAll(observedCallsInFormula(binary.getRight(), scope));
            return calls;
        }
        return List.of();
    }

    private Optional<String> callsiteSymbol(String sourceFile, MethodCallExpr call) {
        return call.getRange().map(range ->
            call.getNameAsString()
                + "@"
                + sourceFile
                + ":"
                + range.begin.line
                + ":"
                + range.begin.column
        );
    }

    private String sourceFileName(CompilationUnit cu) {
        if (cu.getStorage().isPresent()) {
            return cu.getStorage().get().getPath().getFileName().toString();
        }
        return cu.getPrimaryTypeName()
            .or(() -> cu.getTypes().stream().findFirst().map(type -> type.getNameAsString()))
            .map(name -> name + ".java")
            .orElse("<unknown>.java");
    }

    private Expression unwrap(Expression expr) {
        while (expr instanceof EnclosedExpr enclosed) {
            expr = enclosed.getInner();
        }
        return expr;
    }

    private Optional<String> liftMethodCallTerm(MethodCallExpr call, ValueScope scope) {
        List<String> args = new ArrayList<>();
        String name = call.getNameAsString();

        if (call.getScope().isPresent()) {
            Expression recv = call.getScope().get();
            if (recv instanceof NameExpr nameExpr && startsUppercase(nameExpr.getNameAsString())) {
                name = nameExpr.getNameAsString() + "." + name;
            } else if (recv instanceof FieldAccessExpr fieldAccess) {
                name = fieldAccess.toString() + "." + name;
            } else {
                Optional<String> recvTerm = liftTerm(recv, scope);
                if (recvTerm.isEmpty()) return Optional.empty();
                args.add(recvTerm.get());
            }
        }

        for (Expression arg : call.getArguments()) {
            Optional<String> lifted = liftTerm(arg, scope);
            if (lifted.isEmpty()) return Optional.empty();
            args.add(lifted.get());
        }
        return Optional.of(ctor(name, args));
    }

    private Optional<String> binaryFormulaOp(BinaryExpr.Operator op) {
        return switch (op) {
            case EQUALS -> Optional.of("eq");
            case NOT_EQUALS -> Optional.of("neq");
            case GREATER -> Optional.of("gt");
            case GREATER_EQUALS -> Optional.of("gte");
            case LESS -> Optional.of("lt");
            case LESS_EQUALS -> Optional.of("lte");
            default -> Optional.empty();
        };
    }

    private Optional<String> binaryTermOp(BinaryExpr.Operator op) {
        return switch (op) {
            case PLUS -> Optional.of("+");
            case MINUS -> Optional.of("-");
            case MULTIPLY -> Optional.of("*");
            case DIVIDE -> Optional.of("/");
            case REMAINDER -> Optional.of("%");
            default -> Optional.empty();
        };
    }

    private String opaqueBranchCondition(Expression expr) {
        return atom("testng_branch_condition", cStr(expr.toString()));
    }

    private boolean startsUppercase(String s) {
        return !s.isEmpty() && Character.isUpperCase(s.charAt(0));
    }

    private static final class ValueScope {
        final Map<String, String> current;
        final Set<String> locals;
        final List<ScopeFact> facts;
        final Map<String, ObservedCall> calls;
        final Map<String, String> termOverrides;

        ValueScope() {
            this(
                new LinkedHashMap<>(),
                new LinkedHashSet<>(),
                new ArrayList<>(),
                new LinkedHashMap<>(),
                new LinkedHashMap<>()
            );
        }

        private ValueScope(
                Map<String, String> current,
                Set<String> locals,
                List<ScopeFact> facts,
                Map<String, ObservedCall> calls,
                Map<String, String> termOverrides) {
            this.current = current;
            this.locals = locals;
            this.facts = facts;
            this.calls = calls;
            this.termOverrides = termOverrides;
        }

        ValueScope copy() {
            return new ValueScope(
                new LinkedHashMap<>(current),
                new LinkedHashSet<>(locals),
                new ArrayList<>(facts),
                new LinkedHashMap<>(calls),
                new LinkedHashMap<>(termOverrides)
            );
        }

        String wrap(String consequent) {
            return wrapExcluding(Set.of(), consequent);
        }

        String wrapExcluding(Set<String> excludedLocals, String consequent) {
            List<String> formulas = new ArrayList<>();
            for (ScopeFact fact : facts) {
                if (fact.localName() == null || !excludedLocals.contains(fact.localName())) {
                    formulas.add(fact.formula());
                }
            }
            if (formulas.isEmpty()) return consequent;
            return implies(formulas.size() == 1 ? formulas.get(0) : and(formulas), consequent);
        }

        ValueScope withTermOverrides(Map<String, String> overrides) {
            ValueScope next = copy();
            next.termOverrides.putAll(overrides);
            return next;
        }
    }

    private static String var(String name) {
        return "{\"kind\":\"var\",\"name\":\"" + esc(name) + "\"}";
    }

    private static String cInt(long value) {
        return "{\"kind\":\"const\",\"value\":" + value
            + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
    }

    private static String cStr(String value) {
        return "{\"kind\":\"const\",\"value\":\"" + esc(value)
            + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}";
    }

    private static String cBool(boolean value) {
        return "{\"kind\":\"const\",\"value\":" + value
            + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Bool\"}}";
    }

    private static String cNull() {
        return "{\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}}";
    }

    private static String ctor(String name, String... args) {
        return ctor(name, Arrays.asList(args));
    }

    private static String ctor(String name, List<String> args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"ctor\",\"name\":\"")
            .append(esc(name))
            .append("\",\"args\":[");
        for (int i = 0; i < args.size(); i++) {
            if (i > 0) sb.append(",");
            sb.append(args.get(i));
        }
        return sb.append("]}").toString();
    }

    private static String atom(String name, String... args) {
        return atom(name, Arrays.asList(args));
    }

    private static String atom(String name, List<String> args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\"")
            .append(esc(name))
            .append("\",\"args\":[");
        for (int i = 0; i < args.size(); i++) {
            if (i > 0) sb.append(",");
            sb.append(args.get(i));
        }
        return sb.append("]}").toString();
    }

    private static String and(List<String> operands) {
        return connective("and", operands);
    }

    private static String and(String... operands) {
        return connective("and", Arrays.asList(operands));
    }

    private static String or(List<String> operands) {
        return connective("or", operands);
    }

    private static String not(String operand) {
        return connective("not", List.of(operand));
    }

    private static String implies(String antecedent, String consequent) {
        return connective("implies", List.of(antecedent, consequent));
    }

    private static String connective(String kind, List<String> operands) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"")
            .append(kind)
            .append("\",\"operands\":[");
        for (int i = 0; i < operands.size(); i++) {
            if (i > 0) sb.append(",");
            sb.append(operands.get(i));
        }
        return sb.append("]}").toString();
    }

    private static String esc(String s) {
        return s
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\n")
            .replace("\r", "\\r")
            .replace("\t", "\\t");
    }

    private record LiftedAssertion(String symbol, String formula) {}

    private record ObservedCall(
            MethodCallExpr call,
            String term,
            Map<String, String> termOverrides,
            Set<String> sourceLocals) {
        ObservedCall withLocalOverride(String localName) {
            Map<String, String> nextOverrides = new LinkedHashMap<>(termOverrides);
            nextOverrides.put(localName, term);
            Set<String> nextLocals = new LinkedHashSet<>(sourceLocals);
            nextLocals.add(localName);
            return new ObservedCall(call, term, nextOverrides, nextLocals);
        }
    }

    private record ScopeFact(String formula, String localName) {}
}
