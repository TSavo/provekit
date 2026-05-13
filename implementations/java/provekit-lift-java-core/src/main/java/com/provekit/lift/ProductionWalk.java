package com.provekit.lift;

import java.util.*;

import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.Node;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.github.javaparser.ast.expr.*;
import com.github.javaparser.ast.stmt.*;

public final class ProductionWalk {
    private ProductionWalk() {}

    public record Result(List<ContractDecl> declarations, List<ImplicationDecl> implications) {}

    private record FunctionPrecondition(String name, List<String> formals, String precondition) {}

    private record CallsiteHit(
            MethodCallExpr call,
            int stmtIndex,
            List<String> conditions,
            List<Statement> precedingInnerStmts) {}

    private record Binding(String name, String term) {}

    public static Result lift(CompilationUnit cu, String sourceFile) {
        List<MethodDeclaration> methods = cu.findAll(MethodDeclaration.class);
        Map<String, FunctionPrecondition> callees = new LinkedHashMap<>();
        for (MethodDeclaration method : methods) {
            if (isTestMethod(method)) continue;
            liftFunctionPrecondition(method).ifPresent(pre -> callees.putIfAbsent(pre.name(), pre));
        }

        List<ContractDecl> declarations = new ArrayList<>();
        List<ImplicationDecl> implications = new ArrayList<>();
        Set<String> usedNames = new LinkedHashSet<>();
        for (MethodDeclaration caller : methods) {
            if (isTestMethod(caller) || caller.getBody().isEmpty()) continue;
            for (FunctionPrecondition callee : callees.values()) {
                if (callee.name().equals(caller.getNameAsString())) continue;
                emitWalksForCallee(caller, callee, sourceFile, declarations, implications, usedNames);
            }
        }
        return new Result(declarations, implications);
    }

    private static Optional<FunctionPrecondition> liftFunctionPrecondition(MethodDeclaration method) {
        if (method.getBody().isEmpty()) return Optional.empty();
        List<String> atoms = new ArrayList<>();
        for (Statement stmt : method.getBody().get().getStatements()) {
            stmtPreconditionContribution(stmt).ifPresent(atoms::add);
        }
        if (atoms.isEmpty()) return Optional.empty();
        String precondition = atoms.size() == 1 ? atoms.get(0) : and(atoms);
        List<String> formals = method.getParameters()
            .stream()
            .map(p -> p.getNameAsString())
            .toList();
        return Optional.of(new FunctionPrecondition(method.getNameAsString(), formals, precondition));
    }

    private static Optional<String> stmtPreconditionContribution(Statement stmt) {
        if (stmt instanceof AssertStmt assertStmt) {
            return liftFormula(assertStmt.getCheck());
        }
        if (stmt instanceof IfStmt ifStmt
                && ifStmt.getElseStmt().isEmpty()
                && statementOnlyThrows(ifStmt.getThenStmt())) {
            return liftNegatedFormula(ifStmt.getCondition());
        }
        return Optional.empty();
    }

    private static boolean statementOnlyThrows(Statement stmt) {
        if (stmt instanceof ThrowStmt) return true;
        if (stmt instanceof BlockStmt block) {
            return block.getStatements().size() == 1
                && block.getStatement(0) instanceof ThrowStmt;
        }
        return false;
    }

    private static void emitWalksForCallee(
            MethodDeclaration caller,
            FunctionPrecondition callee,
            String sourceFile,
            List<ContractDecl> declarations,
            List<ImplicationDecl> implications,
            Set<String> usedNames) {
        for (CallsiteHit hit : findCallsites(caller, callee.name())) {
            if (callee.formals().size() != hit.call().getArguments().size()) continue;

            String wp = callee.precondition();
            for (int i = 0; i < callee.formals().size(); i++) {
                wp = substituteVar(wp, callee.formals().get(i), termFromExpr(hit.call().getArgument(i)));
            }

            if (!hit.conditions().isEmpty()) {
                String premise = hit.conditions().size() == 1 ? hit.conditions().get(0) : and(hit.conditions());
                wp = implies(premise, wp);
            }

            Optional<String> base = callsiteBase(callee.name(), sourceFile, hit.call());
            if (base.isEmpty()) continue;
            appendEdge(
                declarations,
                implications,
                usedNames,
                base.get() + "::callsite",
                wp,
                wp,
                caller.getNameAsString(),
                callee.name()
            );
            String previousWp = wp;

            for (Statement stmt : hit.precedingInnerStmts()) {
                for (Binding binding : bindingsFromStmt(stmt)) {
                    String nextWp = substituteVar(previousWp, binding.name(), binding.term());
                    appendEdge(
                        declarations,
                        implications,
                        usedNames,
                        base.get() + "::let:" + binding.name(),
                        nextWp,
                        previousWp,
                        caller.getNameAsString(),
                        callee.name()
                    );
                    previousWp = nextWp;
                }
            }

            List<Statement> stmts = caller.getBody().get().getStatements();
            for (int i = hit.stmtIndex() - 1; i >= 0; i--) {
                for (Binding binding : bindingsFromStmt(stmts.get(i))) {
                    String nextWp = substituteVar(previousWp, binding.name(), binding.term());
                    appendEdge(
                        declarations,
                        implications,
                        usedNames,
                        base.get() + "::let:" + binding.name(),
                        nextWp,
                        previousWp,
                        caller.getNameAsString(),
                        callee.name()
                    );
                    previousWp = nextWp;
                }
            }

            appendEdge(
                declarations,
                implications,
                usedNames,
                base.get() + "::entry",
                previousWp,
                previousWp,
                caller.getNameAsString(),
                callee.name()
            );
        }
    }

    private static void appendEdge(
            List<ContractDecl> declarations,
            List<ImplicationDecl> implications,
            Set<String> usedNames,
            String rawName,
            String pre,
            String post,
            String caller,
            String callee) {
        String name = uniqueName(rawName, usedNames);
        declarations.add(new ContractDecl(name, List.of(pre), List.of(post)));
        implications.add(new ImplicationDecl(
            name + "::pre-implies-post",
            name,
            name,
            "pre",
            "post",
            "java-wp-walk",
            caller + "->" + callee
        ));
    }

    private static List<CallsiteHit> findCallsites(MethodDeclaration caller, String calleeName) {
        if (caller.getBody().isEmpty()) return List.of();
        List<CallsiteHit> hits = new ArrayList<>();
        List<Statement> stmts = caller.getBody().get().getStatements();
        for (int i = 0; i < stmts.size(); i++) {
            walkStmtForCallsites(stmts.get(i), i, calleeName, new ArrayList<>(), new ArrayList<>(), hits);
        }
        return hits;
    }

    private static void walkStmtForCallsites(
            Statement stmt,
            int stmtIndex,
            String calleeName,
            List<String> conditions,
            List<Statement> innerStmts,
            List<CallsiteHit> hits) {
        if (stmt instanceof IfStmt ifStmt) {
            Optional<String> lifted = liftFormula(ifStmt.getCondition());
            lifted.ifPresent(conditions::add);
            walkStmtForCallsites(ifStmt.getThenStmt(), stmtIndex, calleeName, conditions, innerStmts, hits);
            if (lifted.isPresent()) {
                conditions.remove(conditions.size() - 1);
                conditions.add(negateFormula(lifted.get()));
            }
            ifStmt.getElseStmt().ifPresent(elseStmt ->
                walkStmtForCallsites(elseStmt, stmtIndex, calleeName, conditions, innerStmts, hits));
            if (lifted.isPresent()) conditions.remove(conditions.size() - 1);
            return;
        }
        if (stmt instanceof BlockStmt block) {
            walkBlockForCallsites(block.getStatements(), stmtIndex, calleeName, conditions, innerStmts, hits);
            return;
        }
        for (MethodCallExpr call : stmt.findAll(MethodCallExpr.class)) {
            if (call.getNameAsString().equals(calleeName)) {
                hits.add(new CallsiteHit(
                    call,
                    stmtIndex,
                    new ArrayList<>(conditions),
                    new ArrayList<>(innerStmts)
                ));
            }
        }
    }

    private static void walkBlockForCallsites(
            List<Statement> stmts,
            int stmtIndex,
            String calleeName,
            List<String> conditions,
            List<Statement> innerStmts,
            List<CallsiteHit> hits) {
        for (int i = 0; i < stmts.size(); i++) {
            List<Statement> branchPreceding = new ArrayList<>();
            for (int j = i - 1; j >= 0; j--) {
                branchPreceding.add(stmts.get(j));
            }
            branchPreceding.addAll(innerStmts);
            walkStmtForCallsites(stmts.get(i), stmtIndex, calleeName, conditions, branchPreceding, hits);
        }
    }

    private static List<Binding> bindingsFromStmt(Statement stmt) {
        if (!stmt.isExpressionStmt()) return List.of();
        Expression expr = stmt.asExpressionStmt().getExpression();
        List<Binding> bindings = new ArrayList<>();
        if (expr instanceof VariableDeclarationExpr decl) {
            for (var variable : decl.getVariables()) {
                variable.getInitializer().ifPresent(init ->
                    bindings.add(new Binding(variable.getNameAsString(), termFromExpr(init))));
            }
            return bindings;
        }
        if (expr instanceof AssignExpr assign
                && assign.getOperator() == AssignExpr.Operator.ASSIGN
                && assign.getTarget() instanceof NameExpr name) {
            return List.of(new Binding(name.getNameAsString(), termFromExpr(assign.getValue())));
        }
        return List.of();
    }

    private static Optional<String> liftFormula(Expression expr) {
        expr = unwrap(expr);
        if (expr instanceof BinaryExpr binary) {
            Optional<String> op = binaryFormulaOp(binary.getOperator());
            if (op.isPresent()) {
                return Optional.of(atom(op.get(), termFromExpr(binary.getLeft()), termFromExpr(binary.getRight())));
            }
            if (binary.getOperator() == BinaryExpr.Operator.AND) {
                Optional<String> left = liftFormula(binary.getLeft());
                Optional<String> right = liftFormula(binary.getRight());
                if (left.isEmpty() || right.isEmpty()) return Optional.empty();
                return Optional.of(and(List.of(left.get(), right.get())));
            }
            if (binary.getOperator() == BinaryExpr.Operator.OR) {
                Optional<String> left = liftFormula(binary.getLeft());
                Optional<String> right = liftFormula(binary.getRight());
                if (left.isEmpty() || right.isEmpty()) return Optional.empty();
                return Optional.of(or(List.of(left.get(), right.get())));
            }
        }
        if (expr instanceof UnaryExpr unary && unary.getOperator() == UnaryExpr.Operator.LOGICAL_COMPLEMENT) {
            return liftFormula(unary.getExpression()).map(ProductionWalk::negateFormula);
        }
        return Optional.of(atom("eq", termFromExpr(expr), cBool(true)));
    }

    private static Optional<String> liftNegatedFormula(Expression expr) {
        expr = unwrap(expr);
        if (expr instanceof BinaryExpr binary) {
            Optional<String> op = inverseFormulaOp(binary.getOperator());
            if (op.isPresent()) {
                return Optional.of(atom(op.get(), termFromExpr(binary.getLeft()), termFromExpr(binary.getRight())));
            }
        }
        return liftFormula(expr).map(ProductionWalk::negateFormula);
    }

    private static String termFromExpr(Expression expr) {
        expr = unwrap(expr);
        if (expr instanceof NameExpr name) {
            return var(name.getNameAsString());
        }
        if (expr instanceof IntegerLiteralExpr intLit) {
            return cInt(intLit.asNumber().longValue());
        }
        if (expr instanceof LongLiteralExpr longLit) {
            return cInt(longLit.asNumber().longValue());
        }
        if (expr instanceof StringLiteralExpr strLit) {
            return cStr(strLit.getValue());
        }
        if (expr instanceof BooleanLiteralExpr boolLit) {
            return cBool(boolLit.getValue());
        }
        if (expr instanceof NullLiteralExpr) {
            return cNull();
        }
        if (expr instanceof UnaryExpr unary
                && unary.getOperator() == UnaryExpr.Operator.MINUS
                && unary.getExpression() instanceof IntegerLiteralExpr intLit) {
            return cInt(-intLit.asNumber().longValue());
        }
        if (expr instanceof MethodCallExpr call) {
            List<String> args = new ArrayList<>();
            if (call.getScope().isPresent()) {
                args.add(termFromExpr(call.getScope().get()));
            }
            for (Expression arg : call.getArguments()) {
                args.add(termFromExpr(arg));
            }
            return ctor(call.getNameAsString(), args);
        }
        if (expr instanceof BinaryExpr binary) {
            Optional<String> op = binaryTermOp(binary.getOperator());
            if (op.isPresent()) {
                return ctor(op.get(), List.of(termFromExpr(binary.getLeft()), termFromExpr(binary.getRight())));
            }
        }
        if (expr instanceof FieldAccessExpr field) {
            return ctor("field", List.of(termFromExpr(field.getScope()), cStr(field.getNameAsString())));
        }
        return var("<expr:" + expr + ">");
    }

    private static String substituteVar(String formula, String varName, String replacement) {
        return formula.replace(var(varName), replacement);
    }

    private static Optional<String> callsiteBase(String callee, String sourceFile, MethodCallExpr call) {
        return call.getRange().map(range ->
            callee + "@" + sourceFile + ":" + range.begin.line + ":" + range.begin.column);
    }

    private static String uniqueName(String name, Set<String> usedNames) {
        if (usedNames.add(name)) return name;
        int i = 1;
        while (!usedNames.add(name + "::" + i)) {
            i++;
        }
        return name + "::" + i;
    }

    private static boolean isTestMethod(MethodDeclaration method) {
        for (AnnotationExpr ann : method.getAnnotations()) {
            String name = ann.getNameAsString();
            if (name.equals("Test")
                    || name.equals("RepeatedTest")
                    || name.equals("ParameterizedTest")
                    || name.endsWith(".Test")
                    || name.endsWith(".ParameterizedTest")) {
                return true;
            }
        }
        return method.getNameAsString().startsWith("test");
    }

    private static Expression unwrap(Expression expr) {
        while (expr instanceof EnclosedExpr enclosed) {
            expr = enclosed.getInner();
        }
        return expr;
    }

    private static Optional<String> binaryFormulaOp(BinaryExpr.Operator op) {
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

    private static Optional<String> inverseFormulaOp(BinaryExpr.Operator op) {
        return switch (op) {
            case EQUALS -> Optional.of("neq");
            case NOT_EQUALS -> Optional.of("eq");
            case GREATER -> Optional.of("lte");
            case GREATER_EQUALS -> Optional.of("lt");
            case LESS -> Optional.of("gte");
            case LESS_EQUALS -> Optional.of("gt");
            default -> Optional.empty();
        };
    }

    private static Optional<String> binaryTermOp(BinaryExpr.Operator op) {
        return switch (op) {
            case PLUS -> Optional.of("+");
            case MINUS -> Optional.of("-");
            case MULTIPLY -> Optional.of("*");
            case DIVIDE -> Optional.of("/");
            case REMAINDER -> Optional.of("%");
            default -> Optional.empty();
        };
    }

    private static String negateFormula(String formula) {
        return not(formula);
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
