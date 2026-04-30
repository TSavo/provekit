/**
 * Weaken workflow — registry assembly + manifest loading.
 *
 * Relaxes an existing invariant. Reuses the formulate-via-lifter Stage
 * to lift the new surface text to IR, the new compare-formulas Stage
 * to diff old vs new in weaken mode, and the write-invariant-file
 * Action to overwrite the on-disk file.
 *
 * Dual to strengthen.ts; the only difference is mode=weaken in the
 * compare-formulas node. The future SMT-backed implication check
 * plugs into the same Stage slot.
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

const DEFAULT_MANIFEST_PATH = join(__dirname, "weaken.workflow.yaml");

export const WEAKEN_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const WEAKEN_STAGE_CAPABILITIES = [
  "formulate-via-lifter",
  "compare-formulas",
] as const;
export const WEAKEN_ACTION_CAPABILITIES = ["write-invariant-file"] as const;
export const WEAKEN_CAPABILITIES = [
  ...WEAKEN_STAGE_CAPABILITIES,
  ...WEAKEN_ACTION_CAPABILITIES,
] as const;

export interface WeakenDeps {
  llm: LLMProvider;
}

export interface WeakenRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

/** Construct stage + action registries for the weaken workflow. */
export function registerWeakenRegistries(deps: WeakenDeps): WeakenRegistries {
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

/** Read and parse the weaken manifest from disk. */
export function loadWeakenManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/** Workflow input shape that runManifest expects. */
export interface WeakenWorkflowInput {
  /**
   * Natural-language description of the relaxation. Threaded into
   * the lifter prompt as the universal-context intent.
   */
  intent: IntentSignal;
  /**
   * Existential-intent tests passed to the lifter. The new invariant
   * must pass all of them.
   */
  tests?: { source: string; testNames: string[] }[];
  /** Optional diff describing the prospective change. */
  diff?: string;
  /** Optional bug locus; threads into the lifter's target-file rendering. */
  locus?: BugLocus;
  /** The prior IR formula being relaxed. The diff target. */
  oldFormula: IrFormula;
  /** Absolute path to the `.invariant.ts` file to overwrite. */
  filePath: string;
}
