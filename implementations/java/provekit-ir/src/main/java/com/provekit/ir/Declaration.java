package com.provekit.ir;

/**
 * IR Document declarations matching the CDDL spec.
 */
public sealed interface Declaration {
    String toJson();

    record Property(String name, Param[] params, Formula body) implements Declaration {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"kind\":\"property\",\"name\":\"" + Sort.escape(name) + "\",\"params\":[");
            for (int i = 0; i < params.length; i++) {
                if (i > 0) sb.append(",");
                sb.append(params[i].toJson());
            }
            sb.append("],\"body\":").append(body.toJson()).append("}");
            return sb.toString();
        }
    }

    record Param(String name, Sort sort) {
        public String toJson() {
            return "{\"name\":\"" + Sort.escape(name) + "\",\"sort\":" + sort.toJson() + "}";
        }
    }

    record Bridge(String sourceSymbol, String sourceContractCid, String targetContractCid, String evidence) implements Declaration {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"kind\":\"bridge\",\"sourceSymbol\":\"" + Sort.escape(sourceSymbol) + "\",\"sourceContractCid\":\"" + Sort.escape(sourceContractCid) + "\",\"targetContractCid\":\"" + Sort.escape(targetContractCid) + "\"");
            if (evidence != null) sb.append(",\"evidence\":\"").append(Sort.escape(evidence)).append("\"");
            sb.append("}");
            return sb.toString();
        }
    }

    record Contract(String symbol, Formula precondition, Formula postcondition, Formula invariant, String evidence) implements Declaration {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"kind\":\"contract\",\"symbol\":\"" + Sort.escape(symbol) + "\"");
            if (precondition != null) sb.append(",\"precondition\":").append(precondition.toJson());
            sb.append(",\"postcondition\":").append(postcondition.toJson());
            if (invariant != null) sb.append(",\"invariant\":").append(invariant.toJson());
            if (evidence != null) sb.append(",\"evidence\":\"").append(Sort.escape(evidence)).append("\"");
            sb.append("}");
            return sb.toString();
        }
    }
}
