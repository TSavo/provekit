package com.provekit.emit.assertj;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.provekit.ir.Jcs;

/**
 * Render a neutral term subtree (the {@code args} of a predicate) into a java
 * expression string.
 *
 * <p>The neutral term forms are the catalog's:
 * <ul>
 *   <li>{@code {"kind":"var","name":"x"}}            -> {@code x}</li>
 *   <li>{@code {"kind":"const","value":7,"sort":{...}}} -> {@code 7}</li>
 *   <li>{@code {"kind":"const","value":"hi",...}}     -> {@code "hi"}</li>
 *   <li>{@code {"kind":"const","value":null,...}}     -> {@code null}</li>
 *   <li>{@code {"kind":"ctor"|"op","name":"+","args":[a,b]}} (arithmetic)
 *       -> {@code (a + b)}</li>
 *   <li>{@code {"kind":"ctor"|"op","name":"foo","args":[a]}} (call)
 *       -> {@code foo(a)}</li>
 * </ul>
 *
 * <p>Substrate-honest: a term shape this renderer does not understand yields
 * {@code Optional.empty()} so the caller can refuse rather than emit a
 * silently-wrong expression.
 */
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
        // Strip a concept: prefix if a nested op carries one (defensive).
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

        // Infix arithmetic operators.
        if (ARITHMETIC.contains(name) && args.size() == 2) {
            return Optional.of("(" + args.get(0) + " " + name + " " + args.get(1) + ")");
        }

        // Otherwise: a method/constructor call expression.
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
