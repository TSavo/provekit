/**
 * B3: "infrastructure" remediation layer.
 *
 * Bug caused by infrastructure behavior (network, DB, queue, etc.) rather
 * than code logic.
 */

import { registerRemediationLayer } from "../remediationLayerRegistry.js";

const descriptor = {
  name: "infrastructure",
  description:
    "Bug caused by infrastructure behavior (network, DB, queue, etc.) rather than code logic.",
  promptHint:
    `The bug is an infrastructure issue — database connection drops, network partition, queue backup, disk full. Fix is typically an error handler + observability hook + documentation. Examples: "intermittent 500s when DB primary fails over", "events dropped when Kafka partition is rebalancing".`,
  artifactKinds: ["error_handler", "observability_hook", "documentation"],
};

export function registerInfrastructureLayer(): void {
  registerRemediationLayer(descriptor);
}

// Self-register at module load.
registerInfrastructureLayer();
