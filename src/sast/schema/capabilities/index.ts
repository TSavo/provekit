export * from "./arithmetic.js";
export * from "./assigns.js";
export * from "./returns.js";
export * from "./memberAccess.js";
export * from "./nonNullAssertion.js";
export * from "./truthiness.js";
export * from "./narrows.js";
export * from "./decides.js";
export * from "./iterates.js";
export * from "./yields.js";
export * from "./throws.js";
export * from "./calls.js";
export * from "./captures.js";
export * from "./pattern.js";
export * from "./binding.js";
export * from "./signal.js";

import { registerArithmetic } from "./arithmetic.js";
import { registerAssigns } from "./assigns.js";
import { registerReturns } from "./returns.js";
import { registerMemberAccess } from "./memberAccess.js";
import { registerNonNullAssertion } from "./nonNullAssertion.js";
import { registerTruthiness } from "./truthiness.js";
import { registerNarrows } from "./narrows.js";
import { registerDecides } from "./decides.js";
import { registerIterates } from "./iterates.js";
import { registerYields } from "./yields.js";
import { registerThrows } from "./throws.js";
import { registerCalls } from "./calls.js";
import { registerCaptures } from "./captures.js";
import { registerPattern } from "./pattern.js";
import { registerBinding } from "./binding.js";
import { registerSignal, registerSignalInterpolations } from "./signal.js";

/**
 * Re-register all 17 capabilities. Safe to call after _clearRegistry() in tests.
 * Individual files also call their own register function at module load time,
 * so this is only needed to re-populate after a test clear.
 */
export function registerAll(): void {
  registerArithmetic();
  registerAssigns();
  registerReturns();
  registerMemberAccess();
  registerNonNullAssertion();
  registerTruthiness();
  registerNarrows();
  registerDecides();
  registerIterates();
  registerYields();
  registerThrows();
  registerCalls();
  registerCaptures();
  registerPattern();
  registerBinding();
  registerSignal();
  registerSignalInterpolations();
}
