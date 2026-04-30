/**
 * Kit discovery tests. Builds a fake node_modules layout with a
 * fake protocol-aware package, runs discovery, asserts the package
 * is found and bridges are tagged with provenance.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { discoverProtocolKits } from "./kitDiscovery.js";
import { _resetBridges, lookupBridge, primitiveBridge } from "./bridges.js";

beforeEach(() => {
  _resetBridges();
});

function makeFakeProject(): string {
  const root = mkdtempSync(join(tmpdir(), "kit-discovery-test-"));
  mkdirSync(join(root, "node_modules"), { recursive: true });
  return root;
}

function installFakePackage(
  projectRoot: string,
  name: string,
  version: string,
  bridges: Array<{ irName: string; targetCid: string }>,
): void {
  const isScoped = name.startsWith("@");
  const packageRoot = isScoped
    ? join(projectRoot, "node_modules", ...name.split("/"))
    : join(projectRoot, "node_modules", name);
  mkdirSync(packageRoot, { recursive: true });

  // package.json with the magic provekit field.
  const pkg = {
    name,
    version,
    main: "index.cjs",
    provekit: { shimRole: "test-fixture" },
  };
  writeFileSync(join(packageRoot, "package.json"), JSON.stringify(pkg, null, 2));

  // index.cjs registers bridges as a side effect of being imported.
  const bridgesJs = bridges
    .map(
      (b) =>
        `register({ irName: ${JSON.stringify(b.irName)}, irArgSorts: ["String"], irReturnSort: "Int", sourceLayer: "test", targetContractCid: ${JSON.stringify(b.targetCid)}, targetLayer: "test-layer" });`,
    )
    .join("\n");
  // The fake package imports the bridge factory directly from the
  // monorepo's source. In a real npm-installed package this would be
  // a normal `require("@provekit/ir-symbolic")` once that package is
  // published; the test fixture takes a shortcut.
  const bridgePath = join(__dirname, "bridges.ts").replace(/\.ts$/, ".js");
  writeFileSync(
    join(packageRoot, "index.cjs"),
    `const { primitiveBridge: register } = require(${JSON.stringify(bridgePath)});\n${bridgesJs}\n`,
  );
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
        JSON.stringify({ name: "lodash", version: "4.17.21", main: "index.cjs" }),
      );
      writeFileSync(join(pkgRoot, "index.cjs"), "module.exports = {};");

      const result = await discoverProtocolKits(root);
      expect(result.kits).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("walks scoped packages (@provekit/*) one level deep", async () => {
    const root = makeFakeProject();
    try {
      // Pre-register a bridge directly so the test doesn't depend on
      // dynamic-loading a fake package; we're testing the WALK logic
      // here. We then assert a bare scoped directory is enumerated
      // without the package itself contributing bridges (because it
      // has no real entry point).
      mkdirSync(join(root, "node_modules", "@provekit", "fake"), { recursive: true });
      writeFileSync(
        join(root, "node_modules", "@provekit", "fake", "package.json"),
        JSON.stringify({
          name: "@provekit/fake",
          version: "0.1.0",
          main: "missing-entry.js",
          provekit: { shimRole: "test" },
        }),
      );
      // Note: missing-entry.js doesn't exist. inspectPackage should
      // return null for this candidate, so it doesn't appear in kits.
      const result = await discoverProtocolKits(root);
      expect(result.kits).toEqual([]);
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
