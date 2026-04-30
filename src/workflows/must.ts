/**
 * Must workflow — registry assembly + manifest loading.
 *
 * `provekit must <file> "<intent>"` adds an invariant to <file>'s
 * .invariant.ts companion based on natural-language intent.
 *
 * Reuses three Stages from the bug-fix workflow's producer set:
 *   - intake (IntentSignal from natural language)
 *   - locate (target file's symbols)
 *   - formulate-via-lifter (LLM emits invariant; lifted to IrFormula)
 *
 * Adds one Action:
 *   - write-invariant-file (writes surface text to .invariant.ts)
 *
 * The user's production code is NOT modified. Only the .invariant.ts
 * companion file is written. This is invariant authorship, not code
 * change.
 */

import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import type { Db } from "../db/index.js";
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

import { INTAKE_CAPABILITY, makeIntakeStage } from "../workflow/producers/intake.js";
import { LOCATE_CAPABILITY, makeLocateStage } from "../workflow/producers/locate.js";
import {
  FORMULATE_VIA_LIFTER_CAPABILITY,
  makeFormulateViaLifterStage,
} from "../workflow/producers/formulateViaLifter.js";
import {
  WRITE_INVARIANT_FILE_CAPABILITY,
  makeWriteInvariantFileAction,
} from "../workflow/producers/writeInvariantFile.js";

const __dirname = dirname(fileURLToPath(import.meta.url));

export interface MustWorkflowDeps {
  /** LLM provider for intake + formulate. */
  llm: LLMProvider;
  /** Database handle for the locate stage's SAST queries. */
  db: Db;
}

export interface MustRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

/**
 * Construct the stage ProducerRegistry and the ActionRegistry for the
 * must workflow. Pass both to `runManifest` to execute the YAML.
 */
export function registerMustRegistries(deps: MustWorkflowDeps): MustRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(INTAKE_CAPABILITY, makeIntakeStage(deps.llm));
  registry.register(LOCATE_CAPABILITY, makeLocateStage({ db: deps.db }));
  registry.register(
    FORMULATE_VIA_LIFTER_CAPABILITY,
    makeFormulateViaLifterStage({ llm: deps.llm }),
  );

  actionRegistry.register(
    WRITE_INVARIANT_FILE_CAPABILITY,
    makeWriteInvariantFileAction(),
  );

  return { registry, actionRegistry };
}

/**
 * Load the on-disk must manifest YAML.
 */
export function loadMustManifest(): WorkflowManifest {
  const yamlPath = join(__dirname, "must.workflow.yaml");
  const text = readFileSync(yamlPath, "utf-8");
  return parseManifest(text);
}
