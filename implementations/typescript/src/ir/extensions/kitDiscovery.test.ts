/**
 * Kit discovery tests. Builds a fake node_modules layout where each
 * protocol-aware package ships a real `.proof` file at its root,
 * runs discovery, asserts bridges are registered with provenance.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readdirSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { generateKeyPairSync, randomBytes } from "node:crypto";
import { discoverProtocolKits } from "./kitDiscovery.js";
import { _resetBridges, lookupBridge, primitiveBridge } from "./bridges.js";
import { mintBridge } from "../../claimEnvelope/index.js";
import { buildProofEnvelope } from "../../proofEnvelope/index.js";
import { generateKeypair } from "../../producerKeys/index.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";

beforeEach(() => {
  _resetBridges();
});

function makeFakeProject(): string {
  const root = mkdtempSync(join(tmpdir(), "kit-discovery-test-"));
  mkdirSync(join(root, "node_modules"), { recursive: true });
  return root;
}

interface BridgeSpec {
  irName: string;
  sourceLayer: string;
  targetCid: string;
  targetLayer: string;
}

/**
 * Mint bridge envelopes, compose them into a `.proof` file at the
 * package root, write a package.json with the proofHash hint, and
 * install the package under node_modules. The result is a
 * protocol-aware package the discovery walker can find and verify.
 */
function installFakePackageWithProof(
  projectRoot: string,
  name: string,
  version: string,
  bridges: BridgeSpec[],
): { packageRoot: string; proofCid: string } {
  const isScoped = name.startsWith("@");
  const packageRoot = isScoped
    ? join(projectRoot, "node_modules", ...name.split("/"))
    : join(projectRoot, "node_modules", name);
  mkdirSync(packageRoot, { recursive: true });

  // Mint bridge envelopes.
  const { privateKey: bridgeKey } = generateKeypair({ seed: randomBytes(32) });
  const members = new Map<string, ClaimEnvelope>();
  for (const b of bridges) {
    const env = mintBridge({
      bindingHash: hash16(`${b.sourceLayer}:${b.irName}`),
      propertyHash: hash16(`bridge:${b.irName}`),
      producedBy: `${b.sourceLayer}@test`,
      privateKey: bridgeKey,
      sourceSymbol: b.irName,
      sourceLayer: b.sourceLayer,
      targetContractCid: b.targetCid,
      targetLayer: b.targetLayer,
      irArgSorts: ["String"],
      irReturnSort: "Int",
    });
    members.set(env.cid, env);
  }

  // Build the .proof file.
  const { privateKey: catalogKey, publicKey: catalogPub } = generateKeypair({
    seed: randomBytes(32),
  });
  const pubDer = catalogPub.export({ type: "spki", format: "der" });
  const { createHash } = require("node:crypto");
  const signerCid =
    "sha256:" + createHash("sha256").update(pubDer).digest("hex").slice(0, 16);

  const built = buildProofEnvelope({
    name,
    version,
    members,
    signerCid,
    signerPrivateKey: catalogKey,
  });

  const proofPath = join(packageRoot, `${built.cid}.proof`);
  writeFileSync(proofPath, Buffer.from(built.bytes));

  // package.json with proofHash hint.
  const pkg = {
    name,
    version,
    main: "index.cjs",
    provekit: { proofHash: built.cid },
  };
  writeFileSync(join(packageRoot, "package.json"), JSON.stringify(pkg, null, 2));

  return { packageRoot, proofCid: built.cid };
}

function hash16(s: string): string {
  const { createHash } = require("node:crypto");
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

describe("discoverProtocolKits", () => {
  it("finds zero kits when node_modules is empty", async () => {
    const root = makeFakeProject();
    try {
      const result = await discoverProtocolKits(root);
      expect(result.kits).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("finds zero kits when node_modules doesn't exist", async () => {
    const root = mkdtempSync(join(tmpdir(), "kit-discovery-empty-"));
    try {
      const result = await discoverProtocolKits(root);
      expect(result.kits).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("ignores packages without a provekit field in package.json", async () => {
    const root = makeFakeProject();
    try {
      const pkgRoot = join(root, "node_modules", "lodash");
      mkdirSync(pkgRoot, { recursive: true });
      writeFileSync(
        join(pkgRoot, "package.json"),
        JSON.stringify({ name: "lodash", version: "4.17.21" }),
      );

      const result = await discoverProtocolKits(root);
      expect(result.kits).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("walks a package's .proof file and registers bridges with provenance", async () => {
    const root = makeFakeProject();
    try {
      installFakePackageWithProof(root, "@example/kit", "1.0.0", [
        { irName: "parseInt", sourceLayer: "ts", targetCid: "sha256:abc", targetLayer: "v8" },
        { irName: "abs", sourceLayer: "ts", targetCid: "sha256:def", targetLayer: "v8" },
      ]);

      const result = await discoverProtocolKits(root);
      expect(result.kits).toHaveLength(1);
      const kit = result.kits[0]!;
      expect(kit.packageName).toBe("@example/kit");
      expect(kit.packageVersion).toBe("1.0.0");
      expect(kit.errors).toEqual([]);
      expect(kit.registeredBridgeNames.sort()).toEqual(["abs", "parseInt"]);

      const parseInt = lookupBridge("parseInt");
      expect(parseInt).not.toBeNull();
      expect(parseInt!.sourceLayer).toBe("ts");
      expect(parseInt!.targetContractCid).toBe("sha256:abc");
      expect(parseInt!.targetLayer).toBe("v8");
      // Task #40: type signature is now carried in the bridge envelope
      // and round-trips through .proof discovery. No more empty argSorts.
      expect(parseInt!.irArgSorts).toEqual(["String"]);
      expect(parseInt!.irReturnSort).toBe("Int");
      expect(parseInt!.registeredBy).toEqual({
        packageName: "@example/kit",
        packageVersion: "1.0.0",
      });

      const abs = lookupBridge("abs");
      expect(abs).not.toBeNull();
      expect(abs!.registeredBy?.packageName).toBe("@example/kit");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("falls back to extension scan when no proofHash hint is present", async () => {
    const root = makeFakeProject();
    try {
      const { packageRoot, proofCid } = installFakePackageWithProof(
        root,
        "no-hint-kit",
        "0.1.0",
        [{ irName: "noHintFn", sourceLayer: "ts", targetCid: "sha256:xyz", targetLayer: "v8" }],
      );
      // Strip the hint from package.json to force the extension scan path.
      writeFileSync(
        join(packageRoot, "package.json"),
        JSON.stringify({ name: "no-hint-kit", version: "0.1.0", provekit: {} }, null, 2),
      );

      const result = await discoverProtocolKits(root);
      expect(result.kits).toHaveLength(1);
      const kit = result.kits[0]!;
      expect(kit.errors).toEqual([]);
      expect(kit.proofCid).toBe(proofCid);
      expect(kit.registeredBridgeNames).toEqual(["noHintFn"]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("rejects on tampered .proof file (rule 1: filename CID mismatch)", async () => {
    const root = makeFakeProject();
    try {
      const { packageRoot, proofCid } = installFakePackageWithProof(
        root,
        "tampered-kit",
        "0.1.0",
        [{ irName: "tampered", sourceLayer: "ts", targetCid: "sha256:t", targetLayer: "v8" }],
      );
      // Flip a byte in the .proof file (without renaming it).
      const fs = await import("fs");
      const proofPath = join(packageRoot, `${proofCid}.proof`);
      const bytes = fs.readFileSync(proofPath);
      bytes[Math.floor(bytes.length / 2)]! ^= 0xff;
      fs.writeFileSync(proofPath, bytes);

      const result = await discoverProtocolKits(root);
      expect(result.kits).toHaveLength(1);
      expect(result.kits[0]!.errors[0]).toMatch(/rule 1|trust root/);
      // Bridge MUST NOT be registered when trust-root check fails.
      expect(lookupBridge("tampered")).toBeNull();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("returns DiscoveryResult shape with kits + collisions + byName", async () => {
    const root = makeFakeProject();
    try {
      const result = await discoverProtocolKits(root);
      expect(result).toHaveProperty("kits");
      expect(result).toHaveProperty("collisions");
      expect(result).toHaveProperty("byName");
      expect(Array.isArray(result.kits)).toBe(true);
      expect(Array.isArray(result.collisions)).toBe(true);
      expect(typeof result.byName).toBe("object");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("tags pre-existing bridges as internal kit lazy-init", async () => {
    const root = makeFakeProject();
    try {
      // Register a bridge before discovery runs; discoverProtocolKits
      // should tag it as "(internal kit lazy-init)" since no protocol-
      // aware package claimed it.
      primitiveBridge({
        irName: "preregistered",
        irArgSorts: ["Int"],
        irReturnSort: "Int",
        sourceLayer: "test",
        targetContractCid: "cid",
        targetLayer: "test-layer",
      });
      const result = await discoverProtocolKits(root);
      const bridge = lookupBridge("preregistered");
      expect(bridge).not.toBeNull();
      expect(bridge!.registeredBy).toEqual({
        packageName: "(internal kit lazy-init)",
        packageVersion: "n/a",
      });
      expect(result.byName.preregistered).toEqual(bridge);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
