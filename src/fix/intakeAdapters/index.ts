/**
 * B1: Intake adapter index.
 *
 * Imports all four v1 adapters (triggering their self-registration side
 * effects) and exports a registerAll() function for test beforeEach() blocks
 * that need to re-populate the registry after _clearIntakeRegistry().
 *
 * Mirrors the shape of src/sast/schema/capabilities/index.ts.
 */

import { registerReportIntakeAdapter } from "./report.js";
import { registerGapReportIntakeAdapter } from "./gapReport.js";
import { registerTestFailureIntakeAdapter } from "./testFailure.js";
import { registerRuntimeLogIntakeAdapter } from "./runtimeLog.js";

export function registerAll(): void {
  registerReportIntakeAdapter();
  registerGapReportIntakeAdapter();
  registerTestFailureIntakeAdapter();
  registerRuntimeLogIntakeAdapter();
}
