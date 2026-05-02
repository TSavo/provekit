package com.provekit.ir;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class IrDocumentTest {
    @Test
    public void testSimpleContract() {
        Term x = Term.var_("x", Sort.Int);
        Term zero = Term.const_(0, Sort.Int);
        Formula post = Formula.atomic("gte", x, zero);

        IrDocument doc = IrDocument.builder()
            .contract("abs", null, post)
            .build();

        String json = doc.toJson();
        assertTrue(json.contains("\"version\":\"provekit-ir/1.1.0\""));
        assertTrue(json.contains("\"kind\":\"contract\""));
        assertTrue(json.contains("\"symbol\":\"abs\""));
        assertTrue(json.contains("\"kind\":\"atomic\""));
        assertTrue(json.contains("\"name\":\"gte\""));
    }

    @Test
    public void testQuantifier() {
        Term x = Term.var_("x", Sort.Int);
        Formula body = Formula.atomic("gte", x, Term.const_(0, Sort.Int));
        Formula forall = Formula.forall("x", Sort.Int, body);

        IrDocument doc = IrDocument.builder()
            .contract("nonNegative", null, forall)
            .build();

        String json = doc.toJson();
        assertTrue(json.contains("\"kind\":\"forall\""));
        assertTrue(json.contains("\"name\":\"x\""));
    }

    @Test
    public void testBridge() {
        IrDocument doc = IrDocument.builder()
            .bridge("parseInt", "bafy...js", "bafy...java")
            .build();

        String json = doc.toJson();
        assertTrue(json.contains("\"kind\":\"bridge\""));
        assertTrue(json.contains("\"sourceSymbol\":\"parseInt\""));
    }
}
