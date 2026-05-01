import { describe, it, expect } from "vitest";
import { randomBytes } from "node:crypto";
import { generateKeypair } from "../../producerKeys/index.js";
import { mintContract, mintBridge } from "../../claimEnvelope/index.js";
import { makeEnumerateBridgeCallsitesStage } from "./enumerateBridgeCallsites.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";
import type { IrFormula } from "../../ir/formulas.js";

const IntSort = { kind: "primitive" as const, name: "Int" };

function buildBridge(): ClaimEnvelope {
  const { privateKey } = generateKeypair({ seed: randomBytes(32) });
  return mintBridge({
    producedBy: "ts-kit@1",
    privateKey,
    sourceSymbol: "parseInt",
    sourceLayer: "ts",
    targetContractCid: "1".repeat(32),
    targetLayer: "v8",
    irArgSorts: ["String"],
    irReturnSort: "Int",
  });
}

function contractWithPre(name: string, formula: IrFormula): ClaimEnvelope {
  const { privateKey } = generateKeypair({ seed: randomBytes(32) });
  return mintContract({
    producedBy: "consumer@1",
    privateKey,
    contractName: name,
    pre: formula,
    authoring: { producerKind: "kit-author", author: "consumer@1" },
  });
}

describe("enumerateBridgeCallsites stage (pool-based)", () => {
  it("finds a Ctor matching a bridge symbol in a contract memento's pre formula", async () => {
    const stage = makeEnumerateBridgeCallsitesStage();
    const bridgeEnv = buildBridge();
    const propEnv = contractWithPre("test", {
      kind: "atomic",
      name: "=",
      args: [
        {
          kind: "ctor",
          name: "parseInt",
          args: [{ kind: "const", value: 5, sort: IntSort }],
        },
        { kind: "const", value: 5, sort: IntSort },
      ],
    });

    const result = await stage.run({
      mementoPool: { [propEnv.cid]: propEnv, [bridgeEnv.cid]: bridgeEnv },
      bridgesBySymbol: { parseInt: bridgeEnv },
    });
    expect(result.callsites).toHaveLength(1);
    expect(result.callsites[0]!.bridgeIrName).toBe("parseInt");
    expect(result.callsites[0]!.argTerms[0]).toEqual({
      kind: "const",
      value: 5,
      sort: IntSort,
    });
    expect(result.callsites[0]!.propertyName).toBe("test");
    expect(result.callsites[0]!.propertyCid).toBe(propEnv.cid);
  });

  it("walks nested formula constructors (forall, implies, and)", async () => {
    const stage = makeEnumerateBridgeCallsitesStage();
    const bridgeEnv = buildBridge();
    const propEnv = contractWithPre("nested", {
      kind: "forall",
      name: "n",
      sort: IntSort,
      body: {
        kind: "implies",
        operands: [
          {
            kind: "atomic",
            name: ">",
            args: [
              { kind: "var", name: "n" },
              { kind: "const", value: 0, sort: IntSort },
            ],
          },
          {
            kind: "atomic",
            name: "=",
            args: [
              {
                kind: "ctor",
                name: "parseInt",
                args: [{ kind: "var", name: "n" }],
              },
              { kind: "var", name: "n" },
            ],
          },
        ],
      },
    });
    const result = await stage.run({
      mementoPool: { [propEnv.cid]: propEnv, [bridgeEnv.cid]: bridgeEnv },
      bridgesBySymbol: { parseInt: bridgeEnv },
    });
    expect(result.callsites).toHaveLength(1);
    expect(result.callsites[0]!.argTerms[0]).toEqual({
      kind: "var",
      name: "n",
    });
  });

  it("ignores Ctors not matching any registered bridge", async () => {
    const stage = makeEnumerateBridgeCallsitesStage();
    const bridgeEnv = buildBridge();
    const propEnv = contractWithPre("noBridges", {
      kind: "atomic",
      name: "=",
      args: [
        {
          kind: "ctor",
          name: "unrelated",
          args: [{ kind: "const", value: 1, sort: IntSort }],
        },
        { kind: "const", value: 1, sort: IntSort },
      ],
    });
    const result = await stage.run({
      mementoPool: { [propEnv.cid]: propEnv, [bridgeEnv.cid]: bridgeEnv },
      bridgesBySymbol: { parseInt: bridgeEnv },
    });
    expect(result.callsites).toEqual([]);
  });

  it("ignores non-contract variants in the pool (bridges, etc.)", async () => {
    const stage = makeEnumerateBridgeCallsitesStage();
    const bridgeEnv = buildBridge();
    const result = await stage.run({
      mementoPool: { [bridgeEnv.cid]: bridgeEnv },
      bridgesBySymbol: { parseInt: bridgeEnv },
    });
    expect(result.callsites).toEqual([]);
  });

  it("finds multiple call sites within a single formula", async () => {
    const stage = makeEnumerateBridgeCallsitesStage();
    const bridgeEnv = buildBridge();
    const propEnv = contractWithPre("multiCall", {
      kind: "atomic",
      name: "=",
      args: [
        {
          kind: "ctor",
          name: "parseInt",
          args: [{ kind: "const", value: 5, sort: IntSort }],
        },
        {
          kind: "ctor",
          name: "parseInt",
          args: [{ kind: "const", value: 5, sort: IntSort }],
        },
      ],
    });
    const result = await stage.run({
      mementoPool: { [propEnv.cid]: propEnv, [bridgeEnv.cid]: bridgeEnv },
      bridgesBySymbol: { parseInt: bridgeEnv },
    });
    expect(result.callsites).toHaveLength(2);
  });
});
