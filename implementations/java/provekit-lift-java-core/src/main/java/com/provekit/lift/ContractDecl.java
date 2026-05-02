package com.provekit.lift;

import java.util.*;

public class ContractDecl {
    public final String symbol;
    public final List<String> preconditions;
    public final List<String> postconditions;
    public final List<String> invariants;

    public ContractDecl(String symbol, List<String> pres, List<String> posts) {
        this(symbol, pres, posts, List.of());
    }

    public ContractDecl(String symbol, List<String> pres, List<String> posts, List<String> invs) {
        this.symbol = symbol;
        this.preconditions = pres;
        this.postconditions = posts;
        this.invariants = invs;
    }

    public String toJson() {
        StringBuilder sb = new StringBuilder();
        sb.append("{\"kind\":\"contract\",\"symbol\":\"").append(symbol).append("\"");
        if (!preconditions.isEmpty()) sb.append(",\"precondition\":").append(buildAnd(preconditions));
        if (!postconditions.isEmpty()) sb.append(",\"postcondition\":").append(buildAnd(postconditions));
        if (!invariants.isEmpty()) sb.append(",\"invariant\":").append(buildAnd(invariants));
        sb.append("}");
        return sb.toString();
    }

    private String buildAnd(List<String> parts) {
        if (parts.size() == 1) return parts.get(0);
        StringBuilder sb = new StringBuilder("{\"kind\":\"and\",\"operands\":[");
        for (int i = 0; i < parts.size(); i++) {
            if (i > 0) sb.append(",");
            sb.append(parts.get(i));
        }
        sb.append("]}");
        return sb.toString();
    }
}
