package com.provekit.ir;

/**
 * Top-level IR Document matching the CDDL spec.
 */
public class IrDocument {
    private final String version;
    private final Declaration[] declarations;

    public IrDocument(String version, Declaration[] declarations) {
        this.version = version;
        this.declarations = declarations;
    }

    public IrDocument(Declaration[] declarations) {
        this("provekit-ir/1.1.0", declarations);
    }

    public String toJson() {
        StringBuilder sb = new StringBuilder("{\"version\":\"" + Sort.escape(version) + "\",\"declarations\":[");
        for (int i = 0; i < declarations.length; i++) {
            if (i > 0) sb.append(",");
            sb.append(declarations[i].toJson());
        }
        sb.append("]}");
        return sb.toString();
    }

    // Builder pattern
    public static Builder builder() { return new Builder(); }

    public static class Builder {
        private String version = "provekit-ir/1.1.0";
        private final java.util.List<Declaration> declarations = new java.util.ArrayList<>();

        public Builder version(String v) { this.version = v; return this; }
        public Builder property(String name, Declaration.Param[] params, Formula body) {
            declarations.add(new Declaration.Property(name, params, body));
            return this;
        }
        /** v1.1.0 9-field bridge with no notes. */
        public Builder bridge(
            String name,
            String sourceSymbol,
            String sourceLayer,
            String sourceContractCid,
            String targetContractCid,
            String targetProofCid,
            String targetLayer
        ) {
            declarations.add(new Declaration.Bridge(
                name, sourceSymbol, sourceLayer,
                sourceContractCid, targetContractCid, targetProofCid, targetLayer,
                null));
            return this;
        }
        /** v1.1.0 9-field bridge including optional notes. */
        public Builder bridge(
            String name,
            String sourceSymbol,
            String sourceLayer,
            String sourceContractCid,
            String targetContractCid,
            String targetProofCid,
            String targetLayer,
            String notes
        ) {
            declarations.add(new Declaration.Bridge(
                name, sourceSymbol, sourceLayer,
                sourceContractCid, targetContractCid, targetProofCid, targetLayer,
                notes));
            return this;
        }
        public Builder contract(String symbol, Formula precondition, Formula postcondition) {
            declarations.add(new Declaration.Contract(symbol, precondition, postcondition, null, null));
            return this;
        }
        public Builder contract(String symbol, Formula precondition, Formula postcondition, Formula invariant) {
            declarations.add(new Declaration.Contract(symbol, precondition, postcondition, invariant, null));
            return this;
        }
        public Builder contract(String symbol, Formula precondition, Formula postcondition, Formula invariant, String evidence) {
            declarations.add(new Declaration.Contract(symbol, precondition, postcondition, invariant, evidence));
            return this;
        }
        public IrDocument build() {
            return new IrDocument(version, declarations.toArray(new Declaration[0]));
        }
    }
}
