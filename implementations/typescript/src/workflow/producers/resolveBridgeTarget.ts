/**
 * resolve-bridge-target: Stage 3 of the bridge enforcement workflow.
 *
 * Hash lookup: given a bridge's targetContractCid + the unified
 * memento pool from load-all-proofs, returns the resolved contract
 * memento's pre-formula (the precondition the verifier instantiates
 * at the call site). O(1): no file IO; the pool was built once at
 * Stage 1.
 *
 * Returns null when the target CID isn't in the pool (target memento
 * not locally available: install the missing kit), when the matched
 * memento isn't a contract variant, or when the contract has no `pre`
 * formula (preconditions are the bridge-enforcement obligation source).
 *
 * Forward-pin gate (BridgeDeclaration.ConsequentBundlePinned, NORMATIVE):
 * after locating the consequent contract member, refuse to consume it
 * unless its containing `.proof` bundle CID matches the bridge's
 * `targetProofCid`. See protocol/specs/2026-04-30-ir-formal-grammar.md
 * § "Bridge target pinning: the shim-poisoning vector". Mirrors Rust
 * PR #13's resolve_target.rs.
 */

import type { Stage } from "../types.js";
import type { ContractEvidence } from "../../claimEnvelope/types.js";
import type { IrFormula } from "../../ir/formulas.js";
import type { MementoPool } from "../../verifier/mementoPool.js";

export const RESOLVE_BRIDGE_TARGET_CAPABILITY = "resolve-bridge-target";

export interface ResolveBridgeTargetInput {
  bridgeTargetContractCid: string;
  /**
   * Forward pin: the `.proof` bundle CID the bridge committed to as the
   * source of its consequent contract member. When set, resolution is
   * gated: the contract member MUST live in that bundle, else reject
   * with `BridgeTargetProofCidMismatch`. When `undefined`, the legacy
   * back-compat path is taken (soft warning, accept). Mirrors Rust
   * PR #13's `CallSite.bridge_target_proof_cid`.
   */
  bridgeTargetProofCid?: string;
  /**
   * IR name of the bridge being resolved. Used only for the soft-warn
   * message on the back-compat path so operators can identify which
   * bridge is missing the pin. Optional to keep the legacy call sites
   * working without churn.
   */
  bridgeIrName?: string;
  mementoPool: MementoPool;
}

export interface ResolvedProperty {
  cid: string;
  /** Precondition formula extracted from the contract memento's `pre` slot. */
  irFormula: IrFormula;
  contractName: string;
  outBinding: string;
}

export interface ResolveBridgeTargetOutput {
  resolved: ResolvedProperty | null;
  failureReason:
    | "not-in-pool"
    | "not-contract-variant"
    | "no-precondition"
    | "bundle-mismatch"
    | null;
  /**
   * Human-readable detail when `failureReason` is `"bundle-mismatch"`.
   * Always carries the magic substring `BridgeTargetProofCidMismatch`
   * so cross-impl conformance tests (Rust + TS) can pattern-match the
   * same shape. `undefined` for non-bundle-mismatch outcomes.
   */
  failureMessage?: string;
}

export interface MakeResolveBridgeTargetStageDeps {
  producerVersion?: string;
}

export function makeResolveBridgeTargetStage(
  deps: MakeResolveBridgeTargetStageDeps = {},
): Stage<ResolveBridgeTargetInput, ResolveBridgeTargetOutput> {
  // v4: gate on bridgeTargetProofCid and bundleMembers. The cache key in
  // serializeInput must include both so a stale @v3 hit can't bypass the
  // new check.
  const producedBy = deps.producerVersion ?? "resolveBridgeTarget@v4";

  return {
    name: "resolveBridgeTarget",
    producedBy,

    serializeInput(input) {
      // Cache key includes the pin and the bundleMembers slice the gate
      // reads. Without this, a v3 cache hit (computed before the gate
      // existed, or computed for a different pin) would silently bypass
      // ConsequentBundlePinned enforcement.
      const pinned = input.bridgeTargetProofCid;
      const pinnedMembers =
        pinned !== undefined
          ? [...(input.mementoPool.bundleMembers[pinned] ?? [])].sort()
          : [];
      return {
        bridgeTargetContractCid: input.bridgeTargetContractCid,
        bridgeTargetProofCid: pinned ?? null,
        pinnedBundleMembers: pinnedMembers,
        poolCids: Object.keys(input.mementoPool.mementos).sort(),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ResolveBridgeTargetOutput;
    },

    async run(input) {
      const envelope = input.mementoPool.mementos[input.bridgeTargetContractCid];
      if (!envelope) {
        return { resolved: null, failureReason: "not-in-pool" };
      }
      if (envelope.evidence?.kind !== "contract") {
        return { resolved: null, failureReason: "not-contract-variant" };
      }
      const ev = envelope.evidence as ContractEvidence;
      const pre = ev.body.pre as IrFormula | undefined;
      if (pre === undefined) {
        return { resolved: null, failureReason: "no-precondition" };
      }

      // Forward pin: BridgeDeclaration.ConsequentBundlePinned.
      //
      //     forall b: BridgeDeclaration, P: ProofBundle .
      //       AcceptedAsConsequentFor(P, b)  =>  Cid(P) = b.targetProofCid
      //
      // If the bridge pins a target proof CID, the contract member we
      // just resolved MUST come from that bundle. Bundles whose contract
      // members happen to share `targetContractCid` MUST NOT be
      // substituted for the pinned bundle. See
      // protocol/specs/2026-04-30-ir-formal-grammar.md
      // § "Bridge target pinning: the shim-poisoning vector".
      const pinned = input.bridgeTargetProofCid;
      if (pinned !== undefined) {
        const members = input.mementoPool.bundleMembers[pinned];
        if (!members) {
          return {
            resolved: null,
            failureReason: "bundle-mismatch",
            failureMessage:
              `BridgeTargetProofCidMismatch: pinned bundle ${pinned} not in pool`,
          };
        }
        if (!members.includes(input.bridgeTargetContractCid)) {
          return {
            resolved: null,
            failureReason: "bundle-mismatch",
            failureMessage:
              `BridgeTargetProofCidMismatch: contract ${input.bridgeTargetContractCid} ` +
              `is not a member of pinned bundle ${pinned}`,
          };
        }
      } else {
        // Back-compat: legacy bridges that pre-date `targetProofCid` are
        // loadable but cannot have ConsequentBundlePinned enforced. New
        // bridges MUST set the field; flag the gap so operators can see
        // what isn't being checked. Mirrors Rust resolve_target.rs:62-67.
        const name = input.bridgeIrName ?? input.bridgeTargetContractCid;
        console.warn(
          `warning: bridge ${name} has no targetProofCid; ` +
            `ConsequentBundlePinned not enforced (back-compat path)`,
        );
      }

      return {
        resolved: {
          cid: input.bridgeTargetContractCid,
          irFormula: pre,
          contractName: ev.body.contractName,
          outBinding: ev.body.outBinding,
        },
        failureReason: null,
      };
    },
  };
}

