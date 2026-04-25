/**
 * Corpus loader: enumerates and loads all registered scenarios.
 *
 * Each scenario is a named export `scenario: CorpusScenario` in a file under
 * src/fix/corpus/scenarios/<bug-class>/<id>.ts.
 *
 * To add a scenario: create the file and import it here. The loader is explicit
 * (no dynamic require glob) so tsc can validate every scenario at build time.
 */

import type { CorpusScenario } from "./scenarios.js";

import { scenario as dbz001 } from "./scenarios/division-by-zero/dbz-001.js";
import { scenario as dbz002 } from "./scenarios/division-by-zero/dbz-002.js";
import { scenario as dbz003 } from "./scenarios/division-by-zero/dbz-003.js";
import { scenario as null001 } from "./scenarios/null-assertion/null-001.js";
import { scenario as null002 } from "./scenarios/null-assertion/null-002.js";
import { scenario as ternary001 } from "./scenarios/ternary-branch-collapse/ternary-001.js";
import { scenario as ternary002 } from "./scenarios/ternary-branch-collapse/ternary-002.js";
import { scenario as advMissingFile } from "./scenarios/adversarial/adv-missing-file.js";
import { scenario as advOutOfScope } from "./scenarios/adversarial/adv-out-of-scope.js";
import { scenario as advUnsatInvariant } from "./scenarios/adversarial/adv-unsatisfiable-invariant.js";
import { scenario as advFixFailsOracle2 } from "./scenarios/adversarial/adv-fix-fails-oracle2.js";
import { scenario as multi001 } from "./scenarios/multi-file/multi-001.js";

/** All registered corpus scenarios, in declaration order. */
export const ALL_SCENARIOS: CorpusScenario[] = [
  dbz001,
  dbz002,
  dbz003,
  null001,
  null002,
  ternary001,
  ternary002,
  advMissingFile,
  advOutOfScope,
  advUnsatInvariant,
  advFixFailsOracle2,
  multi001,
];

/**
 * Load scenarios filtered by bug class.
 * Pass "all" (or omit) to get every scenario.
 */
export function loadScenarios(bugClass?: string): CorpusScenario[] {
  if (!bugClass || bugClass === "all") return ALL_SCENARIOS;
  return ALL_SCENARIOS.filter((s) => s.bugClass === bugClass);
}

/** Look up a single scenario by ID. */
export function getScenario(id: string): CorpusScenario | undefined {
  return ALL_SCENARIOS.find((s) => s.id === id);
}
