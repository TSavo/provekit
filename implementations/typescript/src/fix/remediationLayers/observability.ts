/**
 * B3: "observability" remediation layer.
 *
 * The bug exists but the team lacks visibility to detect or diagnose it.
 */

import { registerRemediationLayer } from "../remediationLayerRegistry.js";

const descriptor = {
  name: "observability",
  description:
    "The bug exists but the team lacks visibility to detect or diagnose it.",
  promptHint:
    `The primary fix is to add logging, metrics, or tracing so the next occurrence is visible. Sometimes paired with a code_invariant fix; sometimes observability is the whole remediation. Example: "we know something broke because customer reported it, but we have no logs to find when/why".`,
  artifactKinds: ["observability_hook", "documentation"],
};

export function registerObservabilityLayer(): void {
  registerRemediationLayer(descriptor);
}

// Self-register at module load.
registerObservabilityLayer();
