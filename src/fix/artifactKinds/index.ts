/**
 * D1a/D1b: Artifact kind index.
 *
 * Imports all fourteen artifact kind descriptors (triggering their self-registration
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
import { registerTestFix } from "./test_fix.js";
import { registerConfigUpdate } from "./config_update.js";
import { registerDependencyUpdate } from "./dependency_update.js";
import { registerPromptUpdate } from "./prompt_update.js";
import { registerLintRule } from "./lint_rule.js";
import { registerMigrationFix } from "./migration_fix.js";

export function registerAll(): void {
  registerCodePatch();
  registerRegressionTest();
  registerPrincipleCandidate();
  registerCapabilitySpec();
  registerComplementaryChange();
  registerStartupAssert();
  registerDocumentation();
  registerObservabilityHook();
  registerTestFix();
  registerConfigUpdate();
  registerDependencyUpdate();
  registerPromptUpdate();
  registerLintRule();
  registerMigrationFix();
}
