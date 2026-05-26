package com.provekit.lift;

import java.util.*;

import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.github.javaparser.ast.expr.*;
import com.github.javaparser.ast.stmt.AssertStmt;
import com.github.javaparser.ast.stmt.ReturnStmt;
import com.github.javaparser.ast.stmt.Statement;
import com.github.javaparser.ast.type.PrimitiveType;
import com.github.javaparser.ast.type.Type;

/**
 * Verify-facing Java lift pass for the production bridge gauntlet.
 *
 * This pass stays inside the Java kit: it reads the JavaParser AST and emits
 * normalized ProofIR declarations for the shared Rust mint/verify pipeline to
 * consume. It deliberately emits core operator names such as "*" and "=" so
 * the verifier can reduce body-derived obligations without Rust-side Java
 * parsing.
 */
public final class VerifyLift {
    private VerifyLift() {}

    public static List<ContractDecl> lift(CompilationUnit cu, String sourceFile) {
        List<ContractDecl> decls = new ArrayList<>();
        for (MethodDeclaration method : cu.findAll(MethodDeclaration.class)) {
            liftFunctionContract(method).ifPresent(decls::add);
            decls.addAll(liftAssertionContracts(method, sourceFile));
        }
        return decls;
    }

    private static Optional<ContractDecl> liftFunctionContract(MethodDeclaration method) {
        if (method.getBody().isEmpty() || method.getType().isVoidType()) {
            return Optional.empty();
        }
        List<Statement> statements = method.getBody().get().getStatements();
        if (statements.size() != 1 || !(statements.get(0) instanceof ReturnStmt ret)) {
            return Optional.empty();
        }
        if (ret.getExpression().isEmpty()) {
            return Optional.empty();
        }

        Optional<String> returnSort = sortForType(method.getType());
        if (returnSort.isEmpty()) {
            return Optional.empty();
        }

        List<String> formals = new ArrayList<>();
        List<String> formalSorts = new ArrayList<>();
        for (var param : method.getParameters()) {
            Optional<String> sort = sortForType(param.getType());
            if (sort.isEmpty()) {
                return Optional.empty();
            }
            formals.add(param.getNameAsString());
            formalSorts.add(sort.get());
        }

        Optional<String> body = termFromExpr(ret.getExpression().get());
        if (body.isEmpty()) {
            return Optional.empty();
        }

        String fnName = method.getNameAsString();
        String post = atom("=", List.of(var("result"), body.get()));
        String json = new StringBuilder("{\"kind\":\"function-contract\"")
            .append(",\"fn_name\":\"").append(esc(fnName)).append("\"")
            .append(",\"bridgeSourceSymbol\":\"").append(esc(fnName)).append("\"")
            .append(",\"formals\":").append(stringArray(formals))
            .append(",\"formalSorts\":").append(sortArray(formalSorts))
            .append(",\"returnSort\":").append(returnSort.get())
            .append(",\"outBinding\":\"result\"")
            .append(",\"post\":").append(post)
            .append("}")
            .toString();
        return Optional.of(new RawContractDecl(fnName, json));
    }

    private static List<ContractDecl> liftAssertionContracts(MethodDeclaration method, String sourceFile) {
        if (method.getBody().isEmpty()) {
            return List.of();
        }
        List<ContractDecl> decls = new ArrayList<>();
        for (AssertStmt assertStmt : method.findAll(AssertStmt.class)) {
            Optional<String> inv = formulaFromExpr(assertStmt.getCheck());
            Optional<MethodCallExpr> call = assertStmt.getCheck()
                .findAll(MethodCallExpr.class)
                .stream()
                .findFirst();
            if (inv.isEmpty() || call.isEmpty() || call.get().getRange().isEmpty()) {
                continue;
            }
            var range = call.get().getRange().get();
            String symbol = call.get().getNameAsString()
                + "@"
                + sourceFile
                + ":"
                + range.begin.line
                + ":"
                + range.begin.column;
            String json = new StringBuilder("{\"kind\":\"contract\"")
                .append(",\"symbol\":\"").append(esc(symbol)).append("\"")
                .append(",\"outBinding\":\"out\"")
                .append(",\"inv\":").append(inv.get())
                .append("}")
                .toString();
            decls.add(new RawContractDecl(symbol, json));
        }
        return decls;
    }

    private static Optional<String> formulaFromExpr(Expression expr) {
        expr = unwrap(expr);
        if (expr instanceof BinaryExpr binary) {
            Optional<String> op = binaryFormulaOp(binary.getOperator());
            if (op.isPresent()) {
                Optional<String> left = termFromExpr(binary.getLeft());
                Optional<String> right = termFromExpr(binary.getRight());
                if (left.isEmpty() || right.isEmpty()) {
                    return Optional.empty();
                }
                return Optional.of(atom(op.get(), List.of(left.get(), right.get())));
            }
            if (binary.getOperator() == BinaryExpr.Operator.AND) {
                Optional<String> left = formulaFromExpr(binary.getLeft());
                Optional<String> right = formulaFromExpr(binary.getRight());
                if (left.isEmpty() || right.isEmpty()) {
                    return Optional.empty();
                }
                return Optional.of(connective("and", List.of(left.get(), right.get())));
            }
            if (binary.getOperator() == BinaryExpr.Operator.OR) {
                Optional<String> left = formulaFromExpr(binary.getLeft());
                Optional<String> right = formulaFromExpr(binary.getRight());
                if (left.isEmpty() || right.isEmpty()) {
                    return Optional.empty();
                }
                return Optional.of(connective("or", List.of(left.get(), right.get())));
            }
        }
        if (expr instanceof UnaryExpr unary && unary.getOperator() == UnaryExpr.Operator.LOGICAL_COMPLEMENT) {
            return formulaFromExpr(unary.getExpression())
                .map(formula -> connective("not", List.of(formula)));
        }
        return termFromExpr(expr).map(term -> atom("=", List.of(term, cBool(true))));
    }

    private static Optional<String> termFromExpr(Expression expr) {
        expr = unwrap(expr);
        if (expr instanceof NameExpr name) {
            return Optional.of(var(name.getNameAsString()));
        }
        if (expr instanceof IntegerLiteralExpr intLit) {
            return Optional.of(cInt(intLit.asNumber().longValue()));
        }
        if (expr instanceof LongLiteralExpr longLit) {
            return Optional.of(cInt(longLit.asNumber().longValue()));
        }
        if (expr instanceof BooleanLiteralExpr boolLit) {
            return Optional.of(cBool(boolLit.getValue()));
        }
        if (expr instanceof StringLiteralExpr strLit) {
            return Optional.of(cStr(strLit.getValue()));
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
            List<String> args = new ArrayList<>();
            if (call.getScope().isPresent()) {
                Optional<String> scope = termFromExpr(call.getScope().get());
                if (scope.isEmpty()) {
                    return Optional.empty();
                }
                args.add(scope.get());
            }
            for (Expression arg : call.getArguments()) {
                Optional<String> lifted = termFromExpr(arg);
                if (lifted.isEmpty()) {
                    return Optional.empty();
                }
                args.add(lifted.get());
            }
            return Optional.of(ctor(call.getNameAsString(), args));
        }
        if (expr instanceof BinaryExpr binary) {
            Optional<String> op = binaryTermOp(binary.getOperator());
            if (op.isPresent()) {
                Optional<String> left = termFromExpr(binary.getLeft());
                Optional<String> right = termFromExpr(binary.getRight());
                if (left.isEmpty() || right.isEmpty()) {
                    return Optional.empty();
                }
                return Optional.of(ctor(op.get(), List.of(left.get(), right.get())));
            }
        }
        if (expr instanceof FieldAccessExpr field) {
            Optional<String> scope = termFromExpr(field.getScope());
            return scope.map(s -> ctor("field", List.of(s, cStr(field.getNameAsString()))));
        }
        return Optional.empty();
    }

    private static Optional<String> sortForType(Type type) {
        if (type.isPrimitiveType()) {
            PrimitiveType.Primitive primitive = type.asPrimitiveType().getType();
            return switch (primitive) {
                case BOOLEAN -> Optional.of(sort("Bool"));
                case BYTE, SHORT, INT, LONG, CHAR -> Optional.of(sort("Int"));
                case FLOAT, DOUBLE -> Optional.of(sort("Real"));
            };
        }
        if (type.isClassOrInterfaceType()
                && type.asClassOrInterfaceType().getNameAsString().equals("String")) {
            return Optional.of(sort("String"));
        }
        return Optional.empty();
    }

    private static Optional<String> binaryFormulaOp(BinaryExpr.Operator op) {
        return switch (op) {
            case EQUALS -> Optional.of("=");
            case NOT_EQUALS -> Optional.of("distinct");
            case GREATER -> Optional.of(">");
            case GREATER_EQUALS -> Optional.of(">=");
            case LESS -> Optional.of("<");
            case LESS_EQUALS -> Optional.of("<=");
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

    private static Expression unwrap(Expression expr) {
        while (expr instanceof EnclosedExpr enclosed) {
            expr = enclosed.getInner();
        }
        return expr;
    }

    private static String var(String name) {
        return "{\"kind\":\"var\",\"name\":\"" + esc(name) + "\"}";
    }

    private static String cInt(long value) {
        return "{\"kind\":\"const\",\"value\":" + value
            + ",\"sort\":" + sort("Int") + "}";
    }

    private static String cBool(boolean value) {
        return "{\"kind\":\"const\",\"value\":" + value
            + ",\"sort\":" + sort("Bool") + "}";
    }

    private static String cStr(String value) {
        return "{\"kind\":\"const\",\"value\":\"" + esc(value)
            + "\",\"sort\":" + sort("String") + "}";
    }

    private static String cNull() {
        return "{\"kind\":\"const\",\"value\":null,\"sort\":" + sort("Ref") + "}";
    }

    private static String sort(String name) {
        return "{\"kind\":\"primitive\",\"name\":\"" + esc(name) + "\"}";
    }

    private static String ctor(String name, List<String> args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"ctor\",\"name\":\"")
            .append(esc(name))
            .append("\",\"args\":[");
        for (int i = 0; i < args.size(); i++) {
            if (i > 0) {
                sb.append(",");
            }
            sb.append(args.get(i));
        }
        return sb.append("]}").toString();
    }

    private static String atom(String name, List<String> args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\"")
            .append(esc(name))
            .append("\",\"args\":[");
        for (int i = 0; i < args.size(); i++) {
            if (i > 0) {
                sb.append(",");
            }
            sb.append(args.get(i));
        }
        return sb.append("]}").toString();
    }

    private static String connective(String kind, List<String> operands) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"")
            .append(esc(kind))
            .append("\",\"operands\":[");
        for (int i = 0; i < operands.size(); i++) {
            if (i > 0) {
                sb.append(",");
            }
            sb.append(operands.get(i));
        }
        return sb.append("]}").toString();
    }

    private static String stringArray(List<String> values) {
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < values.size(); i++) {
            if (i > 0) {
                sb.append(",");
            }
            sb.append("\"").append(esc(values.get(i))).append("\"");
        }
        return sb.append("]").toString();
    }

    private static String sortArray(List<String> sorts) {
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < sorts.size(); i++) {
            if (i > 0) {
                sb.append(",");
            }
            sb.append(sorts.get(i));
        }
        return sb.append("]").toString();
    }

    private static String esc(String s) {
        return s
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\n")
            .replace("\r", "\\r")
            .replace("\t", "\\t");
    }

    private static final class RawContractDecl extends ContractDecl {
        private final String json;

        RawContractDecl(String symbol, String json) {
            super(symbol, List.of(), List.of(), List.of());
            this.json = json;
        }

        @Override
        public String toJson() {
            return json;
        }
    }
}
