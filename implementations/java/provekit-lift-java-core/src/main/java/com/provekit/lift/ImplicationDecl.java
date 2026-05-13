package com.provekit.lift;

public class ImplicationDecl {
    public final String name;
    public final String antecedent;
    public final String consequent;
    public final String antecedentSlot;
    public final String consequentSlot;
    public final String prover;
    public final String proofWitness;

    public ImplicationDecl(
            String name,
            String antecedent,
            String consequent,
            String antecedentSlot,
            String consequentSlot,
            String prover,
            String proofWitness) {
        this.name = name;
        this.antecedent = antecedent;
        this.consequent = consequent;
        this.antecedentSlot = antecedentSlot;
        this.consequentSlot = consequentSlot;
        this.prover = prover;
        this.proofWitness = proofWitness;
    }

    public String toJson() {
        return "{"
            + "\"name\":\"" + esc(name) + "\","
            + "\"antecedent\":\"" + esc(antecedent) + "\","
            + "\"consequent\":\"" + esc(consequent) + "\","
            + "\"antecedentSlot\":\"" + esc(antecedentSlot) + "\","
            + "\"consequentSlot\":\"" + esc(consequentSlot) + "\","
            + "\"prover\":\"" + esc(prover) + "\","
            + "\"proofWitness\":\"" + esc(proofWitness) + "\""
            + "}";
    }

    private static String esc(String s) {
        return s
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\n")
            .replace("\r", "\\r")
            .replace("\t", "\\t");
    }
}
