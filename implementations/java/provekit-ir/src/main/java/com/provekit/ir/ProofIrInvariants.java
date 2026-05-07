// SPDX-License-Identifier: Apache-2.0

package com.provekit.ir;

import java.util.List;

/**
 * Java ProofIR-authored invariant catalog.
 *
 * These declarations are authored with the Java IR kit itself
 * ({@link Formula}, {@link Term}, {@link Declaration}) and may be packaged by
 * kit-specific self-contract mints. Packaging is downstream; this file is the
 * Java ProofIR source of truth.
 */
public final class ProofIrInvariants {
    public static final String JAVA_NULL_BOUNDARY_REALIZER_CONTRACT_VAR_IS_BOUND_PARAMETER =
        "java_null_boundary_realizer_contract_var_is_bound_parameter";

    private ProofIrInvariants() {}

    public static List<Declaration.Contract> javaRealizerContracts() {
        return List.of(javaNullBoundaryRealizerContractVarIsBoundParameter());
    }

    public static Declaration.Contract javaNullBoundaryRealizerContractVarIsBoundParameter() {
        Term plan = Term.var_("plan", Sort.String);
        Formula post = Formula.forall(
            "plan",
            Sort.String,
            Formula.implies(
                Formula.atomic("closedNullBoundaryRealizerPlan", plan),
                Formula.atomic(
                    "methodHasParameter",
                    Term.ctor("targetMethod", new Term[] { plan }, Sort.String),
                    Term.ctor("proofVar", new Term[] { plan }, Sort.String))));

        return new Declaration.Contract(
            JAVA_NULL_BOUNDARY_REALIZER_CONTRACT_VAR_IS_BOUND_PARAMETER,
            "out",
            null,
            post,
            null,
            null);
    }
}
