/**
 * resolveBridgeTarget: ConsequentBundlePinned enforcement tests.
 *
 * Mirrors Rust PR #13's `tests/resolve_target.rs` four forward-pin
 * cases. The behavior must match cross-impl: a poisoned bundle CID
 * rejected by Rust must also be rejected by TS, and a pin-less legacy
 * bridge accepted by Rust must also be accepted by TS (with a soft
 * warn).
 *
 * Spec: protocol/specs/2026-04-30-ir-formal-grammar.md
 *   § "Bridge target pinning: the shim-poisoning vector"
 */

import { describe, it, expect, vi, afterEach } from "vitest";
import { makeResolveBridgeTargetStage } from "./resolveBridgeTarget.js";
import type { ClaimEnvelope, ContractEvidence } from "../../claimEnvelope/types.js";
import type { MementoPool } from "../../verifier/mementoPool.js";
import { createMementoPool, recordBundleMember } from "../../verifier/mementoPool.js";

const TRIVIAL_PRE = {
  kind: "atomic" as const,
  name: "true",
  args: [],
};

function poolWithContract(cid: string): MementoPool {
  const pool = createMementoPool();
  // Hand-built minimal contract envelope. The stage only reads
  // evidence.kind, evidence.body.pre, evidence.body.contractName,
  // and evidence.body.outBinding; we do not need a real signed memento
  // to exercise the resolution gate.
  const evidence: ContractEvidence = {
    kind: "contract",
    schema: "blake3-512:test-schema",
    body: {
      contractName: "parseInt",
      outBinding: "out",
      pre: TRIVIAL_PRE,
      authoring: { producerKind: "kit-author", author: "test" },
    },
  };
  const envelope: ClaimEnvelope = {
    schemaVersion: "1",
    bindingHash: "deadbeef".repeat(2),
    propertyHash: "cafef00d".repeat(2),
    verdict: "holds",
    producedBy: "test@1",
    producedAt: "2026-05-02T00:00:00.000Z",
    inputCids: [],
    evidence,
    cid,
  };
  pool.mementos[cid] = envelope;
  return pool;
}

describe("resolveBridgeTarget: ConsequentBundlePinned enforcement", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  // -------------------------------------------------------------------------
  // (a) Mismatch -> reject. Spec scenario A: shim-poisoning.
  //
  // The contract member exists in the pool but was loaded as part of a
  // poisoned bundle. The bridge pinned an honest bundle. Resolution
  // MUST fail with `BridgeTargetProofCidMismatch`.
  // Mirrors Rust `rejects_when_target_proof_cid_does_not_match_bundle`.
  // -------------------------------------------------------------------------
  it("rejects when targetProofCid does not match the bundle the contract was loaded from", async () => {
    const targetCid = "blake3-512:contract-shared";
    const honestBundle = "blake3-512:node-v24-proof-honest";
    const poisonedBundle = "blake3-512:node-v24-proof-poisoned";

    const pool = poolWithContract(targetCid);
    // The contract member lives in the poisoned bundle. The honest
    // bundle is what the bridge pinned but is not loaded.
    recordBundleMember(pool, poisonedBundle, targetCid);

    const stage = makeResolveBridgeTargetStage();
    const result = await stage.run({
      bridgeTargetContractCid: targetCid,
      bridgeTargetProofCid: honestBundle,
      bridgeIrName: "parseInt",
      mementoPool: pool,
    });

    expect(result.resolved).toBeNull();
    expect(result.failureReason).toBe("bundle-mismatch");
    expect(result.failureMessage).toBeDefined();
    expect(result.failureMessage).toContain("BridgeTargetProofCidMismatch");
  });

  // -------------------------------------------------------------------------
  // (b) Pinned bundle missing entirely -> reject. Fail-closed.
  //
  // Mirrors Rust `rejects_when_pinned_bundle_is_not_loaded`.
  // -------------------------------------------------------------------------
  it("rejects when the pinned bundle is not in the pool at all", async () => {
    const targetCid = "blake3-512:contract-orphan";
    const pool = poolWithContract(targetCid);
    // Note: deliberately NO call to recordBundleMember. The pinned
    // bundle is unknown to the pool.

    const stage = makeResolveBridgeTargetStage();
    const result = await stage.run({
      bridgeTargetContractCid: targetCid,
      bridgeTargetProofCid: "blake3-512:never-loaded",
      bridgeIrName: "parseInt",
      mementoPool: pool,
    });

    expect(result.resolved).toBeNull();
    expect(result.failureReason).toBe("bundle-mismatch");
    expect(result.failureMessage).toContain("BridgeTargetProofCidMismatch");
    expect(result.failureMessage).toContain("blake3-512:never-loaded");
  });

  // -------------------------------------------------------------------------
  // (c) Match -> accept. Spec scenario B.
  //
  // Mirrors Rust `accepts_when_target_proof_cid_matches_bundle`.
  // -------------------------------------------------------------------------
  it("accepts when the contract member is in the pinned bundle", async () => {
    const targetCid = "blake3-512:contract-pinned";
    const honestBundle = "blake3-512:node-v24-proof-honest";

    const pool = poolWithContract(targetCid);
    recordBundleMember(pool, honestBundle, targetCid);

    const stage = makeResolveBridgeTargetStage();
    const result = await stage.run({
      bridgeTargetContractCid: targetCid,
      bridgeTargetProofCid: honestBundle,
      bridgeIrName: "parseInt",
      mementoPool: pool,
    });

    expect(result.failureReason).toBeNull();
    expect(result.resolved).not.toBeNull();
    expect(result.resolved!.cid).toBe(targetCid);
    expect(result.resolved!.irFormula).toEqual(TRIVIAL_PRE);
  });

  // -------------------------------------------------------------------------
  // (d) None -> accept with soft warn. Back-compat for legacy bridges.
  //
  // Mirrors Rust `accepts_when_target_proof_cid_is_none_back_compat`.
  // -------------------------------------------------------------------------
  it("accepts a legacy bridge with no targetProofCid and emits a soft warning", async () => {
    const targetCid = "blake3-512:contract-legacy";
    const pool = poolWithContract(targetCid);

    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

    const stage = makeResolveBridgeTargetStage();
    const result = await stage.run({
      bridgeTargetContractCid: targetCid,
      // bridgeTargetProofCid intentionally omitted.
      bridgeIrName: "parseIntLegacy",
      mementoPool: pool,
    });

    expect(result.failureReason).toBeNull();
    expect(result.resolved).not.toBeNull();
    expect(result.resolved!.cid).toBe(targetCid);
    expect(warnSpy).toHaveBeenCalledTimes(1);
    const warnMsg = warnSpy.mock.calls[0]![0] as string;
    expect(warnMsg).toContain("parseIntLegacy");
    expect(warnMsg).toContain("targetProofCid");
    expect(warnMsg).toContain("ConsequentBundlePinned not enforced");
  });
});
