package com.provekit.ir;

/**
 * Sealed formula hierarchy matching the CDDL spec.
 */
public sealed interface Formula {
    String toJson();

    record Atomic(String name, Term[] args) implements Formula {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\"" + Sort.escape(name) + "\",\"args\":[");
            for (int i = 0; i < args.length; i++) {
                if (i > 0) sb.append(",");
                sb.append(args[i].toJson());
            }
            sb.append("]}");
            return sb.toString();
        }
    }

    enum ConnectiveKind { and, or, not, implies }

    record Connective(ConnectiveKind kind, Formula[] operands) implements Formula {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"kind\":\"" + kind.name() + "\",\"operands\":[");
            for (int i = 0; i < operands.length; i++) {
                if (i > 0) sb.append(",");
                sb.append(operands[i].toJson());
            }
            sb.append("]}");
            return sb.toString();
        }
    }

    enum QuantifierKind { forall, exists }

    record Quantifier(QuantifierKind kind, String name, Sort sort, Formula body) implements Formula {
        public String toJson() {
            return "{\"kind\":\"" + kind.name() + "\",\"name\":\"" + Sort.escape(name) + "\",\"sort\":" + sort.toJson() + ",\"body\":" + body.toJson() + "}";
        }
    }

    record Choice(String varName, Sort sort, Formula body) implements Formula {
        public String toJson() {
            return "{\"kind\":\"choice\",\"varName\":\"" + Sort.escape(varName) + "\",\"sort\":" + sort.toJson() + ",\"body\":" + body.toJson() + "}";
        }
    }

    // Convenience constructors
    static Formula atomic(String name, Term... args) { return new Atomic(name, args); }
    static Formula and(Formula... operands) { return new Connective(ConnectiveKind.and, operands); }
    static Formula or(Formula... operands) { return new Connective(ConnectiveKind.or, operands); }
    static Formula not(Formula operand) { return new Connective(ConnectiveKind.not, new Formula[]{operand}); }
    static Formula implies(Formula left, Formula right) { return new Connective(ConnectiveKind.implies, new Formula[]{left, right}); }
    static Formula forall(String name, Sort sort, Formula body) { return new Quantifier(QuantifierKind.forall, name, sort, body); }
    static Formula exists(String name, Sort sort, Formula body) { return new Quantifier(QuantifierKind.exists, name, sort, body); }
    static Formula choice(String varName, Sort sort, Formula body) { return new Choice(varName, sort, body); }
}
