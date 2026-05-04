// SPDX-License-Identifier: Apache-2.0
//
// Smoke tests for the java self-contracts orchestrator. These tests
// exercise the full mint pipeline (slab authoring + memento minting +
// proof envelope build) and assert byte-determinism plus structural
// shape of the output. The cross-kit pinned-CID test lives in the
// rust integration suite (implementations/rust/provekit-cli/tests/
// mint_kit_integration.rs::java_kit_pins_expected_contract_set_cid).

package com.provekit.selfcontracts;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import com.provekit.selfcontracts.Slab.AuthoredSlab;

public class OrchestratorTest {

    /** Empty-set sentinel: blake3-512:d53d18c2... (the known-bad CID we must NOT produce). */
    private static final String EMPTY_SET_CID =
        "blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229";

    @Test
    void mintProducesNonEmptyContractSetCid(@TempDir Path tmp) throws IOException {
        Orchestrator.MintResult r = Orchestrator.mintOneRun(tmp);

        assertTrue(r.contractCount > 0,
            "must mint at least one contract; got " + r.contractCount);
        assertTrue(r.slabCount > 0,
            "must walk at least one slab; got " + r.slabCount);

        assertTrue(r.cid.startsWith("blake3-512:"),
            "catalog CID must be self-identifying: " + r.cid);
        assertTrue(r.contractSetCid.startsWith("blake3-512:"),
            "contractSetCid must be self-identifying: " + r.contractSetCid);

        // The whole point of issue #207: the CID must NOT be the empty-set sentinel.
        assertNotEquals(EMPTY_SET_CID, r.contractSetCid,
            "contractSetCid must be content-meaningful, not the empty-set sentinel");

        // .proof file actually got written.
        Path proofPath = tmp.resolve(r.cid + ".proof");
        assertTrue(Files.exists(proofPath), "proof file must exist: " + proofPath);
        assertTrue(Files.size(proofPath) > 0, "proof file must be non-empty");
    }

    @Test
    void mintIsByteDeterministic(@TempDir Path a, @TempDir Path b) throws IOException {
        Orchestrator.MintResult r1 = Orchestrator.mintOneRun(a);
        Orchestrator.MintResult r2 = Orchestrator.mintOneRun(b);

        assertEquals(r1.cid, r2.cid,
            "two mints must produce byte-identical catalog CIDs");
        assertEquals(r1.contractSetCid, r2.contractSetCid,
            "two mints must produce byte-identical contractSetCids");
        assertEquals(r1.contractCount, r2.contractCount);
        assertEquals(r1.slabCount, r2.slabCount);
    }

    @Test
    void everySlabAuthorsAtLeastOneContract() {
        List<AuthoredSlab> slabs = JavaKitInvariants.authorAll();
        assertTrue(slabs.size() >= 5,
            "expected at least 5 slabs; got " + slabs.size());
        for (AuthoredSlab s : slabs) {
            assertTrue(!s.contracts.isEmpty(),
                "slab `" + s.label + "` authored zero contracts");
            assertTrue(s.path.startsWith("implementations/java/"),
                "slab `" + s.label + "` path should be relative to repo root: " + s.path);
        }
    }

    @Test
    void contractNamesAreUniqueAcrossSlabs() {
        List<AuthoredSlab> slabs = JavaKitInvariants.authorAll();
        java.util.Set<String> seen = new java.util.HashSet<>();
        for (AuthoredSlab s : slabs) {
            for (Slab.ContractDecl d : s.contracts) {
                assertTrue(seen.add(d.name),
                    "duplicate contract name across slabs: " + d.name);
            }
        }
    }

    @Test
    void contractNamesAreJavaPrefixed() {
        List<AuthoredSlab> slabs = JavaKitInvariants.authorAll();
        for (AuthoredSlab s : slabs) {
            for (Slab.ContractDecl d : s.contracts) {
                assertTrue(d.name.startsWith("java_"),
                    "kit-internal contract name must be `java_*`-prefixed: " + d.name);
            }
        }
    }
}
