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

    /**
     * BridgeDeclaration per IR formal grammar v1.1.0
     * (protocol/specs/2026-04-30-ir-formal-grammar.md).
     *
     * Required fields: name, sourceSymbol, sourceLayer, sourceContractCid,
     * targetContractCid, targetProofCid, targetLayer.
     * Optional: notes (omitted from output when null).
     *
     * JCS canonical key order (RFC 8785, alphabetical by code unit):
     * kind, name, [notes,] sourceContractCid, sourceLayer, sourceSymbol,
     * targetContractCid, targetLayer, targetProofCid.
     */
    record Bridge(
        String name,
        String sourceSymbol,
        String sourceLayer,
        String sourceContractCid,
        String targetContractCid,
        String targetProofCid,
        String targetLayer,
        String notes
    ) implements Declaration {
        public String toJson() {
            StringBuilder sb = new StringBuilder();
            sb.append("{\"kind\":\"bridge\"");
            sb.append(",\"name\":\"").append(Sort.escape(name)).append("\"");
            if (notes != null) {
                sb.append(",\"notes\":\"").append(Sort.escape(notes)).append("\"");
            }
            sb.append(",\"sourceContractCid\":\"").append(Sort.escape(sourceContractCid)).append("\"");
            sb.append(",\"sourceLayer\":\"").append(Sort.escape(sourceLayer)).append("\"");
            sb.append(",\"sourceSymbol\":\"").append(Sort.escape(sourceSymbol)).append("\"");
            sb.append(",\"targetContractCid\":\"").append(Sort.escape(targetContractCid)).append("\"");
            sb.append(",\"targetLayer\":\"").append(Sort.escape(targetLayer)).append("\"");
            sb.append(",\"targetProofCid\":\"").append(Sort.escape(targetProofCid)).append("\"");
            sb.append("}");
            return sb.toString();
        }
    }

    record Contract(String name, String outBinding, Formula pre, Formula post, Formula inv, String evidence) implements Declaration {
        public String toJson() {
            StringBuilder sb = new StringBuilder("{\"kind\":\"contract\",\"name\":\"" + Sort.escape(name) + "\"");
            sb.append(",\"outBinding\":\"").append(Sort.escape(outBinding)).append("\"");
            if (pre != null) sb.append(",\"pre\":").append(pre.toJson());
            if (post != null) sb.append(",\"post\":").append(post.toJson());
            if (inv != null) sb.append(",\"inv\":").append(inv.toJson());
            if (evidence != null) sb.append(",\"evidence\":\"").append(Sort.escape(evidence)).append("\"");
            sb.append("}");
            return sb.toString();
        }
    }
}
