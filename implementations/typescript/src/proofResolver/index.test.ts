import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { randomBytes } from "node:crypto";
import { generateKeypair } from "../producerKeys/index.js";
import { mintContract, mintBridge } from "../claimEnvelope/index.js";
import { buildProofEnvelope } from "../proofEnvelope/index.js";
import { computeCid } from "../canonicalizer/hash.js";
import { resolvePropertyFormula } from "./index.js";
import type { IrFormula } from "../ir/formulas.js";
import type { ClaimEnvelope } from "../claimEnvelope/types.js";

const StringSort = { kind: "primitive" as const, name: "String" };

function makeFakeProject(): string {
  const root = mkdtempSync(join(tmpdir(), "proof-resolver-test-"));
  mkdirSync(join(root, "node_modules"), { recursive: true });
  return root;
}

function installPackageWithMembers(
  projectRoot: string,
  packageName: string,
  envelopes: Map<string, ClaimEnvelope>,
): { proofCid: string } {
  const isScoped = packageName.startsWith("@");
  const packageRoot = isScoped
    ? join(projectRoot, "node_modules", ...packageName.split("/"))
    : join(projectRoot, "node_modules", packageName);
  mkdirSync(packageRoot, { recursive: true });

  const { privateKey: catalogKey, publicKey: catalogPub } = generateKeypair({
    seed: randomBytes(32),
  });
  const pubDer = catalogPub.export({ type: "spki", format: "der" });
  const signerCid = computeCid(pubDer);

  const built = buildProofEnvelope({
    name: packageName,
    version: "1.0.0",
    members: envelopes,
    signerCid,
    signerPrivateKey: catalogKey,
  });

  writeFileSync(join(packageRoot, `${built.cid}.proof`), Buffer.from(built.bytes));
  writeFileSync(
    join(packageRoot, "package.json"),
    JSON.stringify(
      { name: packageName, version: "1.0.0", provekit: { proofHash: built.cid } },
      null,
      2,
    ),
  );
  return { proofCid: built.cid };
}

const NONEMPTY_FORMULA: IrFormula = {
  kind: "forall",
  name: "s",
  sort: StringSort,
  body: {
    kind: "atomic",
    name: "nonempty",
    args: [{ kind: "var", name: "s" }],
  },
};

describe("resolvePropertyFormula", () => {
  it("returns null when no .proof file exists in any package", () => {
    const root = makeFakeProject();
    try {
      const result = resolvePropertyFormula(
        root,
        "blake3-512:" + "deadbeef".repeat(16),
      );
      expect(result).toBeNull();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("resolves a contract memento by CID and returns its precondition formula", () => {
    const root = makeFakeProject();
    try {
      const { privateKey } = generateKeypair({ seed: randomBytes(32) });
      const contract = mintContract({
        producedBy: "cpp-kit@1.0",
        privateKey,
        contractName: "parseInt",
        pre: NONEMPTY_FORMULA,
        authoring: { producerKind: "kit-author", author: "cpp-kit@1.0" },
      });

      installPackageWithMembers(
        root,
        "@example/cpp-kit",
        new Map([[contract.cid, contract]]),
      );

      const result = resolvePropertyFormula(root, contract.cid);
      expect(result).not.toBeNull();
      expect(result!.irFormula).toEqual(NONEMPTY_FORMULA);
      expect(result!.contractName).toBe("parseInt");
      expect(result!.outBinding).toBe("out");
      expect(result!.packageName).toBe("@example/cpp-kit");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("returns null when the matched member is a bridge (not a contract)", () => {
    const root = makeFakeProject();
    try {
      const { privateKey } = generateKeypair({ seed: randomBytes(32) });
      const bridge = mintBridge({
        producedBy: "ts-kit@1",
        privateKey,
        sourceSymbol: "parseInt",
        sourceLayer: "ts",
        targetContractCid: "blake3-512:" + "0".repeat(128),
        targetLayer: "v8",
        irArgSorts: ["String"],
        irReturnSort: "Int",
      });

      installPackageWithMembers(root, "fake-kit", new Map([[bridge.cid, bridge]]));

      const result = resolvePropertyFormula(root, bridge.cid);
      expect(result).toBeNull();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("returns null when CID exists in member but bytes don't re-derive", () => {
    const root = makeFakeProject();
    try {
      const { privateKey } = generateKeypair({ seed: randomBytes(32) });
      const real = mintContract({
        producedBy: "k@1",
        privateKey,
        contractName: "f",
        pre: NONEMPTY_FORMULA,
        authoring: { producerKind: "kit-author", author: "k@1" },
      });
      const tampered = { ...real, producedBy: "tampered@1" };
      installPackageWithMembers(root, "tamper-kit", new Map([[real.cid, tampered]]));

      const result = resolvePropertyFormula(root, real.cid);
      expect(result).toBeNull();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
