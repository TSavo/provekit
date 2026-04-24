/**
 * B3: "code_invariant" remediation layer.
 *
 * Logic error expressible as a Z3-provable invariant violation at a specific
 * code site. The only layer that can trigger substrate extension (C6).
 */

import { registerRemediationLayer } from "../remediationLayerRegistry.js";

const descriptor = {
  name: "code_invariant",
  description:
    "Logic error expressible as a Z3-provable invariant violation at a specific code site.",
  promptHint:
    `The bug is in the code itself — something like a division by zero, off-by-one, unguarded null access, wrong operator. The fix is a code patch plus a regression test. If this represents a NEW class of bug (not covered by existing principles), the loop will also propose a principle candidate and potentially a substrate capability extension. Examples: "function crashes on empty array", "timeout when input has NaN", "assertion fails when config is missing optional field".`,
  artifactKinds: ["code_patch", "regression_test", "principle_candidate"],
  canTriggerSubstrateExtension: true,
};

export function registerCodeInvariantLayer(): void {
  registerRemediationLayer(descriptor);
}

// Self-register at module load.
registerCodeInvariantLayer();
