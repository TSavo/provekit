import { describe, it, expect } from "vitest";
import { randomBytes, createHash } from "node:crypto";
import { generateKeypair } from "../../producerKeys/index.js";
import { mintProperty, mintBridge } from "../../claimEnvelope/index.js";
import { makeEnumerateBridgeCallsitesStage } from "./enumerateBridgeCallsites.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";
import type { IrFormula } from "../../ir/formulas.js";

const IntSort = { kind: "primitive" as const, name: "Int" };

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function buildBridge(): ClaimEnvelope {
  const { privateKey } = generateKeypair({ seed: randomBytes(32) });
  return mintBridge({
    bindingHash: hash16("ts:parseInt"),
    propertyHash: hash16("bridge:parseInt"),
    producedBy: "ts-kit@1",
    privateKey,
    sourceSymbol: "parseInt",
    sourceLayer: "ts",
    targetContractCid: "sha256:cpp_parseint_contract",
    targetLayer: "v8",
  });
}

function propertyMementoWithFormula(name: string, formula: IrFormula): ClaimEnvelope {
  const { privateKey } = generateKeypair({ seed: randomBytes(32) });
  return mintProperty({
    bindingHash: hash16(`prop:${name}`),
    propertyHash: hash16(`hash:${name}`),
    producedBy: "consumer@1",
    privateKey,
    irFormula: formula,
    scope: { kind: "function", name },
    irKitVersion: "ts-kit@1.0",
  });
}

describe("enumerateBridgeCallsites stage (pool-based)", () => {
  it("finds a Ctor matching a bridge symbol in a property memento's formula", async () => {
    const stage = makeEnumerateBridgeCallsitesStage();
    const bridgeEnv = buildBridge();
    const propEnv = propertyMementoWithFormula("test", {
      kind: "atomic",
      predicate: "=",
      args: [
        {
          kind: "ctor",
          name: "parseInt",
          args: [{ kind: "const", value: 5, sort: IntSort }],
          sort: IntSort,
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
    const propEnv = propertyMementoWithFormula("nested", {
      kind: "forall",
      sort: IntSort,
      predicate: {
        kind: "lambda",
        varName: "n",
        sort: IntSort,
        body: {
          kind: "implies",
          antecedent: {
            kind: "atomic",
            predicate: ">",
            args: [
              { kind: "var", name: "n", sort: IntSort },
              { kind: "const", value: 0, sort: IntSort },
            ],
          },
          consequent: {
            kind: "atomic",
            predicate: "=",
            args: [
              {
                kind: "ctor",
                name: "parseInt",
                args: [{ kind: "var", name: "n", sort: IntSort }],
                sort: IntSort,
              },
              { kind: "var", name: "n", sort: IntSort },
            ],
          },
        },
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
      sort: IntSort,
    });
  });

  it("ignores Ctors not matching any registered bridge", async () => {
    const stage = makeEnumerateBridgeCallsitesStage();
    const bridgeEnv = buildBridge();
    const propEnv = propertyMementoWithFormula("noBridges", {
      kind: "atomic",
      predicate: "=",
      args: [
        {
          kind: "ctor",
          name: "unrelated",
          args: [{ kind: "const", value: 1, sort: IntSort }],
          sort: IntSort,
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

  it("ignores non-property variants in the pool (bridges, etc.)", async () => {
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
    const propEnv = propertyMementoWithFormula("multiCall", {
      kind: "atomic",
      predicate: "=",
      args: [
        {
          kind: "ctor",
          name: "parseInt",
          args: [{ kind: "const", value: 5, sort: IntSort }],
          sort: IntSort,
        },
        {
          kind: "ctor",
          name: "parseInt",
          args: [{ kind: "const", value: 5, sort: IntSort }],
          sort: IntSort,
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
