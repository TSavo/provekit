package com.provekit.ir;

/**
 * Call-edge memento per protocol/specs/2026-05-03-bridge-linkage-protocol.md §1 R1.
 *
 * JCS-canonical key order (RFC 8785, alphabetical by code unit):
 *   callSiteLocus, evidenceTerm, kind, schemaVersion, sourceContractCid,
 *   targetContractCid, targetSymbol
 *
 * Locus JCS-canonical key order: column, file, line
 *
 * targetContractCid is null for cross-kit calls (e.g. JNI); in that
 * case targetSymbol carries the kit-prefixed symbol name for linker
 * resolution per R3.
 */
public final class CallEdgeDecl {
    public final String sourceContractCid;
    /** null encodes as JSON null for cross-kit calls. */
    public final String targetContractCid;
    public final String targetSymbol;
    /** file, line, column */
    public final String locusFile;
    public final int locusLine;
    public final int locusColumn;
    /** Formula JSON for the evidence term (placeholder obligation). */
    public final String evidenceTermJson;

    public CallEdgeDecl(
        String sourceContractCid,
        String targetContractCid,
        String targetSymbol,
        String locusFile,
        int locusLine,
        int locusColumn,
        String evidenceTermJson
    ) {
        this.sourceContractCid = sourceContractCid;
        this.targetContractCid = targetContractCid;
        this.targetSymbol = targetSymbol;
        this.locusFile = locusFile;
        this.locusLine = locusLine;
        this.locusColumn = locusColumn;
        this.evidenceTermJson = evidenceTermJson;
    }

    /**
     * Serialize to JCS-canonical JSON matching Go's CallEdgeDeclaration.MarshalJSON.
     *
     * Key order:
     *   callSiteLocus: { column, file, line }
     *   evidenceTerm
     *   kind
     *   schemaVersion
     *   sourceContractCid
     *   targetContractCid  (null or "...")
     *   targetSymbol
     */
    public String toJson() {
        StringBuilder sb = new StringBuilder();
        sb.append("{\"callSiteLocus\":{\"column\":").append(locusColumn)
          .append(",\"file\":\"").append(Sort.escape(locusFile)).append("\"")
          .append(",\"line\":").append(locusLine).append("}");
        sb.append(",\"evidenceTerm\":").append(evidenceTermJson);
        sb.append(",\"kind\":\"call-edge\"");
        sb.append(",\"schemaVersion\":\"1\"");
        sb.append(",\"sourceContractCid\":\"").append(Sort.escape(sourceContractCid)).append("\"");
        if (targetContractCid == null) {
            sb.append(",\"targetContractCid\":null");
        } else {
            sb.append(",\"targetContractCid\":\"").append(Sort.escape(targetContractCid)).append("\"");
        }
        sb.append(",\"targetSymbol\":\"").append(Sort.escape(targetSymbol)).append("\"");
        sb.append("}");
        return sb.toString();
    }
}
