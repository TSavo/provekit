/**
 * prove-with-lean workflow — registry assembly + manifest loading.
 *
 * Companion to refute.ts (the Z3 leg of cross-paradigm composition). Wires
 * the prove-with-lean producers (locate-memento, emit-lean, provideLeanProof,
 * mint-lean-verdict-memento) onto a ProducerRegistry + ActionRegistry that
 * the workflow runner can drive.
 *
 * The on-disk manifest is at `src/workflows/prove-with-lean.workflow.yaml`.
 */

import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import type { Db } from "../db/index.js";
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
  LOCATE_MEMENTO_CAPABILITY,
  makeLocateMementoStage,
} from "../workflow/producers/locateMemento.js";
import {
  EMIT_LEAN_CAPABILITY,
  makeEmitLeanStage,
} from "../workflow/producers/emitLean.js";
import {
  PROVIDE_LEAN_PROOF_CAPABILITY,
  makeProvideLeanProofAction,
} from "../workflow/producers/provideLeanProof.js";
import {
  MINT_LEAN_VERDICT_MEMENTO_CAPABILITY,
  makeMintLeanVerdictMementoAction,
} from "../workflow/producers/mintLeanVerdictMemento.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "prove-with-lean.workflow.yaml");

export const PROVE_WITH_LEAN_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const PROVE_WITH_LEAN_STAGE_CAPABILITIES = [
  LOCATE_MEMENTO_CAPABILITY,
  EMIT_LEAN_CAPABILITY,
] as const;

export const PROVE_WITH_LEAN_ACTION_CAPABILITIES = [
  PROVIDE_LEAN_PROOF_CAPABILITY,
  MINT_LEAN_VERDICT_MEMENTO_CAPABILITY,
] as const;

export const PROVE_WITH_LEAN_CAPABILITIES = [
  ...PROVE_WITH_LEAN_STAGE_CAPABILITIES,
  ...PROVE_WITH_LEAN_ACTION_CAPABILITIES,
] as const;

export interface ProveWithLeanDeps {
  db: Db;
  /** Override for provideLeanProof's producer identity (e.g. "lean@4.6.0"). */
  leanProducerVersion?: string;
}

export interface ProveWithLeanRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerProveWithLeanRegistries(
  deps: ProveWithLeanDeps,
): ProveWithLeanRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(LOCATE_MEMENTO_CAPABILITY, makeLocateMementoStage({ db: deps.db }));
  registry.register(EMIT_LEAN_CAPABILITY, makeEmitLeanStage());

  actionRegistry.register(
    PROVIDE_LEAN_PROOF_CAPABILITY,
    makeProvideLeanProofAction(
      deps.leanProducerVersion !== undefined
        ? { producerVersion: deps.leanProducerVersion }
        : {},
    ),
  );
  actionRegistry.register(
    MINT_LEAN_VERDICT_MEMENTO_CAPABILITY,
    makeMintLeanVerdictMementoAction({ db: deps.db }),
  );

  return { registry, actionRegistry };
}

export function loadProveWithLeanManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

import type { IrFormula } from "../ir/formulas.js";

/**
 * Workflow input shape that runManifest expects, encoded for $input refs.
 */
export interface ProveWithLeanWorkflowInput {
  /** The propertyHash to prove. */
  propertyHash: string;
  /** User-supplied Lean proof body (replaces `sorry`). */
  proofText: string;
  /** Kit-supplied axioms. Defaults to []. */
  axioms?: IrFormula[];
  /** Lean timeout in milliseconds. Defaults to 60_000. */
  timeoutMs?: number;
  /** Producer identity to record on the verdict memento. */
  producedBy?: string;
}
