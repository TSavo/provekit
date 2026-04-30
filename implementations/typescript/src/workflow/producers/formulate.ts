/**
 * Formulate stage — bug-fix workflow's invariant-formulation capability.
 *
 * Wraps formulateInvariant() in a Stage<I, O>. Same factory pattern
 * as intake: dependencies (db, llm, optional logger + fidelity
 * verifiers) bind at construction; per-call inputs (signal, locus,
 * recognized, investigateReport) are content-hashable.
 *
 * Effective purity: the function calls Z3 (oracle #1) and reads
 * principle-match rows from the DB. Same DB state + same inputs
 * = same output. When the principle library evolves, the cache
 * silently goes stale until we hash the principle library state
 * into the binding hash; that's a known follow-up, not a v1 bug.
 *
 * Output shape: InvariantClaim. Fully JSON-round-trippable
 * (all fields are primitives / arrays / nested primitives).
 */

import { execFileSync } from "child_process";
import { dirname } from "path";
import { formulateInvariant } from "../../fix/stages/formulateInvariant.js";
import type {
  BugLocus,
  IntentSignal,
  InvariantClaim,
  LLMProvider,
} from "../../fix/types.js";
import type { FixLoopLogger } from "../../fix/logger.js";
import type { FidelityVerifiers } from "../../fix/invariantFidelity.js";
import type { RecognizeResult } from "../../fix/stages/recognize.js";
import type { InvestigateReport } from "../../fix/stages/investigate.js";
import type { Db } from "../../db/index.js";
import type { Stage } from "../types.js";

export const FORMULATE_CAPABILITY = "formulate";

export interface FormulateStageInput {
  signal: IntentSignal;
  locus: BugLocus;
  recognized?: RecognizeResult;
  investigateReport?: InvestigateReport;
}

export interface MakeFormulateStageDeps {
  db: Db;
  llm: LLMProvider;
  logger?: FixLoopLogger;
  /** Test-only injection point. Production callers should leave undefined. */
  fidelityVerifiers?: FidelityVerifiers;
  /** Override producer identity. Default: "formulate@v1". */
  producerVersion?: string;
}

export function makeFormulateStage(
  deps: MakeFormulateStageDeps,
): Stage<FormulateStageInput, InvariantClaim> {
  const producedBy = deps.producerVersion ?? "formulate@v1";

  return {
    name: "formulate",
    producedBy,

    serializeInput(input) {
      return {
        signal: input.signal,
        locus: input.locus,
        recognized: input.recognized ?? null,
        investigateReport: input.investigateReport ?? null,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as InvariantClaim;
    },

    async run(input) {
      return formulateInvariant({
        signal: input.signal,
        locus: input.locus,
        db: deps.db,
        llm: deps.llm,
        projectRoot: resolveProjectRoot(input.locus.file) ?? undefined,
        logger: deps.logger,
        recognized: input.recognized,
        investigateReport: input.investigateReport,
        _fidelityVerifiers: deps.fidelityVerifiers,
      });
    },
  };
}

/**
 * Locus file → git toplevel. Returns null when the file isn't inside
 * a git repo or git isn't installed; formulateInvariant treats null
 * as "skip the persistence step."
 *
 * Inlined to keep the workflow producer module self-contained. The
 * twin in orchestrator.ts disappears when runFixLoop is replaced
 * with manifest dispatch.
 */
function resolveProjectRoot(locusFile: string): string | null {
  try {
    const root = execFileSync(
      "git",
      ["rev-parse", "--show-toplevel"],
      {
        cwd: dirname(locusFile),
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      },
    ).trim();
    return root || null;
  } catch {
    return null;
  }
}
