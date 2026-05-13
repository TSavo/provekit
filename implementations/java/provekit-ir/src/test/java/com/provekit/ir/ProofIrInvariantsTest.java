// SPDX-License-Identifier: Apache-2.0

package com.provekit.ir;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

public class ProofIrInvariantsTest {
    @Test
    public void javaNullBoundaryInvariantIsAuthoredInProofIr() {
        Declaration.Contract contract =
            ProofIrInvariants.javaNullBoundaryRealizerContractVarIsBoundParameter();

        assertEquals(
            ProofIrInvariants.JAVA_NULL_BOUNDARY_REALIZER_CONTRACT_VAR_IS_BOUND_PARAMETER,
            contract.name());
        assertTrue(contract.toJson().contains("\"kind\":\"implies\""));
        assertTrue(contract.toJson().contains("\"name\":\"closedNullBoundaryRealizerPlan\""));
        assertTrue(contract.toJson().contains("\"name\":\"methodHasParameter\""));
    }
}
