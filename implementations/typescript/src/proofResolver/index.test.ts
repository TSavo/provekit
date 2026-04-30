import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { randomBytes, createHash } from "node:crypto";
import { generateKeypair } from "../producerKeys/index.js";
import { mintProperty, mintBridge } from "../claimEnvelope/index.js";
import { buildProofEnvelope } from "../proofEnvelope/index.js";
import { resolvePropertyFormula } from "./index.js";
import type { IrFormula } from "../ir/formulas.js";

const StringSort = { kind: "primitive" as const, name: "String" };

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function makeFakeProject(): string {
  const root = mkdtempSync(join(tmpdir(), "proof-resolver-test-"));
  mkdirSync(join(root, "node_modules"), { recursive: true });
  return root;
}

function installPackageWithMembers(
  projectRoot: string,
  packageName: string,
  envelopes: Map<string, ReturnType<typeof mintProperty>>,
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
  const signerCid = "sha256:" + createHash("sha256").update(pubDer).digest("hex").slice(0, 16);

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
  sort: StringSort,
  predicate: {
    kind: "lambda",
    varName: "s",
    sort: StringSort,
    body: {
      kind: "atomic",
      predicate: "nonempty",
      args: [{ kind: "var", name: "s", sort: StringSort }],
    },
  },
};

describe("resolvePropertyFormula", () => {
  it("returns null when no .proof file exists in any package", () => {
    const root = makeFakeProject();
    try {
      const result = resolvePropertyFormula(root, "deadbeef".repeat(4));
      expect(result).toBeNull();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("resolves a property memento by CID and returns its IrFormula", () => {
    const root = makeFakeProject();
    try {
      const { privateKey } = generateKeypair({ seed: randomBytes(32) });
      const propMemento = mintProperty({
        bindingHash: hash16("parseInt-precondition"),
        propertyHash: hash16("parseInt:requires-nonempty"),
        producedBy: "cpp-kit@1.0",
        privateKey,
        irFormula: NONEMPTY_FORMULA,
        scope: { kind: "function", name: "parseInt" },
        irKitVersion: "cpp-kit@1.0",
      });

      installPackageWithMembers(
        root,
        "@example/cpp-kit",
        new Map([[propMemento.cid, propMemento]]),
      );

      const result = resolvePropertyFormula(root, propMemento.cid);
      expect(result).not.toBeNull();
      expect(result!.irFormula).toEqual(NONEMPTY_FORMULA);
      expect(result!.irKitVersion).toBe("cpp-kit@1.0");
      expect(result!.scope).toEqual({ kind: "function", name: "parseInt" });
      expect(result!.packageName).toBe("@example/cpp-kit");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("returns null when the matched member is a bridge (not a property)", () => {
    const root = makeFakeProject();
    try {
      const { privateKey } = generateKeypair({ seed: randomBytes(32) });
      const bridge = mintBridge({
        bindingHash: hash16("ts:parseInt"),
        propertyHash: hash16("bridge:parseInt"),
        producedBy: "ts-kit@1",
        privateKey,
        sourceSymbol: "parseInt",
        sourceLayer: "ts",
        targetContractCid: "sha256:something",
        targetLayer: "v8",
        irArgSorts: ["String"],
        irReturnSort: "Int",
      });

      installPackageWithMembers(root, "fake-kit", new Map([[bridge.cid, bridge]]));

      const result = resolvePropertyFormula(root, bridge.cid);
      // Member found but it's a bridge variant, not a property → null.
      expect(result).toBeNull();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("returns null when CID exists in member but bytes don't re-derive", () => {
    const root = makeFakeProject();
    try {
      const { privateKey } = generateKeypair({ seed: randomBytes(32) });
      const real = mintProperty({
        bindingHash: hash16("real"),
        propertyHash: hash16("real"),
        producedBy: "k",
        privateKey,
        irFormula: NONEMPTY_FORMULA,
        scope: { kind: "function", name: "f" },
        irKitVersion: "k@1",
      });
      // Tamper: store a different envelope under the real CID's key.
      const tampered = { ...real, producedBy: "tampered" };
      installPackageWithMembers(root, "tamper-kit", new Map([[real.cid, tampered]]));

      const result = resolvePropertyFormula(root, real.cid);
      expect(result).toBeNull();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
