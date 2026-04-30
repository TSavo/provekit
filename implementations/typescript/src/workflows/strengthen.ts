/**
 * Strengthen workflow — registry assembly + manifest loading.
 *
 * Tightens an existing invariant. Reuses the formulate-via-lifter
 * Stage to lift the new surface text to IR, the compare-formulas
 * Stage to diff old vs new in strengthen mode, and the
 * write-invariant-file Action to overwrite the on-disk file.
 *
 * Dual to weaken.ts; the only difference is mode=strengthen in the
 * compare-formulas node. The future SMT-backed implication check
 * (new is a superset of old) plugs into the same Stage slot.
 *
 * Strengthen's failure mode is real: each callsite that previously
 * satisfied the OLD contract may NOT satisfy the NEW contract. The
 * delta names the added conjuncts; the caller takes that list and
 * re-verifies each callsite. The framework's discipline is to surface
 * the punch list precisely, not to traverse it.
 */

import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import type { LLMProvider } from "../fix/types.js";
import {
  InMemoryActionRegistry,
  InMemoryRegistry,
  type ActionRegistry,
  type ProducerRegistry,
} from "../workflow/registry.js";
import {
  parseManifest,
  type WorkflowManifest,
} from "../workflow/manifest.js";
import {
  FORMULATE_VIA_LIFTER_CAPABILITY,
  makeFormulateViaLifterStage,
} from "../workflow/producers/formulateViaLifter.js";
import {
  COMPARE_FORMULAS_CAPABILITY,
  makeCompareFormulasStage,
} from "../workflow/producers/compareFormulas.js";
import {
  WRITE_INVARIANT_FILE_CAPABILITY,
  makeWriteInvariantFileAction,
} from "../workflow/producers/writeInvariantFile.js";
import type { IntentSignal, BugLocus } from "../fix/types.js";
import type { IrFormula } from "../ir/formulas.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "strengthen.workflow.yaml");

export const STRENGTHEN_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const STRENGTHEN_STAGE_CAPABILITIES = [
  "formulate-via-lifter",
  "compare-formulas",
] as const;
export const STRENGTHEN_ACTION_CAPABILITIES = ["write-invariant-file"] as const;
export const STRENGTHEN_CAPABILITIES = [
  ...STRENGTHEN_STAGE_CAPABILITIES,
  ...STRENGTHEN_ACTION_CAPABILITIES,
] as const;

export interface StrengthenDeps {
  llm: LLMProvider;
}

export interface StrengthenRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

/** Construct stage + action registries for the strengthen workflow. */
export function registerStrengthenRegistries(
  deps: StrengthenDeps,
): StrengthenRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(
    FORMULATE_VIA_LIFTER_CAPABILITY,
    makeFormulateViaLifterStage({ llm: deps.llm }),
  );
  registry.register(COMPARE_FORMULAS_CAPABILITY, makeCompareFormulasStage());

  actionRegistry.register(
    WRITE_INVARIANT_FILE_CAPABILITY,
    makeWriteInvariantFileAction(),
  );

  return { registry, actionRegistry };
}

/** Read and parse the strengthen manifest from disk. */
export function loadStrengthenManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/** Workflow input shape that runManifest expects. */
export interface StrengthenWorkflowInput {
  /** Natural-language description of the strengthening. */
  intent: IntentSignal;
  /** Existential-intent tests passed to the lifter. */
  tests?: { source: string; testNames: string[] }[];
  /** Optional diff describing the prospective change. */
  diff?: string;
  /** Optional bug locus; threads into the lifter's target-file rendering. */
  locus?: BugLocus;
  /** The prior IR formula being tightened. The diff target. */
  oldFormula: IrFormula;
  /** Absolute path to the `.invariant.ts` file to overwrite. */
  filePath: string;
}
