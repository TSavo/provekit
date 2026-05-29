package com.provekit.emit.testng;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.provekit.ir.Jcs;

/** Render a neutral term subtree into a Java expression string. */
final class TermRenderer {
    private TermRenderer() {}

    private static final java.util.Set<String> ARITHMETIC =
        java.util.Set.of("+", "-", "*", "/", "%");

    static Optional<String> render(Jcs.Json term) {
        if (!(term instanceof Jcs.Obj obj)) return Optional.empty();
        String kind = obj.stringFieldOrNull("kind");
        if (kind == null) return Optional.empty();
        switch (kind) {
            case "var":
                return renderVar(obj);
            case "const":
                return renderConst(obj);
            case "ctor":
            case "op":
                return renderApplication(obj);
            default:
                return Optional.empty();
        }
    }

    private static Optional<String> renderVar(Jcs.Obj obj) {
        String name = obj.stringFieldOrNull("name");
        return (name == null || name.isBlank()) ? Optional.empty() : Optional.of(name);
    }

    private static Optional<String> renderConst(Jcs.Obj obj) {
        Jcs.Json value = obj.get("value");
        if (value == null) return Optional.empty();
        if (value instanceof Jcs.Null) return Optional.of("null");
        if (value instanceof Jcs.Bool b) return Optional.of(Boolean.toString(b.value()));
        if (value instanceof Jcs.Num n) return Optional.of(Long.toString(n.value()));
        if (value instanceof Jcs.Str s) return Optional.of(quote(s.value()));
        return Optional.empty();
    }

    private static Optional<String> renderApplication(Jcs.Obj obj) {
        String name = obj.stringFieldOrNull("name");
        if (name == null || name.isBlank()) return Optional.empty();
        if (name.startsWith("concept:")) name = name.substring("concept:".length());

        List<String> args = new ArrayList<>();
        Jcs.Json rawArgs = obj.get("args");
        if (rawArgs instanceof Jcs.Arr arr) {
            for (Jcs.Json a : arr.values()) {
                Optional<String> r = render(a);
                if (r.isEmpty()) return Optional.empty();
                args.add(r.get());
            }
        }

        if (ARITHMETIC.contains(name) && args.size() == 2) {
            return Optional.of("(" + args.get(0) + " " + name + " " + args.get(1) + ")");
        }

        StringBuilder sb = new StringBuilder(name).append('(');
        for (int i = 0; i < args.size(); i++) {
            if (i > 0) sb.append(", ");
            sb.append(args.get(i));
        }
        return Optional.of(sb.append(')').toString());
    }

    private static String quote(String s) {
        StringBuilder sb = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> sb.append(c);
            }
        }
        return sb.append('"').toString();
    }
}
