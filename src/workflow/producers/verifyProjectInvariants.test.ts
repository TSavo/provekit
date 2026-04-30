import { describe, it, expect, beforeEach } from "vitest";
import { writeFileSync, mkdirSync, existsSync, rmSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { generateKeypair } from "../../producerKeys/index.js";
import { _resetCollector } from "../../ir/symbolic/index.js";
import {
  runVerifyProjectInvariants,
  type VerifyProjectInvariantsStageInput,
  type InvariantFileSource,
} from "./verifyProjectInvariants.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIXTURE_DIR = join(__dirname, "__fixtures__", "verify");

const KEY_SEED = Buffer.from("verify-test-seed-padded-32-bytes").subarray(0, 32);

beforeEach(() => {
  _resetCollector();
  if (existsSync(FIXTURE_DIR)) {
    rmSync(FIXTURE_DIR, { recursive: true, force: true });
  }
  mkdirSync(FIXTURE_DIR, { recursive: true });
});

function writeFixture(filename: string, source: string): InvariantFileSource {
  const fullPath = join(FIXTURE_DIR, filename);
  writeFileSync(fullPath, source);
  return {
    path: filename,
    contentHash: "sha256-placeholder",
    resolvedModulePath: fullPath,
  };
}

const symbolicImportPath = join(__dirname, "..", "..", "ir", "symbolic", "index.js");
const importPathLiteral = JSON.stringify(symbolicImportPath);

const SAMPLE_PROPERTY_FILE = `
import { describe, must, eq, num } from ${importPathLiteral};

describe("trivial", () => {
  must("zero-equals-zero", eq(num(0), num(0)));
});
`;

const SAMPLE_BRIDGE_FILE = `
import { bridge } from ${importPathLiteral};

bridge("parseIntBridgesV8", {
  sourceSymbol: "global.parseInt",
  sourceLayer: "ts-kit@1.0",
  targetContractCid: "deadbeef".repeat(4),
  targetLayer: "V8@12.4 parseInt",
});
`;

const TWO_INVARIANTS_FILE = `
import { describe, must, eq, gt, num, parseInt, str } from ${importPathLiteral};

describe("parseInt", () => {
  must("zeroIsZero", eq(parseInt(str("0")), num(0)));
  must("returnsInt", gt(num(1), num(0)));
});
`;

describe("verifyProjectInvariants", () => {
  it("mints one memento per declaration in a property file", async () => {
    const kp = generateKeypair({ seed: KEY_SEED });
    const file = writeFixture("trivial.invariant.mjs", SAMPLE_PROPERTY_FILE);

    const input: VerifyProjectInvariantsStageInput = {
      projectName: "test-project",
      projectVersion: "0.0.1",
      invariantFiles: [file],
      locallyAvailableCids: [],
    };

    const out = await runVerifyProjectInvariants(input, {
      privateKey: kp.privateKey,
      producerId: "verify-test@v1",
      producedAt: new Date(0).toISOString(),
    });

    expect(out.declarations).toHaveLength(1);
    expect(out.declarations[0]!.declarationKind).toBe("property");
    expect(out.declarations[0]!.declarationName).toBe("trivial > zero-equals-zero");
    expect(out.declarations[0]!.cid).toMatch(/^[0-9a-f]{32}$/);
    expect(out.projectRootCid).toMatch(/^[0-9a-f]{32}$/);
  });

  it("mints a bridge memento for a bridge declaration", async () => {
    const kp = generateKeypair({ seed: KEY_SEED });
    const file = writeFixture("bridge.invariant.mjs", SAMPLE_BRIDGE_FILE);

    const input: VerifyProjectInvariantsStageInput = {
      projectName: "test-project",
      projectVersion: "0.0.1",
      invariantFiles: [file],
      locallyAvailableCids: [],
    };

    const out = await runVerifyProjectInvariants(input, {
      privateKey: kp.privateKey,
      producerId: "verify-test@v1",
      producedAt: new Date(0).toISOString(),
    });

    expect(out.declarations).toHaveLength(1);
    expect(out.declarations[0]!.declarationKind).toBe("bridge");
    expect(out.declarations[0]!.declarationName).toBe("parseIntBridgesV8");
  });

  it("composes multiple declarations from one file", async () => {
    const kp = generateKeypair({ seed: KEY_SEED });
    const file = writeFixture("two.invariant.mjs", TWO_INVARIANTS_FILE);

    const input: VerifyProjectInvariantsStageInput = {
      projectName: "test-project",
      projectVersion: "0.0.1",
      invariantFiles: [file],
      locallyAvailableCids: [],
    };

    const out = await runVerifyProjectInvariants(input, {
      privateKey: kp.privateKey,
      producerId: "verify-test@v1",
      producedAt: new Date(0).toISOString(),
    });

    expect(out.declarations).toHaveLength(2);
    expect(out.declarations.map((d) => d.declarationName)).toEqual([
      "parseInt > zeroIsZero",
      "parseInt > returnsInt",
    ]);
  });

  it("identifies bridge targetContractCid as a null root when not locally available", async () => {
    const kp = generateKeypair({ seed: KEY_SEED });
    const file = writeFixture("bridge2.invariant.mjs", SAMPLE_BRIDGE_FILE);

    const input: VerifyProjectInvariantsStageInput = {
      projectName: "test-project",
      projectVersion: "0.0.1",
      invariantFiles: [file],
      locallyAvailableCids: [],
    };

    const out = await runVerifyProjectInvariants(input, {
      privateKey: kp.privateKey,
      producerId: "verify-test@v1",
      producedAt: new Date(0).toISOString(),
    });

    // The bridge memento's inputCids includes the targetContractCid
    // (deadbeef × 4). Since locallyAvailableCids is empty, it's a null
    // root.
    expect(out.nullRoots).toContain("deadbeef".repeat(4));
  });

  it("does NOT report a null root when target is locally available", async () => {
    const kp = generateKeypair({ seed: KEY_SEED });
    const file = writeFixture("bridge3.invariant.mjs", SAMPLE_BRIDGE_FILE);

    const input: VerifyProjectInvariantsStageInput = {
      projectName: "test-project",
      projectVersion: "0.0.1",
      invariantFiles: [file],
      locallyAvailableCids: ["deadbeef".repeat(4)],
    };

    const out = await runVerifyProjectInvariants(input, {
      privateKey: kp.privateKey,
      producerId: "verify-test@v1",
      producedAt: new Date(0).toISOString(),
    });

    expect(out.nullRoots).not.toContain("deadbeef".repeat(4));
  });

  it("identical content from different files produces identical CIDs (content-addressing)", async () => {
    const kp = generateKeypair({ seed: KEY_SEED });
    // Two fixture files with byte-identical content. ESM caches by URL,
    // so the test isolates determinism by using distinct paths.
    const f1 = writeFixture("det1.invariant.mjs", SAMPLE_PROPERTY_FILE);
    const f2 = writeFixture("det2.invariant.mjs", SAMPLE_PROPERTY_FILE);
    const deps = {
      privateKey: kp.privateKey,
      producerId: "verify-test@v1",
      producedAt: new Date(0).toISOString(),
    };

    _resetCollector();
    const out1 = await runVerifyProjectInvariants(
      {
        projectName: "p",
        projectVersion: "0.0.1",
        invariantFiles: [f1],
        locallyAvailableCids: [],
      },
      deps,
    );

    _resetCollector();
    const out2 = await runVerifyProjectInvariants(
      {
        projectName: "p",
        projectVersion: "0.0.1",
        invariantFiles: [f2],
        locallyAvailableCids: [],
      },
      deps,
    );

    // Different file paths in the binding hash → different memento CIDs,
    // BUT the propertyHashes must be identical (the canonical FOL is
    // content-addressed, not path-addressed).
    expect(out1.declarations[0]!.propertyHash).toBe(
      out2.declarations[0]!.propertyHash,
    );
  });

  it("zero null roots when no bridges and no external references", async () => {
    const kp = generateKeypair({ seed: KEY_SEED });
    const file = writeFixture("trivial2.invariant.mjs", SAMPLE_PROPERTY_FILE);
    const input: VerifyProjectInvariantsStageInput = {
      projectName: "test-project",
      projectVersion: "0.0.1",
      invariantFiles: [file],
      locallyAvailableCids: [],
    };

    const out = await runVerifyProjectInvariants(input, {
      privateKey: kp.privateKey,
      producerId: "verify-test@v1",
      producedAt: new Date(0).toISOString(),
    });

    // A pure property memento has no inputCids, so it produces no null roots.
    expect(out.nullRoots).toEqual([]);
  });

  it("aggregates multiple files into one project root", async () => {
    const kp = generateKeypair({ seed: KEY_SEED });
    const f1 = writeFixture("a.invariant.mjs", SAMPLE_PROPERTY_FILE);
    const f2 = writeFixture("b.invariant.mjs", SAMPLE_BRIDGE_FILE);

    const input: VerifyProjectInvariantsStageInput = {
      projectName: "test-project",
      projectVersion: "0.0.1",
      invariantFiles: [f1, f2],
      locallyAvailableCids: [],
    };

    const out = await runVerifyProjectInvariants(input, {
      privateKey: kp.privateKey,
      producerId: "verify-test@v1",
      producedAt: new Date(0).toISOString(),
    });

    expect(out.declarations).toHaveLength(2);
    expect(out.declarations.map((d) => d.declarationKind).sort()).toEqual([
      "bridge",
      "property",
    ]);
  });
});
