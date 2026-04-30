/**
 * B3: "spec_drift" remediation layer.
 *
 * The code is correct per its implementation, but the specification or
 * contract has changed.
 */

import { registerRemediationLayer } from "../remediationLayerRegistry.js";

const descriptor = {
  name: "spec_drift",
  description:
    "The code is correct per its implementation, but the specification or contract has changed.",
  promptHint:
    `The bug is a mismatch between what the code does and what some contract (API, protocol, schema) now requires. Fix is updating the code to match the new contract plus updating documentation. Examples: "vendor changed their API response shape in v3", "internal protocol version bumped but client wasn't updated".`,
  artifactKinds: ["code_patch", "documentation"],
};

export function registerSpecDriftLayer(): void {
  registerRemediationLayer(descriptor);
}

// Self-register at module load.
registerSpecDriftLayer();
