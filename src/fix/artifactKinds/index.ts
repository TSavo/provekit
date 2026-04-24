/**
 * D1a: Artifact kind index.
 *
 * Imports all eight artifact kind descriptors (triggering their self-registration
 * side effects) and exports a registerAll() function for test beforeEach() blocks
 * that need to re-populate the registry after _clearArtifactKindRegistry().
 *
 * Mirrors the shape of src/fix/intakeAdapters/index.ts.
 */

import { registerCodePatch } from "./code_patch.js";
import { registerRegressionTest } from "./regression_test.js";
import { registerPrincipleCandidate } from "./principle_candidate.js";
import { registerCapabilitySpec } from "./capability_spec.js";
import { registerComplementaryChange } from "./complementary_change.js";
import { registerStartupAssert } from "./startup_assert.js";
import { registerDocumentation } from "./documentation.js";
import { registerObservabilityHook } from "./observability_hook.js";

export function registerAll(): void {
  registerCodePatch();
  registerRegressionTest();
  registerPrincipleCandidate();
  registerCapabilitySpec();
  registerComplementaryChange();
  registerStartupAssert();
  registerDocumentation();
  registerObservabilityHook();
}
