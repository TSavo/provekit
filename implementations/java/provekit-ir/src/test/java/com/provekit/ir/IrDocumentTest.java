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
        Declaration.Bridge bridge = new Declaration.Bridge(
            "myBridge",
            "source",
            "c-kit",
            "bafySource",
            "bafyTarget",
            "bafyProof",
            "coq",
            null
        );

        String json = bridge.toJson();
        assertTrue(json.contains("\"kind\":\"bridge\""));
        assertTrue(json.contains("\"name\":\"myBridge\""));
        assertTrue(json.contains("\"sourceSymbol\":\"source\""));
        assertTrue(json.contains("\"sourceLayer\":\"c-kit\""));
        assertTrue(json.contains("\"targetProofCid\":\"bafyProof\""));
        assertTrue(json.contains("\"targetLayer\":\"coq\""));
    }

    /**
     * Spec v1.1.0 — bridge_decl_v1_1 conformance fixture from conformance/fixtures.toml.
     * The 9-field Bridge with optional notes must serialize to JCS-canonical
     * (alphabetical key order) bytes that match the canonical fixture exactly.
     */
    @Test
    public void testBridgeJcsRoundtripV110() {
        Declaration.Bridge bridge = new Declaration.Bridge(
            "myBridge",
            "source",
            "c-kit",
            "bafySource",
            "bafyTarget",
            "bafyProof",
            "coq",
            "some notes"
        );

        String got = bridge.toJson();
        String expected = "{\"kind\":\"bridge\",\"name\":\"myBridge\",\"notes\":\"some notes\",\"sourceContractCid\":\"bafySource\",\"sourceLayer\":\"c-kit\",\"sourceSymbol\":\"source\",\"targetContractCid\":\"bafyTarget\",\"targetLayer\":\"coq\",\"targetProofCid\":\"bafyProof\"}";

        assertEquals(expected, got, "Bridge JCS bytes must match conformance/fixtures.toml bridge_decl_v1_1");
    }
}
