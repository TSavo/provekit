/**
 * B3: "timing" remediation layer.
 *
 * Bug caused by ordering, concurrency, or race conditions.
 */

import { registerRemediationLayer } from "../remediationLayerRegistry.js";

const descriptor = {
  name: "timing",
  description: "Bug caused by ordering, concurrency, or race conditions.",
  promptHint:
    `The bug only manifests under certain timing — a race, a retry loop, a stale cache, a missing lock. Fix typically requires both a code patch and an observability hook to detect the condition again. Examples: "two requests update the same row and last-write-wins silently", "stale cache returns outdated user state after login".`,
  artifactKinds: ["code_patch", "observability_hook", "regression_test"],
};

export function registerTimingLayer(): void {
  registerRemediationLayer(descriptor);
}

// Self-register at module load.
registerTimingLayer();
