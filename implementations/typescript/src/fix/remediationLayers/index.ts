/**
 * B3: Remediation layer index.
 *
 * Imports all eight v1 layers (triggering their self-registration side
 * effects) and exports a registerAll() function for test beforeEach() blocks
 * that need to re-populate the registry after _clearRemediationLayerRegistry().
 *
 * Mirrors the shape of src/fix/intakeAdapters/index.ts.
 */

import { registerCodeInvariantLayer } from "./code_invariant.js";
import { registerConfigLayer } from "./config.js";
import { registerInfrastructureLayer } from "./infrastructure.js";
import { registerDataIngressLayer } from "./data_ingress.js";
import { registerTimingLayer } from "./timing.js";
import { registerSpecDriftLayer } from "./spec_drift.js";
import { registerObservabilityLayer } from "./observability.js";
import { registerOutOfScopeLayer } from "./out_of_scope.js";

export function registerAll(): void {
  registerCodeInvariantLayer();
  registerConfigLayer();
  registerInfrastructureLayer();
  registerDataIngressLayer();
  registerTimingLayer();
  registerSpecDriftLayer();
  registerObservabilityLayer();
  registerOutOfScopeLayer();
}
