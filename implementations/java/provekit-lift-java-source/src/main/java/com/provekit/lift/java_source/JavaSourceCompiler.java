package com.provekit.lift.java_source;

import com.provekit.ir.Jcs;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Collectors;
import java.util.stream.IntStream;

public final class JavaSourceCompiler {
    private final List<String> lines = new ArrayList<>();
    private int indent;

    public String compile(Jcs.Json ir) {
        if (ir instanceof Jcs.Obj obj && "java:source-unit".equals(obj.stringFieldOrNull("name"))) {
            Jcs.Json bytes = obj.arrayField("args").get(0);
            if (bytes instanceof Jcs.Str s) return s.value();
            if (bytes instanceof Jcs.Obj term && "const".equals(term.stringFieldOrNull("kind"))) {
                Jcs.Json value = term.get("value");
                if (value instanceof Jcs.Str s) return s.value();
            }
        }
        lines.clear();
        indent = 0;
        emitStmt(ir);
        String params = IntStream.range(0, 2).mapToObj(i -> "int x" + i).collect(Collectors.joining(", "));
        return "class Lifted {\n  int f(" + params + ") {\n" + String.join("\n", lines) + "\n  }\n}\n";
    }

    private void emitStmt(Jcs.Json node) {
        if (!(node instanceof Jcs.Obj obj)) {
            line("(" + emitExpr(node) + ");");
            return;
        }
        String name = obj.stringFieldOrNull("name");
        if (name == null) {
            line("(" + emitExpr(node) + ");");
            return;
        }
        Jcs.Arr args = obj.arrayField("args");
        switch (name) {
            case "java:seq" -> {
                emitStmt(args.get(0));
                emitStmt(args.get(1));
            }
            case "java:return" -> line("return " + emitExpr(args.get(0)) + ";");
            case "java:decl" -> line("int " + stringValue(args.get(0)) + " = " + emitExpr(args.get(1)) + ";");
            case "java:assign" -> line(emitExpr(args.get(0)) + " = " + emitExpr(args.get(1)) + ";");
            case "java:if" -> emitIf(args);
            case "java:while" -> emitWhile(args);
            case "java:for" -> emitFor(args);
            case "java:break" -> line("break;");
            case "java:continue" -> line("continue;");
            case "java:throw" -> line("throw " + emitExpr(args.get(0)) + ";");
            case "java:skip" -> line(";");
            default -> line("(" + emitExpr(node) + ");");
        }
    }

    private void emitIf(Jcs.Arr args) {
        line("if (" + emitExpr(args.get(0)) + ") {");
        indent++;
        emitStmt(args.get(1));
        indent--;
        line("} else {");
        indent++;
        emitStmt(args.get(2));
        indent--;
        line("}");
    }

    private void emitWhile(Jcs.Arr args) {
        line("while (" + emitExpr(args.get(0)) + ") {");
        indent++;
        emitStmt(args.get(1));
        indent--;
        line("}");
    }

    private void emitFor(Jcs.Arr args) {
        line("for (; " + emitExpr(args.get(1)) + "; ) {");
        indent++;
        emitStmt(args.get(3));
        emitStmt(args.get(2));
        indent--;
        line("}");
    }

    private String emitExpr(Jcs.Json node) {
        if (node instanceof Jcs.Str s) return quote(s.value());
        if (node instanceof Jcs.Num n) return Long.toString(n.value());
        if (node instanceof Jcs.Bool b) return b.value() ? "true" : "false";
        if (node instanceof Jcs.Null) return "null";
        if (!(node instanceof Jcs.Obj obj)) return "0";
        String kind = obj.stringFieldOrNull("kind");
        if ("const".equals(kind)) return emitExpr(obj.get("value"));
        if ("var".equals(kind)) return obj.stringField("name");
        String name = obj.stringFieldOrNull("name");
        if (name == null) return "0";
        Jcs.Arr args = obj.arrayField("args");
        return switch (name) {
            case "java:add" -> bin(args, "+");
            case "java:sub" -> bin(args, "-");
            case "java:mul" -> bin(args, "*");
            case "java:div" -> bin(args, "/");
            case "java:mod" -> bin(args, "%");
            case "java:eq" -> bin(args, "==");
            case "java:ne" -> bin(args, "!=");
            case "java:lt" -> bin(args, "<");
            case "java:le" -> bin(args, "<=");
            case "java:gt" -> bin(args, ">");
            case "java:ge" -> bin(args, ">=");
            case "java:and" -> bin(args, "&&");
            case "java:or" -> bin(args, "||");
            case "java:bitand" -> bin(args, "&");
            case "java:bitor" -> bin(args, "|");
            case "java:bitxor" -> bin(args, "^");
            case "java:shl" -> bin(args, "<<");
            case "java:shr" -> bin(args, ">>");
            case "java:ushr" -> bin(args, ">>>");
            case "java:neg" -> "(-" + emitExpr(args.get(0)) + ")";
            case "java:plus" -> "(+" + emitExpr(args.get(0)) + ")";
            case "java:not" -> "(!" + emitExpr(args.get(0)) + ")";
            case "java:bitnot" -> "(~" + emitExpr(args.get(0)) + ")";
            case "java:member" -> emitExpr(args.get(0)) + "." + stringValue(args.get(1));
            case "java:index" -> emitExpr(args.get(0)) + "[" + emitExpr(args.get(1)) + "]";
            case "java:cast" -> "(" + stringValue(args.get(0)) + ") (" + emitExpr(args.get(1)) + ")";
            case "java:ite" -> "(" + emitExpr(args.get(0)) + " ? " + emitExpr(args.get(1)) + " : " + emitExpr(args.get(2)) + ")";
            case "java:new" -> "new " + stringValue(args.get(0)) + "(" + args.values().subList(1, args.values().size()).stream().map(this::emitExpr).collect(Collectors.joining(", ")) + ")";
            case "java:call" -> stringValue(args.get(0)) + "(" + args.values().subList(1, args.values().size()).stream().map(this::emitExpr).collect(Collectors.joining(", ")) + ")";
            case "java:assign" -> "(" + emitExpr(args.get(0)) + " = " + emitExpr(args.get(1)) + ")";
            default -> "0";
        };
    }

    private String bin(Jcs.Arr args, String op) {
        return "(" + emitExpr(args.get(0)) + " " + op + " " + emitExpr(args.get(1)) + ")";
    }

    private String stringValue(Jcs.Json node) {
        if (node instanceof Jcs.Str s) return s.value();
        if (node instanceof Jcs.Obj obj && "const".equals(obj.stringFieldOrNull("kind")) && obj.get("value") instanceof Jcs.Str s) return s.value();
        return emitExpr(node);
    }

    private void line(String text) {
        lines.add("    " + "  ".repeat(indent) + text);
    }

    private static String quote(String value) {
        return "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n") + "\"";
    }
}
