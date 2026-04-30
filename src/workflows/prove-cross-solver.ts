/**
 * prove-cross-solver workflow — registry assembly + manifest loading.
 *
 * Wires the cross-solver producers (locate-memento, emit-smt-lib,
 * invoke-z3, emit-cvc5-smt-lib, invoke-cvc5, compare-verdicts,
 * mint-cross-solver-memento) onto a ProducerRegistry + ActionRegistry
 * the workflow runner can drive.
 *
 * The on-disk manifest is at `src/workflows/prove-cross-solver.workflow.yaml`.
 *
 * This is the operational test of the architectural claim that
 * propertyHash CIDs are solver-agnostic: two solver-flavored translators
 * take the SAME IR, produce DISTINCT producer-identity mementos, both
 * reference the SAME IR sourceCid, and the cross-solver memento composes
 * over both.
 *
 * Spec: docs/specs/2026-04-29-the-semantic-envelope.md
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
  EMIT_SMT_LIB_CAPABILITY,
  makeEmitSmtLibStage,
} from "../workflow/producers/emitSmtLib.js";
import {
  INVOKE_Z3_CAPABILITY,
  makeInvokeZ3Stage,
} from "../workflow/producers/invokeZ3.js";
import {
  EMIT_CVC5_SMT_LIB_CAPABILITY,
  makeEmitCvc5SmtLibStage,
} from "../workflow/producers/emitCvc5SmtLib.js";
import {
  INVOKE_CVC5_CAPABILITY,
  makeInvokeCvc5Stage,
} from "../workflow/producers/invokeCvc5.js";
import {
  COMPARE_VERDICTS_CAPABILITY,
  makeCompareVerdictsStage,
} from "../workflow/producers/compareVerdicts.js";
import {
  MINT_CROSS_SOLVER_MEMENTO_CAPABILITY,
  makeMintCrossSolverMementoAction,
} from "../workflow/producers/mintCrossSolverMemento.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "prove-cross-solver.workflow.yaml");

export const PROVE_CROSS_SOLVER_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const PROVE_CROSS_SOLVER_STAGE_CAPABILITIES = [
  LOCATE_MEMENTO_CAPABILITY,
  EMIT_SMT_LIB_CAPABILITY,
  INVOKE_Z3_CAPABILITY,
  EMIT_CVC5_SMT_LIB_CAPABILITY,
  INVOKE_CVC5_CAPABILITY,
  COMPARE_VERDICTS_CAPABILITY,
] as const;

export const PROVE_CROSS_SOLVER_ACTION_CAPABILITIES = [
  MINT_CROSS_SOLVER_MEMENTO_CAPABILITY,
] as const;

export const PROVE_CROSS_SOLVER_CAPABILITIES = [
  ...PROVE_CROSS_SOLVER_STAGE_CAPABILITIES,
  ...PROVE_CROSS_SOLVER_ACTION_CAPABILITIES,
] as const;

export interface ProveCrossSolverDeps {
  db: Db;
  /** Override z3 producer identity. */
  z3ProducerVersion?: string;
  /** Override cvc5 producer identity. */
  cvc5ProducerVersion?: string;
}

export interface ProveCrossSolverRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerProveCrossSolverRegistries(
  deps: ProveCrossSolverDeps,
): ProveCrossSolverRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(LOCATE_MEMENTO_CAPABILITY, makeLocateMementoStage({ db: deps.db }));
  registry.register(EMIT_SMT_LIB_CAPABILITY, makeEmitSmtLibStage());
  registry.register(
    INVOKE_Z3_CAPABILITY,
    makeInvokeZ3Stage(
      deps.z3ProducerVersion !== undefined
        ? { producerVersion: deps.z3ProducerVersion }
        : {},
    ),
  );
  registry.register(EMIT_CVC5_SMT_LIB_CAPABILITY, makeEmitCvc5SmtLibStage());
  registry.register(
    INVOKE_CVC5_CAPABILITY,
    makeInvokeCvc5Stage(
      deps.cvc5ProducerVersion !== undefined
        ? { producerVersion: deps.cvc5ProducerVersion }
        : {},
    ),
  );
  registry.register(COMPARE_VERDICTS_CAPABILITY, makeCompareVerdictsStage());

  actionRegistry.register(
    MINT_CROSS_SOLVER_MEMENTO_CAPABILITY,
    makeMintCrossSolverMementoAction({ db: deps.db }),
  );

  return { registry, actionRegistry };
}

export function loadProveCrossSolverManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

import type { IrFormula } from "../ir/formulas.js";

/**
 * Workflow input shape that runManifest expects, encoded for $input refs.
 */
export interface ProveCrossSolverWorkflowInput {
  /** The propertyHash to verify. */
  propertyHash: string;
  /** Kit-supplied axioms. Defaults to []. */
  axioms?: IrFormula[];
  /** SMT-LIB logic. Defaults to "ALL". */
  logic?: string;
  /** Solver timeout in milliseconds (applied to both z3 and cvc5). Defaults to 30_000. */
  timeoutMs?: number;
  /** Producer identity to record on the cross-solver memento. */
  producedBy?: string;
}
