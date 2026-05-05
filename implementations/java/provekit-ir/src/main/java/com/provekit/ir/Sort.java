package com.provekit.ir;

/**
 * Sealed sort hierarchy matching the CDDL spec.
 */
public sealed interface Sort {
    String toJson();

    record Primitive(String name) implements Sort {
        public String toJson() {
            return "{\"kind\":\"primitive\",\"name\":\"" + escape(name) + "\"}";
        }
    }

    record Set(Sort element) implements Sort {
        public String toJson() {
            return "{\"kind\":\"set\",\"element\":" + element.toJson() + "}";
        }
    }

    record Tuple(Sort[] elements) implements Sort {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"kind\":\"tuple\",\"elements\":[");
            for (int i = 0; i < elements.length; i++) {
                if (i > 0) sb.append(",");
                sb.append(elements[i].toJson());
            }
            sb.append("]}");
            return sb.toString();
        }
    }

    record Function(Sort[] args, Sort ret) implements Sort {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"kind\":\"function\",\"args\":[");
            for (int i = 0; i < args.length; i++) {
                if (i > 0) sb.append(",");
                sb.append(args[i].toJson());
            }
            sb.append("],\"return\":");
            sb.append(ret.toJson());
            sb.append("}");
            return sb.toString();
        }
    }

    record Dependent(String name, String indexVar, Sort indexSort) implements Sort {
        public String toJson() {
            return "{\"kind\":\"dependent\",\"name\":\"" + escape(name) + "\",\"indexVar\":\"" + escape(indexVar) + "\",\"indexSort\":" + indexSort.toJson() + "}";
        }

        public String kind() { return "dependent"; }
    }

    Sort Bool = new Primitive("Bool");
    Sort Int = new Primitive("Int");
    Sort Real = new Primitive("Real");
    Sort String = new Primitive("String");
    Sort Ref = new Primitive("Ref");
    Sort Node = new Primitive("Node");
    Sort Edge = new Primitive("Edge");

    static String escape(String s) {
        return s.replace("\\", "\\\\").replace("\"", "\\\"");
    }
}
