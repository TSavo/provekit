package com.provekit.emit.testng;

import java.util.Optional;

import com.provekit.ir.Jcs;

/**
 * INLINE predicate -> TestNG Assert mapping.
 *
 * <p>TestNG assertion spelling is Java framework knowledge owned by this
 * artifact. The Rust CLI sees only the neutral emit plan and the plugin RPC
 * result; unsupported predicates are refused by returning Optional.empty().
 */
final class PredicateAssertionTable {
    private PredicateAssertionTable() {}

    static Optional<String> render(Jcs.Obj predicate) {
        String head = headOf(predicate);
        if (head == null) return Optional.empty();
        java.util.List<Jcs.Json> args = argsOf(predicate);

        switch (head) {
            case "eq":
                return binary(args, (a, b) -> "Assert.assertEquals(" + a + ", " + b + ");");
            case "ne":
                return binary(args, (a, b) -> "Assert.assertNotEquals(" + a + ", " + b + ");");
            case "lt":
                return binary(args, (a, b) -> "Assert.assertTrue(" + a + " < " + b + ");");
            case "gt":
                return binary(args, (a, b) -> "Assert.assertTrue(" + a + " > " + b + ");");
            case "le":
                return binary(args, (a, b) -> "Assert.assertTrue(" + a + " <= " + b + ");");
            case "ge":
                return binary(args, (a, b) -> "Assert.assertTrue(" + a + " >= " + b + ");");
            case "option-is-some":
            case "not-null":
                return unary(args, x -> "Assert.assertNotNull(" + x + ");");
            case "option-is-none":
                return unary(args, x -> "Assert.assertNull(" + x + ");");
            case "fallible-err":
                return unary(args, x ->
                    "Assert.expectThrows(Exception.class, () -> { Object __thrown = " + x + "; });");
            default:
                return Optional.empty();
        }
    }

    static boolean supports(String head) {
        return normalizeHead(head) != null;
    }

    /**
     * The predicate head with concept prefix stripped and common ProofIR
     * operator aliases normalized, or null when malformed/unsupported.
     */
    static String headOf(Jcs.Obj predicate) {
        String name = predicate.stringFieldOrNull("name");
        if (name == null || name.isBlank()) {
            name = predicate.stringFieldOrNull("predicate");
        }
        return normalizeHead(name);
    }

    private static String normalizeHead(String head) {
        if (head == null || head.isBlank()) return null;
        String h = head.startsWith("concept:") ? head.substring("concept:".length()) : head;
        return switch (h) {
            case "eq", "=", "==" -> "eq";
            case "ne", "neq", "!=", "\u2260" -> "ne";
            case "lt", "<" -> "lt";
            case "gt", ">" -> "gt";
            case "le", "lte", "<=", "\u2264" -> "le";
            case "ge", "gte", ">=", "\u2265" -> "ge";
            case "option-is-some" -> "option-is-some";
            case "option-is-none" -> "option-is-none";
            case "not-null" -> "not-null";
            case "fallible-err" -> "fallible-err";
            default -> null;
        };
    }

    private static java.util.List<Jcs.Json> argsOf(Jcs.Obj predicate) {
        Jcs.Json args = predicate.get("args");
        if (args instanceof Jcs.Arr arr) return arr.values();
        return java.util.List.of();
    }

    private interface Binary {
        String render(String a, String b);
    }

    private interface Unary {
        String render(String x);
    }

    private static Optional<String> binary(java.util.List<Jcs.Json> args, Binary f) {
        if (args.size() != 2) return Optional.empty();
        Optional<String> a = TermRenderer.render(args.get(0));
        Optional<String> b = TermRenderer.render(args.get(1));
        if (a.isEmpty() || b.isEmpty()) return Optional.empty();
        return Optional.of(f.render(a.get(), b.get()));
    }

    private static Optional<String> unary(java.util.List<Jcs.Json> args, Unary f) {
        if (args.size() != 1) return Optional.empty();
        Optional<String> x = TermRenderer.render(args.get(0));
        if (x.isEmpty()) return Optional.empty();
        return Optional.of(f.render(x.get()));
    }
}
