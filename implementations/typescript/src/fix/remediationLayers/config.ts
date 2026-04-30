/**
 * B3: "config" remediation layer.
 *
 * Missing or incorrect configuration causes the bug.
 */

import { registerRemediationLayer } from "../remediationLayerRegistry.js";

const descriptor = {
  name: "config",
  description: "Missing or incorrect configuration causes the bug.",
  promptHint:
    `The bug is in a config file, environment variable, feature flag, or deployment setting — not in code. Fix is a config change plus a startup assertion that fails loudly if the config is missing next time. Examples: "missing GOOGLE_API_KEY env var", "Redis URL points at stale instance", "feature flag not enabled for the right cohort".`,
  artifactKinds: ["startup_assert", "documentation"],
};

export function registerConfigLayer(): void {
  registerRemediationLayer(descriptor);
}

// Self-register at module load.
registerConfigLayer();
