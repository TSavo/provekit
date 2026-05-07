package com.provekit.lift.junit;

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

public class JUnitExtractor implements Extractor {
    private static final Set<String> JUPITER_TESTS = Set.of("Test", "RepeatedTest");
    private static final Set<String> JUPITER_PARAM_TESTS = Set.of("ParameterizedTest");
    private static final Set<String> ASSERTIONS = Set.of(
        "assertEquals", "assertNotEquals", "assertTrue", "assertFalse"
    );

    public String name() { return "junit"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        for (TypeDeclaration<?> type : cu.getTypes()) {
            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof MethodDeclaration method && isJUnitTest(cu, method)) {
                    extractMethod(cu, method, out);
                }
            }
        }
        return out;
    }

    private boolean isJUnitTest(CompilationUnit cu, MethodDeclaration method) {
        for (AnnotationExpr ann : method.getAnnotations()) {
            if (AnnotationSupport.belongsToFamily(
                    cu, ann, "org.junit.jupiter.api", JUPITER_TESTS, Set.of())) {
                return true;
            }
            if (AnnotationSupport.belongsToFamily(
                    cu, ann, "org.junit.jupiter.params", JUPITER_PARAM_TESTS, Set.of())) {
                return true;
            }
        }
        return false;
    }

    private void extractMethod(CompilationUnit cu, MethodDeclaration method, List<ContractDecl> out) {
        if (method.getBody().isEmpty()) return;

        Map<String, Integer> versions = new LinkedHashMap<>();
        List<ValueScope> scopes = List.of(new ValueScope());
        int assertionIndex = 0;

        for (Statement stmt : method.getBody().get().getStatements()) {
            Optional<String> assertion = liftAssertion(cu, stmt, scopes);
            if (assertion.isPresent()) {
                out.add(new ContractDecl(
                    method.getNameAsString() + "::" + assertionIndex,
                    List.of(),
                    List.of(),
                    List.of(assertion.get())
                ));
                assertionIndex++;
                continue;
            }
            scopes = applyStatement(stmt, scopes, versions);
        }
    }

    private Optional<String> liftAssertion(
            CompilationUnit cu,
            Statement stmt,
            List<ValueScope> scopes) {
        if (!stmt.isExpressionStmt()) return Optional.empty();
        Expression expr = stmt.asExpressionStmt().getExpression();
        if (!(expr instanceof MethodCallExpr call)) return Optional.empty();
        if (!isJUnitAssertion(cu, call)) return Optional.empty();

        String name = call.getNameAsString();
        List<String> scoped = new ArrayList<>();
        for (ValueScope scope : scopes) {
            Optional<String> consequent = switch (name) {
                case "assertEquals" -> liftBinaryAssertion(call, scope, "eq");
                case "assertNotEquals" -> liftBinaryAssertion(call, scope, "neq");
                case "assertTrue" -> liftTruthAssertion(call, scope, true);
                case "assertFalse" -> liftTruthAssertion(call, scope, false);
                default -> Optional.empty();
            };
            if (consequent.isEmpty()) return Optional.empty();
            scoped.add(scope.wrap(consequent.get()));
        }
        if (scoped.isEmpty()) return Optional.empty();
        return Optional.of(scoped.size() == 1 ? scoped.get(0) : and(scoped));
    }

    private Optional<String> liftBinaryAssertion(MethodCallExpr call, ValueScope scope, String op) {
        if (call.getArguments().size() < 2) return Optional.empty();
        if (call.getArguments().size() > 2 && !(call.getArgument(2) instanceof StringLiteralExpr)) {
            return Optional.empty();
        }
        Optional<String> expected = liftTerm(call.getArgument(0), scope);
        Optional<String> actual = liftTerm(call.getArgument(1), scope);
        if (expected.isEmpty() || actual.isEmpty()) return Optional.empty();
        return Optional.of(atom(op, actual.get(), expected.get()));
    }

    private Optional<String> liftTruthAssertion(MethodCallExpr call, ValueScope scope, boolean expected) {
        if (call.getArguments().size() != 1) return Optional.empty();
        Optional<String> formula = liftFormula(call.getArgument(0), scope);
        if (formula.isEmpty()) return Optional.empty();
        return Optional.of(expected ? formula.get() : not(formula.get()));
    }

    private boolean isJUnitAssertion(CompilationUnit cu, MethodCallExpr call) {
        String name = call.getNameAsString();
        if (!ASSERTIONS.contains(name)) return false;

        if (call.getScope().isEmpty()) {
            return hasStaticAssertionsImport(cu, name);
        }

        String scope = call.getScope().get().toString();
        if (scope.equals("Assertions") || scope.equals("org.junit.jupiter.api.Assertions")) {
            return true;
        }
        return scope.endsWith(".Assertions") && scope.contains("junit");
    }

    private boolean hasStaticAssertionsImport(CompilationUnit cu, String assertionName) {
        for (ImportDeclaration importDecl : cu.getImports()) {
            if (!importDecl.isStatic()) continue;
            String imported = importDecl.getNameAsString();
            if (importDecl.isAsterisk() && imported.equals("org.junit.jupiter.api.Assertions")) {
                return true;
            }
            if (!importDecl.isAsterisk()
                    && imported.equals("org.junit.jupiter.api.Assertions." + assertionName)) {
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
            thenScope.facts.add(guard);
            out.addAll(applyStatement(stmt.getThenStmt(), List.of(thenScope), versions));

            ValueScope elseScope = scope.copy();
            elseScope.facts.add(not(guard));
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
            return;
        }

        int version = versions.getOrDefault(name, 0);
        versions.put(name, version + 1);
        String ssaName = name + "$" + version;
        String ssaVar = var(ssaName);
        scope.current.put(name, ssaVar);
        scope.facts.add(atom("eq", ssaVar, lifted.get()));
    }

    private Optional<String> liftFormula(Expression expr, ValueScope scope) {
        if (expr instanceof EnclosedExpr enclosed) return liftFormula(enclosed.getInner(), scope);
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
        if (expr instanceof EnclosedExpr enclosed) return liftTerm(enclosed.getInner(), scope);
        if (expr instanceof NameExpr name) {
            String simple = name.getNameAsString();
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
        return atom("junit_branch_condition", cStr(expr.toString()));
    }

    private boolean startsUppercase(String s) {
        return !s.isEmpty() && Character.isUpperCase(s.charAt(0));
    }

    private static final class ValueScope {
        final Map<String, String> current;
        final Set<String> locals;
        final List<String> facts;

        ValueScope() {
            this(new LinkedHashMap<>(), new LinkedHashSet<>(), new ArrayList<>());
        }

        private ValueScope(Map<String, String> current, Set<String> locals, List<String> facts) {
            this.current = current;
            this.locals = locals;
            this.facts = facts;
        }

        ValueScope copy() {
            return new ValueScope(
                new LinkedHashMap<>(current),
                new LinkedHashSet<>(locals),
                new ArrayList<>(facts)
            );
        }

        String wrap(String consequent) {
            if (facts.isEmpty()) return consequent;
            return implies(facts.size() == 1 ? facts.get(0) : and(facts), consequent);
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
}
