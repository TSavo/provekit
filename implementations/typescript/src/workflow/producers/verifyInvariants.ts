/**
 * VerifyInvariants Stage — Z3-driven verification of every standing
 * invariant in `.provekit/invariants/`.
 *
 * Wraps the existing verifyAllCached() / verifyAll() runtime entrypoints
 * (the `provekit invariants verify` gate). The Stage is the canonical
 * data-driven shape; the imperative entrypoint at runInvariants("verify")
 * stays in cli.ts until the meta-dispatcher routes through this manifest.
 *
 * Cache discipline: this Stage runs Z3, which is deterministic given the
 * substrate identity + invariant set. The runtime layer ALREADY does its
 * own per-invariant cache (verifyCache.ts); the workflow Stage's cache
 * sits one level above and is keyed on the high-level inputs (projectRoot,
 * timeout, maxPaths, adversarial). Two layers of caching is intentional:
 * the workflow cache lets the Stage be skipped wholesale if nothing
 * changed; the runtime cache reuses verdicts when only some invariants
 * changed.
 *
 * Spec: protocol/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 */

import type { Stage } from "../types.js";

export const VERIFY_INVARIANTS_CAPABILITY = "verify-invariants";

export interface VerifyInvariantsInput {
  projectRoot: string;
  timeoutMs?: number;
  maxPaths?: number;
  adversarial: boolean;
}

export interface VerifyInvariantVerdict {
  invariantId: string;
  scope: "callsite" | "sink";
  status: "holds" | "decayed" | "violated";
  pathCheck: "skipped" | "holds" | "violated" | "undecidable";
  cacheStatus: "hit" | "miss";
}

export interface VerifyInvariantsSummary {
  total: number;
  holds: number;
  decayed: number;
  violated: number;
  cacheHits: number;
  cacheMisses: number;
}

export interface VerifyInvariantsOutput {
  verdicts: VerifyInvariantVerdict[];
  summary: VerifyInvariantsSummary;
  exitCode: number;
}

export interface MakeVerifyInvariantsStageDeps {
  /** Override producer identity. Default: "verifyInvariants@v1". */
  producerVersion?: string;
}

export function makeVerifyInvariantsStage(
  deps: MakeVerifyInvariantsStageDeps = {},
): Stage<VerifyInvariantsInput, VerifyInvariantsOutput> {
  const producedBy = deps.producerVersion ?? "verifyInvariants@v1";

  return {
    name: "verifyInvariants",
    producedBy,

    serializeInput(input) {
      const out: Record<string, unknown> = {
        projectRoot: input.projectRoot,
        adversarial: input.adversarial,
      };
      if (input.timeoutMs !== undefined) out.timeoutMs = input.timeoutMs;
      if (input.maxPaths !== undefined) out.maxPaths = input.maxPaths;
      return out;
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as VerifyInvariantsOutput;
    },

    async run(input) {
      return runVerifyInvariants(input);
    },
  };
}

export async function runVerifyInvariants(
  input: VerifyInvariantsInput,
): Promise<VerifyInvariantsOutput> {
  const { exitCodeFor } = await import("../../fix/runtime/verify.js");
  const { verifyAllCached } = await import("../../fix/runtime/verifyCache.js");

  type Report = Awaited<ReturnType<typeof verifyAllCached>>;
  let report: Report;
  if (input.adversarial) {
    const { verifyAll } = await import("../../fix/runtime/verify.js");
    const fresh = await verifyAll(input.projectRoot, {
      ...(input.timeoutMs !== undefined ? { timeoutMs: input.timeoutMs } : {}),
      ...(input.maxPaths !== undefined ? { maxPaths: input.maxPaths } : {}),
      adversarial: true,
    });
    report = {
      verdicts: fresh.verdicts.map((v) => ({ ...v, cacheStatus: "miss" as const })),
      summary: {
        ...fresh.summary,
        cacheHits: 0,
        cacheMisses: fresh.verdicts.length,
      },
    };
  } else {
    report = await verifyAllCached(input.projectRoot, {
      ...(input.timeoutMs !== undefined ? { timeoutMs: input.timeoutMs } : {}),
      ...(input.maxPaths !== undefined ? { maxPaths: input.maxPaths } : {}),
    });
  }

  return {
    verdicts: report.verdicts.map((v) => ({
      invariantId: v.invariant.id,
      scope: v.invariant.scope ?? "callsite",
      status: v.status,
      pathCheck: v.pathCheck,
      cacheStatus: v.cacheStatus,
    })),
    summary: report.summary,
    exitCode: exitCodeFor(report),
  };
}
