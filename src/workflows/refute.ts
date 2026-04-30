/**
 * Refute workflow — registry assembly + manifest loading.
 *
 * Companion to bug-fix.ts. Wires the four refute producers
 * (locate-memento, emit-smt-lib, invoke-z3, mint-verdict-memento) to a
 * ProducerRegistry + ActionRegistry that runManifest can drive.
 *
 * The on-disk manifest is at `src/workflows/refute.workflow.yaml`.
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
  MINT_VERDICT_MEMENTO_CAPABILITY,
  makeMintVerdictMementoAction,
} from "../workflow/producers/mintVerdictMemento.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "refute.workflow.yaml");

export const REFUTE_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const REFUTE_STAGE_CAPABILITIES = [
  LOCATE_MEMENTO_CAPABILITY,
  EMIT_SMT_LIB_CAPABILITY,
  INVOKE_Z3_CAPABILITY,
] as const;

export const REFUTE_ACTION_CAPABILITIES = [
  MINT_VERDICT_MEMENTO_CAPABILITY,
] as const;

export const REFUTE_CAPABILITIES = [
  ...REFUTE_STAGE_CAPABILITIES,
  ...REFUTE_ACTION_CAPABILITIES,
] as const;

export interface RefuteDeps {
  db: Db;
  /** Override for invoke-z3's producer identity (e.g. "z3-symbolic@4.13.4"). */
  z3ProducerVersion?: string;
}

export interface RefuteRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerRefuteRegistries(deps: RefuteDeps): RefuteRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(LOCATE_MEMENTO_CAPABILITY, makeLocateMementoStage({ db: deps.db }));
  registry.register(EMIT_SMT_LIB_CAPABILITY, makeEmitSmtLibStage());
  registry.register(
    INVOKE_Z3_CAPABILITY,
    makeInvokeZ3Stage({
      ...(deps.z3ProducerVersion !== undefined
        ? { producerVersion: deps.z3ProducerVersion }
        : {}),
    }),
  );

  actionRegistry.register(
    MINT_VERDICT_MEMENTO_CAPABILITY,
    makeMintVerdictMementoAction({ db: deps.db }),
  );

  return { registry, actionRegistry };
}

export function loadRefuteManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

import type { IrFormula } from "../ir/formulas.js";

/**
 * Workflow input shape that runManifest expects, encoded for $input refs.
 */
export interface RefuteWorkflowInput {
  /** The propertyHash to refute. */
  propertyHash: string;
  /** Kit-supplied axioms. Defaults to []. */
  axioms?: IrFormula[];
  /** SMT-LIB logic. Defaults to "ALL". */
  logic?: string;
  /** Z3 timeout in milliseconds. Defaults to 30_000. */
  timeoutMs?: number;
  /** Producer identity to record on the verdict memento. */
  producedBy?: string;
}
