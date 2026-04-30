/**
 * B3: "out_of_scope" remediation layer.
 *
 * Sentinel: the bug is not something this system can remediate — route to human.
 */

import { registerRemediationLayer } from "../remediationLayerRegistry.js";

const descriptor = {
  name: "out_of_scope",
  description:
    "The bug is not something this system can remediate — route to human.",
  promptHint:
    `Escape hatch. Use when the bug is a policy question, a UX issue, an external vendor problem, or genuinely ambiguous. Better to bail loudly than to produce a confident wrong plan.`,
  artifactKinds: ["documentation"],
};

export function registerOutOfScopeLayer(): void {
  registerRemediationLayer(descriptor);
}

// Self-register at module load.
registerOutOfScopeLayer();
