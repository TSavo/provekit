import { describe, it, expect } from "vitest";
import { createHash, randomBytes } from "node:crypto";
import { makeResolveBridgeTargetStage } from "./resolveBridgeTarget.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";
import { mintContract, mintMemento } from "../../claimEnvelope/index.js";
import { generateKeypair } from "../../producerKeys/index.js";

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

const IntSort = { kind: "primitive" as const, name: "Int" };

describe("resolveBridgeTarget", () => {
  const { privateKey } = generateKeypair({ seed: randomBytes(32) });

  it("resolves a contract with a precondition", async () => {
    const preProp = mintMemento({
      bindingHash: hash16("pre-prop"),
      propertyHash: hash16("pre"),
      producedBy: "test@1",
      privateKey,
      evidence: {
        kind: "property",
        schema: "test-schema",
        body: {
          irFormula: { kind: "atomic", predicate: ">", args: [], sort: IntSort },
          scope: { kind: "function", name: "pre" },
          irKitVersion: "test@1",
        },
      },
    });

    const contract = mintContract({
      bindingHash: hash16("contract"),
      propertyHash: hash16("contract"),
      producedBy: "test@1",
      privateKey,
      contractName: "TestContract",
      outBinding: "output",
      pre: preProp.cid,
      post: null,
      invariants: [],
      irKitVersion: "test@1",
    });

    const pool: Record<string, ClaimEnvelope> = {
      [contract.cid]: contract,
    };

    const stage = makeResolveBridgeTargetStage();
    const output = await stage.run({
      bridgeTargetContractCid: contract.cid,
      mementoPool: pool,
    });

    expect(output.resolved).toBeDefined();
    expect(output.resolved?.contractName).toBe("TestContract");
    expect(output.failureReason).toBeNull();
  });

  it("returns not-in-pool when CID is missing", async () => {
    const stage = makeResolveBridgeTargetStage();
    const output = await stage.run({
      bridgeTargetContractCid: "missing-cid",
      mementoPool: {},
    });

    expect(output.resolved).toBeNull();
    expect(output.failureReason).toBe("not-in-pool");
  });

  it("returns not-contract-variant when envelope is not a contract", async () => {
    const prop = mintMemento({
      bindingHash: hash16("prop"),
      propertyHash: hash16("prop"),
      producedBy: "test@1",
      privateKey,
      evidence: {
        kind: "property",
        schema: "test-schema",
        body: {
          irFormula: { kind: "atomic", predicate: "true", args: [], sort: IntSort },
          scope: { kind: "function", name: "test" },
          irKitVersion: "test@1",
        },
      },
    });

    const pool: Record<string, ClaimEnvelope> = {
      [prop.cid]: prop,
    };

    const stage = makeResolveBridgeTargetStage();
    const output = await stage.run({
      bridgeTargetContractCid: prop.cid,
      mementoPool: pool,
    });

    expect(output.resolved).toBeNull();
    expect(output.failureReason).toBe("not-contract-variant");
  });

  it("returns no-precondition when contract has no pre formula", async () => {
    const contract = mintContract({
      bindingHash: hash16("contract-postonly"),
      propertyHash: hash16("contract-postonly"),
      producedBy: "test@1",
      privateKey,
      contractName: "PostOnlyContract",
      outBinding: "output",
      pre: undefined as any,
      post: hash16("post-cid"),
      invariants: [],
      irKitVersion: "test@1",
    });

    const pool: Record<string, ClaimEnvelope> = {
      [contract.cid]: contract,
    };

    const stage = makeResolveBridgeTargetStage();
    const output = await stage.run({
      bridgeTargetContractCid: contract.cid,
      mementoPool: pool,
    });

    expect(output.resolved).toBeNull();
    expect(output.failureReason).toBe("no-precondition");
  });
});