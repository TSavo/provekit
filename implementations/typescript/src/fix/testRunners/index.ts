/**
 * Test runner registry index.
 *
 * Imports all five adapters (triggering their self-registration side effects)
 * and exports a registerAll() function for test beforeEach() blocks that need
 * to re-populate the registry after _clearTestRunnerRegistry().
 *
 * Mirrors the shape of src/fix/remediationLayers/index.ts and
 * src/fix/intakeAdapters/index.ts.
 */

import { registerVitest } from "./vitest.js";
import { registerJest } from "./jest.js";
import { registerMocha } from "./mocha.js";
import { registerNodeTest } from "./nodetest.js";
import { registerNone } from "./none.js";

export function registerAll(): void {
  registerVitest();
  registerJest();
  registerMocha();
  registerNodeTest();
  registerNone();
}

export {
  registerTestRunner,
  getTestRunner,
  listTestRunners,
  detectTestRunner,
  _clearTestRunnerRegistry,
} from "./registry.js";

export type { TestRunnerDescriptor } from "./registry.js";
