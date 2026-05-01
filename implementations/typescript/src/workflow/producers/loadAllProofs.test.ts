import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { makeLoadAllProofsStage } from "./loadAllProofs.js";
import { generateKeypair } from "../../producerKeys/index.js";
import { mintMemento, mintBridge } from "../../claimEnvelope/index.js";
import { buildProofEnvelope } from "../../proofEnvelope/index.js";
import { randomBytes, createHash } from "node:crypto";

const IntSort = { kind: "primitive" as const, name: "Int" };

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

describe("loadAllProofs", () => {
  let projectRoot: string;
  let privateKey: Uint8Array;

  beforeEach(() => {
    projectRoot = mkdtempSync(join(tmpdir(), "load-all-proofs-"));
    const { privateKey: pk } = generateKeypair({ seed: randomBytes(32) });
    privateKey = pk;
  });

  afterEach(() => {
    rmSync(projectRoot, { recursive: true, force: true });
  });

  it("returns empty pool when no .proof files exist", async () => {
    const stage = makeLoadAllProofsStage();
    const output = await stage.run({ projectRoot });
    expect(output.mementoPool).toEqual({});
    expect(output.bridgesBySymbol).toEqual({});
    expect(output.errors).toEqual([]);
  });

  it("loads .proof files from project root", async () => {
    const propertyMemento = mintMemento({
      bindingHash: hash16("test-property"),
      propertyHash: hash16("test-prop"),
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

    const built = buildProofEnvelope({
      name: "test-package",
      version: "1.0.0",
      members: new Map([[propertyMemento.cid, propertyMemento]]),
      signerCid: "test",
      signerPrivateKey: privateKey,
    });

    writeFileSync(join(projectRoot, `${built.cid}.proof`), Buffer.from(built.bytes));

    const stage = makeLoadAllProofsStage();
    const output = await stage.run({ projectRoot });

    expect(Object.keys(output.mementoPool)).toContain(propertyMemento.cid);
    expect(output.errors).toEqual([]);
  });

  it("loads .proof files from node_modules", async () => {
    const propertyMemento = mintMemento({
      bindingHash: hash16("test-property"),
      propertyHash: hash16("test-prop"),
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

    const built = buildProofEnvelope({
      name: "test-package",
      version: "1.0.0",
      members: new Map([[propertyMemento.cid, propertyMemento]]),
      signerCid: "test",
      signerPrivateKey: privateKey,
    });

    const nodeModules = join(projectRoot, "node_modules", "test-pkg");
    mkdirSync(nodeModules, { recursive: true });
    writeFileSync(join(nodeModules, `${built.cid}.proof`), Buffer.from(built.bytes));

    const stage = makeLoadAllProofsStage();
    const output = await stage.run({ projectRoot });

    expect(Object.keys(output.mementoPool)).toContain(propertyMemento.cid);
  });

  it("indexes bridge envelopes by sourceSymbol", async () => {
    const propertyMemento = mintMemento({
      bindingHash: hash16("test-precondition"),
      propertyHash: hash16("test-precondition"),
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

    const bridgeMemento = mintBridge({
      bindingHash: hash16("test-bridge"),
      propertyHash: hash16("test-bridge"),
      producedBy: "test@1",
      privateKey,
      sourceSymbol: "parseInt",
      sourceLayer: "ts",
      targetContractCid: propertyMemento.cid,
      targetLayer: "test",
      irArgSorts: ["String"],
      irReturnSort: "Int",
    });

    const built = buildProofEnvelope({
      name: "test-package",
      version: "1.0.0",
      members: new Map([
        [propertyMemento.cid, propertyMemento],
        [bridgeMemento.cid, bridgeMemento],
      ]),
      signerCid: "test",
      signerPrivateKey: privateKey,
    });

    writeFileSync(join(projectRoot, `${built.cid}.proof`), Buffer.from(built.bytes));

    const stage = makeLoadAllProofsStage();
    const output = await stage.run({ projectRoot });

    expect(output.bridgesBySymbol.parseInt).toBeDefined();
    expect(output.bridgesBySymbol.parseInt?.evidence?.kind).toBe("bridge");
  });

  it("records errors for unreadable files", async () => {
    const proofDir = join(projectRoot, "node_modules", "bad-pkg");
    mkdirSync(proofDir, { recursive: true });
    writeFileSync(join(proofDir, "corrupted.proof"), "not valid cbor");

    const stage = makeLoadAllProofsStage();
    const output = await stage.run({ projectRoot });

    expect(output.errors).toHaveLength(1);
    expect(output.errors[0]?.reason).toContain("decode");
  });

  it("skips node_modules nested under node_modules", async () => {
    const outerNodeModules = join(projectRoot, "node_modules", "outer");
    mkdirSync(outerNodeModules, { recursive: true });
    writeFileSync(join(outerNodeModules, "test.proof"), "corrupt");

    const nestedNodeModules = join(outerNodeModules, "node_modules", "inner");
    mkdirSync(nestedNodeModules, { recursive: true });
    writeFileSync(join(nestedNodeModules, "nested.proof"), "corrupt");

    const stage = makeLoadAllProofsStage();
    const output = await stage.run({ projectRoot });

    expect(output.errors).toHaveLength(1);
    expect(output.errors[0]?.proofFile).not.toContain("node_modules/node_modules");
  });
});