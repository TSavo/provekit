package com.provekit.lift;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.NodeList;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.github.javaparser.ast.body.Parameter;
import com.github.javaparser.ast.body.VariableDeclarator;
import com.github.javaparser.ast.expr.*;
import com.github.javaparser.ast.stmt.*;
import com.provekit.ir.Blake3;
import com.provekit.ir.Jcs;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

public final class JavaAstTemplates {
    private JavaAstTemplates() {}

    public record TemplateInfo(
            Jcs.Json astTemplate,
            String compactJson,
            String templateCid,
            List<String> paramNames) {}

    public static TemplateInfo fromMethodSource(String methodSource) {
        String wrapped = "class __ProveKitTemplate {\n" + methodSource + "\n}\n";
        ParseResult<CompilationUnit> result = new JavaParser().parse(wrapped);
        if (!result.isSuccessful() || result.getResult().isEmpty()) {
            throw new IllegalArgumentException("cannot parse method source: " + result.getProblems());
        }
        return result.getResult().get().findFirst(MethodDeclaration.class)
            .map(JavaAstTemplates::fromMethod)
            .orElseThrow(() -> new IllegalArgumentException("method source contains no method"));
    }

    public static TemplateInfo fromMethod(MethodDeclaration method) {
        List<String> params = paramNames(method);
        Jcs.Json parsed = method.getBody()
            .map(body -> blockToTemplate(body, params))
            .orElseGet(() -> object("kind", Jcs.string("block"), "stmts", Jcs.array()));
        String compact = compactJson(parsed);
        return new TemplateInfo(
            Jcs.raw(compact, parsed),
            compact,
            Blake3.blake3_512(compact.getBytes(StandardCharsets.UTF_8)),
            params
        );
    }

    public static List<String> paramNames(MethodDeclaration method) {
        return method.getParameters().stream()
            .map(Parameter::getNameAsString)
            .toList();
    }

    public static String compactJson(Jcs.Json json) {
        if (json instanceof Jcs.Raw raw) {
            return raw.json();
        }
        StringBuilder out = new StringBuilder();
        writeCompact(json, out);
        return out.toString();
    }

    private static Jcs.Json blockToTemplate(BlockStmt block, List<String> params) {
        List<Jcs.Json> stmts = new ArrayList<>();
        for (Statement stmt : block.getStatements()) {
            stmts.addAll(stmtToTemplates(stmt, params));
        }
        return object("kind", Jcs.string("block"), "stmts", Jcs.array(stmts));
    }

    private static List<Jcs.Json> stmtToTemplates(Statement stmt, List<String> params) {
        if (stmt instanceof ExpressionStmt expressionStmt) {
            Expression expr = unwrap(expressionStmt.getExpression());
            if (expr instanceof VariableDeclarationExpr declaration) {
                List<Jcs.Json> lets = new ArrayList<>();
                for (VariableDeclarator variable : declaration.getVariables()) {
                    lets.add(object(
                        "kind", Jcs.string("let"),
                        "pat", patToTemplate(variable.getNameAsString(), params),
                        "init", variable.getInitializer()
                            .map(init -> exprToTemplate(init, params))
                            .orElseGet(Jcs::nullValue)
                    ));
                }
                return lets;
            }
            return List.of(object(
                "kind", Jcs.string("expr_stmt"),
                "expr", exprToTemplate(expr, params),
                "trailing_semi", Jcs.bool(true)
            ));
        }
        if (stmt instanceof ReturnStmt returnStmt) {
            return List.of(object(
                "kind", Jcs.string("return"),
                "expr", returnStmt.getExpression()
                    .map(expr -> exprToTemplate(expr, params))
                    .orElseGet(Jcs::nullValue)
            ));
        }
        if (stmt instanceof BlockStmt block) {
            return List.of(blockToTemplate(block, params));
        }
        return List.of(other(stmt));
    }

    private static Jcs.Json exprToTemplate(Expression expr, List<String> params) {
        expr = unwrap(expr);
        if (expr instanceof MethodCallExpr call) {
            List<Jcs.Json> args = call.getArguments().stream()
                .map(arg -> exprToTemplate(arg, params))
                .toList();
            Optional<Expression> scope = call.getScope();
            if (scope.isEmpty()) {
                return object(
                    "kind", Jcs.string("call"),
                    "func", object("kind", Jcs.string("ident"), "name", Jcs.string(call.getNameAsString())),
                    "args", Jcs.array(args)
                );
            }
            return object(
                "kind", Jcs.string("method_call"),
                "receiver", scopeToTemplate(scope.get(), params),
                "method", Jcs.string(call.getNameAsString()),
                "args", Jcs.array(args)
            );
        }
        if (expr instanceof NameExpr name) {
            String value = name.getNameAsString();
            int index = params.indexOf(value);
            if (index >= 0) {
                return object("kind", Jcs.string("param_ref"), "index", Jcs.integer(index + 1L));
            }
            return object("kind", Jcs.string("ident"), "name", Jcs.string(value));
        }
        if (expr instanceof FieldAccessExpr field) {
            List<String> segments = fieldAccessSegments(field);
            if (!segments.isEmpty()) {
                return path(segments);
            }
            return other(expr);
        }
        if (expr instanceof LiteralExpr literal) {
            return litToTemplate(literal);
        }
        if (expr instanceof BinaryExpr binary) {
            return object(
                "kind", Jcs.string("binary"),
                "op", Jcs.string(binaryOp(binary.getOperator())),
                "left", exprToTemplate(binary.getLeft(), params),
                "right", exprToTemplate(binary.getRight(), params)
            );
        }
        if (expr instanceof ArrayInitializerExpr array) {
            return arrayTemplate(array.getValues(), params);
        }
        if (expr instanceof ArrayCreationExpr array) {
            if (array.getInitializer().isPresent()) {
                return arrayTemplate(array.getInitializer().get().getValues(), params);
            }
            return other(expr);
        }
        if (expr instanceof UnaryExpr unary) {
            return object(
                "kind", Jcs.string("unary"),
                "op", Jcs.string(unaryOp(unary.getOperator())),
                "expr", exprToTemplate(unary.getExpression(), params)
            );
        }
        if (expr instanceof ThisExpr) {
            return object("kind", Jcs.string("ident"), "name", Jcs.string("this"));
        }
        if (expr instanceof SuperExpr) {
            return object("kind", Jcs.string("ident"), "name", Jcs.string("super"));
        }
        return other(expr);
    }

    private static Jcs.Json scopeToTemplate(Expression scope, List<String> params) {
        scope = unwrap(scope);
        if (scope instanceof NameExpr name && looksLikeTypeName(name.getNameAsString())) {
            return path(List.of(name.getNameAsString()));
        }
        return exprToTemplate(scope, params);
    }

    private static Jcs.Json arrayTemplate(NodeList<Expression> values, List<String> params) {
        return object(
            "kind", Jcs.string("array"),
            "elems", Jcs.array(values.stream().map(value -> exprToTemplate(value, params)).toList())
        );
    }

    private static Jcs.Json litToTemplate(LiteralExpr literal) {
        if (literal instanceof StringLiteralExpr stringLiteral) {
            return lit("str", Jcs.string(stringLiteral.getValue()));
        }
        if (literal instanceof TextBlockLiteralExpr textBlock) {
            return lit("str", Jcs.string(textBlock.getValue()));
        }
        if (literal instanceof IntegerLiteralExpr integerLiteral) {
            return lit("int", Jcs.integer(integerLiteral.asNumber().longValue()));
        }
        if (literal instanceof LongLiteralExpr longLiteral) {
            return lit("long", Jcs.integer(longLiteral.asNumber().longValue()));
        }
        if (literal instanceof BooleanLiteralExpr booleanLiteral) {
            return lit("bool", Jcs.bool(booleanLiteral.getValue()));
        }
        if (literal instanceof CharLiteralExpr charLiteral) {
            return lit("char", Jcs.string(charLiteral.getValue()));
        }
        if (literal instanceof NullLiteralExpr) {
            return lit("null", Jcs.nullValue());
        }
        if (literal instanceof DoubleLiteralExpr doubleLiteral) {
            String raw = numericLiteralJson(doubleLiteral.getValue());
            String ty = doubleLiteral.getValue().toLowerCase(java.util.Locale.ROOT).endsWith("f")
                ? "float"
                : "double";
            return lit(ty, Jcs.raw(raw, null));
        }
        return other(literal);
    }

    private static Jcs.Json lit(String ty, Jcs.Json value) {
        return object("kind", Jcs.string("lit"), "ty", Jcs.string(ty), "value", value);
    }

    private static Jcs.Json patToTemplate(String name, List<String> params) {
        int index = params.indexOf(name);
        if (index >= 0) {
            return object("kind", Jcs.string("param_ref"), "index", Jcs.integer(index + 1L));
        }
        return object("kind", Jcs.string("binding"), "name", Jcs.string(name));
    }

    private static Jcs.Json path(List<String> segments) {
        return object(
            "kind", Jcs.string("path"),
            "segments", Jcs.array(segments.stream().map(Jcs::string).toList())
        );
    }

    private static List<String> fieldAccessSegments(FieldAccessExpr field) {
        List<String> reversed = new ArrayList<>();
        Expression current = field;
        while (current instanceof FieldAccessExpr access) {
            reversed.add(access.getNameAsString());
            current = access.getScope();
        }
        if (current instanceof NameExpr root) {
            reversed.add(root.getNameAsString());
            java.util.Collections.reverse(reversed);
            return reversed;
        }
        return List.of();
    }

    private static Jcs.Json other(com.github.javaparser.ast.Node node) {
        return object(
            "kind", Jcs.string("other"),
            "variant", Jcs.string(node.getClass().getSimpleName())
        );
    }

    private static Expression unwrap(Expression expr) {
        while (expr instanceof EnclosedExpr enclosed) {
            expr = enclosed.getInner();
        }
        return expr;
    }

    private static boolean looksLikeTypeName(String name) {
        return !name.isEmpty() && Character.isUpperCase(name.codePointAt(0));
    }

    private static String binaryOp(BinaryExpr.Operator op) {
        return switch (op) {
            case PLUS -> "Add";
            case MINUS -> "Sub";
            case MULTIPLY -> "Mul";
            case DIVIDE -> "Div";
            case REMAINDER -> "Rem";
            case AND -> "And";
            case OR -> "Or";
            case XOR -> "BitXor";
            case BINARY_AND -> "BitAnd";
            case BINARY_OR -> "BitOr";
            case LEFT_SHIFT -> "Shl";
            case SIGNED_RIGHT_SHIFT, UNSIGNED_RIGHT_SHIFT -> "Shr";
            case EQUALS -> "Eq";
            case LESS -> "Lt";
            case LESS_EQUALS -> "Le";
            case NOT_EQUALS -> "Ne";
            case GREATER_EQUALS -> "Ge";
            case GREATER -> "Gt";
        };
    }

    private static String unaryOp(UnaryExpr.Operator op) {
        return switch (op) {
            case PLUS -> "Plus";
            case MINUS -> "Neg";
            case LOGICAL_COMPLEMENT, BITWISE_COMPLEMENT -> "Not";
            case PREFIX_INCREMENT, POSTFIX_INCREMENT -> "Inc";
            case PREFIX_DECREMENT, POSTFIX_DECREMENT -> "Dec";
        };
    }

    private static String numericLiteralJson(String value) {
        String normalized = value.replace("_", "");
        if (normalized.endsWith("f") || normalized.endsWith("F")
                || normalized.endsWith("d") || normalized.endsWith("D")) {
            normalized = normalized.substring(0, normalized.length() - 1);
        }
        return normalized;
    }

    private static Jcs.Obj object(Object... keyValues) {
        return Jcs.object(keyValues);
    }

    private static void writeCompact(Jcs.Json json, StringBuilder out) {
        if (json instanceof Jcs.Null) {
            out.append("null");
        } else if (json instanceof Jcs.Bool bool) {
            out.append(bool.value() ? "true" : "false");
        } else if (json instanceof Jcs.Num number) {
            out.append(number.value());
        } else if (json instanceof Jcs.Str string) {
            writeString(string.value(), out);
        } else if (json instanceof Jcs.Raw raw) {
            out.append(raw.json());
        } else if (json instanceof Jcs.Arr array) {
            out.append('[');
            boolean first = true;
            for (Jcs.Json item : array.values()) {
                if (!first) out.append(',');
                first = false;
                writeCompact(item, out);
            }
            out.append(']');
        } else if (json instanceof Jcs.Obj obj) {
            out.append('{');
            boolean first = true;
            for (Jcs.Field field : obj.fields()) {
                if (!first) out.append(',');
                first = false;
                writeString(field.key(), out);
                out.append(':');
                writeCompact(field.value(), out);
            }
            out.append('}');
        } else {
            throw new IllegalArgumentException("unknown JSON value");
        }
    }

    private static void writeString(String value, StringBuilder out) {
        out.append('"');
        int i = 0;
        while (i < value.length()) {
            int cp = value.codePointAt(i);
            i += Character.charCount(cp);
            switch (cp) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\b' -> out.append("\\b");
                case '\f' -> out.append("\\f");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> {
                    if (cp < 0x20) {
                        out.append("\\u");
                        String hex = Integer.toHexString(cp);
                        for (int pad = hex.length(); pad < 4; pad++) out.append('0');
                        out.append(hex);
                    } else {
                        out.appendCodePoint(cp);
                    }
                }
            }
        }
        out.append('"');
    }
}
