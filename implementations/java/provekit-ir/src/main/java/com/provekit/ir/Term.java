package com.provekit.ir;

/**
 * Sealed term hierarchy matching the CDDL spec.
 */
public sealed interface Term {
    String toJson();

    record Var(String name, Sort sort) implements Term {
        public String toJson() {
            return "{\"kind\":\"var\",\"name\":\"" + Sort.escape(name) + "\"}";
        }
    }

    record Const(Value value, Sort sort) implements Term {
        public String toJson() {
            return "{\"kind\":\"const\",\"sort\":" + sort.toJson() + ",\"value\":" + value.toJson() + "}";
        }
    }

    record Ctor(String name, Term[] args, Sort sort) implements Term {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"args\":[");
            for (int i = 0; i < args.length; i++) {
                if (i > 0) sb.append(",");
                sb.append(args[i].toJson());
            }
            sb.append("],\"kind\":\"ctor\",\"name\":\"").append(Sort.escape(name)).append("\"}");
            return sb.toString();
        }
    }

    record Lambda(String paramName, Sort paramSort, Term body, Sort sort) implements Term {
        public String toJson() {
            return "{\"kind\":\"lambda\",\"paramName\":\"" + Sort.escape(paramName) + "\",\"paramSort\":" + paramSort.toJson() + ",\"body\":" + body.toJson() + "}";
        }
    }

    record Let(LetBinding[] bindings, Term body, Sort sort) implements Term {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"kind\":\"let\",\"bindings\":[");
            for (int i = 0; i < bindings.length; i++) {
                if (i > 0) sb.append(",");
                sb.append(bindings[i].toJson());
            }
            sb.append("],\"body\":").append(body.toJson()).append("}");
            return sb.toString();
        }
    }

    record LetBinding(String name, Term boundTerm) {
        public String toJson() {
            return "{\"name\":\"" + Sort.escape(name) + "\",\"boundTerm\":" + boundTerm.toJson() + "}";
        }
    }

    sealed interface Value {
        String toJson();
        record Int(long value) implements Value {
            public String toJson() { return String.valueOf(value); }
        }
        record Str(String value) implements Value {
            public String toJson() { return "\"" + Sort.escape(value) + "\""; }
        }
        record Bool(boolean value) implements Value {
            public String toJson() { return String.valueOf(value); }
        }
        record Real(double value) implements Value {
            public String toJson() { return String.valueOf(value); }
        }
    }

    // Convenience constructors
    static Term var_(String name, Sort sort) { return new Var(name, sort); }
    static Term var_(String name) { return new Var(name, Sort.Ref); }
    static Term const_(long value, Sort sort) { return new Const(new Value.Int(value), sort); }
    static Term const_(String value, Sort sort) { return new Const(new Value.Str(value), sort); }
    static Term const_(boolean value, Sort sort) { return new Const(new Value.Bool(value), sort); }
    static Term const_(double value, Sort sort) { return new Const(new Value.Real(value), sort); }
    static Term ctor(String name, Term[] args, Sort sort) { return new Ctor(name, args, sort); }
    static Term lambda(String paramName, Sort paramSort, Term body, Sort sort) { return new Lambda(paramName, paramSort, body, sort); }
    static Term let(LetBinding[] bindings, Term body, Sort sort) { return new Let(bindings, body, sort); }
    static LetBinding binding(String name, Term boundTerm) { return new LetBinding(name, boundTerm); }
}
