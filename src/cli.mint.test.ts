/**
 * Tests for `provekit mint`. Exercises argv parsing for each subcommand
 * (property, bridge, catalog, generic) plus key handling and output
 * routing. Uses a real ed25519 PEM key written to tmpdir for the
 * signing path; the ephemeral fallback warning is asserted separately.
 */

import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import {
  mkdtempSync,
  writeFileSync,
  readFileSync,
  existsSync,
} from "fs";
import { generateKeyPairSync } from "crypto";
import { tmpdir } from "os";
import { join } from "path";

import { runMint } from "./cli.mint";
import { VARIANT_SCHEMA_CIDS } from "./claimEnvelope/index.js";

interface CapturedStream {
  read(): string;
  reset(): void;
}

function captureStdio(): {
  stdout: CapturedStream;
  stderr: CapturedStream;
  restore: () => void;
} {
  let out: string[] = [];
  let err: string[] = [];
  const origStdoutWrite = process.stdout.write.bind(process.stdout);
  const origStderrWrite = process.stderr.write.bind(process.stderr);
  process.stdout.write = ((chunk: string | Uint8Array) => {
    out.push(typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf-8"));
    return true;
  }) as typeof process.stdout.write;
  process.stderr.write = ((chunk: string | Uint8Array) => {
    err.push(typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf-8"));
    return true;
  }) as typeof process.stderr.write;
  return {
    stdout: {
      read: () => out.join(""),
      reset: () => {
        out = [];
      },
    },
    stderr: {
      read: () => err.join(""),
      reset: () => {
        err = [];
      },
    },
    restore: () => {
      process.stdout.write = origStdoutWrite;
      process.stderr.write = origStderrWrite;
    },
  };
}

function captureExit(): { code: () => number | undefined; restore: () => void } {
  let code: number | undefined;
  const orig = process.exit;
  process.exit = ((c?: number) => {
    code = c ?? 0;
    throw new Error("__test_exit__");
  }) as never;
  return {
    code: () => code,
    restore: () => {
      process.exit = orig;
    },
  };
}

const HEX32 = /^[0-9a-f]{32}$/;
const HEX16 = /^[0-9a-f]{16}$/;

let tmpDir: string;
let keyPath: string;

beforeAll(() => {
  // Generate a real ed25519 keypair once for the suite. mintMemento
  // requires a valid PEM key; spinning one up per test is expensive.
  const { privateKey } = generateKeyPairSync("ed25519");
  const pem = privateKey.export({ type: "pkcs8", format: "pem" }) as string;
  tmpDir = mkdtempSync(join(tmpdir(), "cli-mint-test-"));
  keyPath = join(tmpDir, "key.pem");
  writeFileSync(keyPath, pem, "utf-8");
});

describe("runMint", () => {
  let stdio: ReturnType<typeof captureStdio>;
  let exit: ReturnType<typeof captureExit>;

  beforeEach(() => {
    stdio = captureStdio();
    exit = captureExit();
  });

  afterEach(() => {
    stdio.restore();
    exit.restore();
  });

  it("with no args prints help to stderr and returns", async () => {
    await runMint([]);
    const err = stdio.stderr.read();
    expect(err).toContain("provekit mint");
    expect(err).toContain("Usage:");
    // Did not exit non-zero.
    expect(exit.code()).toBeUndefined();
  });

  it("--help is treated identically to no args", async () => {
    await runMint(["--help"]);
    const err = stdio.stderr.read();
    expect(err).toContain("Usage:");
  });

  it("unknown subcommand exits 1 with help output", async () => {
    let caught: Error | null = null;
    try {
      await runMint(["bogus"]);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught?.message).toBe("__test_exit__");
    expect(exit.code()).toBe(1);
    const err = stdio.stderr.read();
    expect(err).toContain("unknown 'mint' subcommand: bogus");
  });

  it("'mint property' with --spec writes a signed memento to stdout", async () => {
    const spec = {
      bindingHash: "deadbeefdeadbeef",
      propertyHash: "cafef00dcafef00d",
      verdict: "holds",
      producedBy: "test-producer@v1",
      inputCids: [],
      rawWitness: "{}",
    };
    const specPath = join(tmpDir, "prop-spec.json");
    writeFileSync(specPath, JSON.stringify(spec), "utf-8");

    await runMint(["property", "--spec", specPath, "--key", keyPath]);

    const out = stdio.stdout.read();
    const memento = JSON.parse(out);
    expect(memento.cid).toMatch(HEX32);
    expect(memento.bindingHash).toBe(spec.bindingHash);
    expect(memento.propertyHash).toBe(spec.propertyHash);
    expect(memento.verdict).toBe("holds");
    expect(memento.producedBy).toBe(spec.producedBy);
    // legacy-witness is the default evidence variant when none supplied.
    expect(memento.evidence?.kind).toBe("legacy-witness");
    expect(memento.evidence?.schema).toBe(VARIANT_SCHEMA_CIDS["legacy-witness"]);
    expect(memento.producerSignature).toBeDefined();
  });

  it("'mint property' with --out writes the memento to disk and not stdout", async () => {
    const spec = {
      bindingHash: "1234567890abcdef",
      propertyHash: "fedcba0987654321",
      verdict: "holds",
      producedBy: "test-producer@v1",
    };
    const specPath = join(tmpDir, "prop-spec-out.json");
    const outPath = join(tmpDir, "memento-out.json");
    writeFileSync(specPath, JSON.stringify(spec), "utf-8");

    await runMint([
      "property",
      "--spec",
      specPath,
      "--key",
      keyPath,
      "--out",
      outPath,
    ]);

    expect(existsSync(outPath)).toBe(true);
    const memento = JSON.parse(readFileSync(outPath, "utf-8"));
    expect(memento.bindingHash).toBe(spec.bindingHash);
    expect(memento.cid).toMatch(HEX32);

    // stdout was not used for the memento.
    expect(stdio.stdout.read()).toBe("");
    const err = stdio.stderr.read();
    expect(err).toContain(`wrote memento → ${outPath}`);
  });

  it("'mint bridge' with all required flags produces a signed bridge memento", async () => {
    await runMint([
      "bridge",
      "--source-symbol",
      "calculate",
      "--source-layer",
      "ts-lang",
      "--target-cid",
      "abc123",
      "--target-layer",
      "math-kit",
      "--key",
      keyPath,
    ]);

    const out = stdio.stdout.read();
    const memento = JSON.parse(out);
    expect(memento.cid).toMatch(HEX32);
    expect(memento.bindingHash).toMatch(HEX16);
    expect(memento.propertyHash).toMatch(HEX16);
    // Bridge mementos use the bridge evidence variant.
    expect(memento.evidence?.kind).toBe("bridge");
  });

  it("'mint bridge' missing required flag exits 1 with a clear error", async () => {
    let caught: Error | null = null;
    try {
      await runMint([
        "bridge",
        "--source-symbol",
        "calculate",
        // omit --source-layer
        "--target-cid",
        "abc123",
        "--target-layer",
        "math-kit",
      ]);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught?.message).toBe("__test_exit__");
    expect(exit.code()).toBe(1);
    const err = stdio.stderr.read();
    expect(err).toContain("--source-layer required");
  });

  it("'mint catalog' composes a .proof envelope from member memento JSON files", async () => {
    // Mint two member mementos to disk first.
    const catalogDir = mkdtempSync(join(tmpDir, "catalog-"));
    const member1Spec = {
      bindingHash: "aaaa1111aaaa1111",
      propertyHash: "bbbb2222bbbb2222",
      producedBy: "p1@v1",
    };
    const member2Spec = {
      bindingHash: "cccc3333cccc3333",
      propertyHash: "dddd4444dddd4444",
      producedBy: "p2@v1",
    };
    const member1SpecPath = join(catalogDir, "member1.spec.json");
    const member2SpecPath = join(catalogDir, "member2.spec.json");
    writeFileSync(member1SpecPath, JSON.stringify(member1Spec), "utf-8");
    writeFileSync(member2SpecPath, JSON.stringify(member2Spec), "utf-8");

    await runMint([
      "property",
      "--spec",
      member1SpecPath,
      "--key",
      keyPath,
      "--out",
      join(catalogDir, "member1.json"),
    ]);
    stdio.stderr.reset();
    stdio.stdout.reset();
    await runMint([
      "property",
      "--spec",
      member2SpecPath,
      "--key",
      keyPath,
      "--out",
      join(catalogDir, "member2.json"),
    ]);
    stdio.stderr.reset();
    stdio.stdout.reset();

    // Read both members' CIDs back so we can assert the .proof contents.
    const m1 = JSON.parse(readFileSync(join(catalogDir, "member1.json"), "utf-8"));
    const m2 = JSON.parse(readFileSync(join(catalogDir, "member2.json"), "utf-8"));

    // Now compose them. Output goes to a separate dir so we can find the .proof file.
    const outDir = mkdtempSync(join(tmpDir, "catalog-out-"));
    await runMint([
      "catalog",
      catalogDir,
      "--name",
      "test-catalog",
      "--version",
      "0.0.1",
      "--key",
      keyPath,
      "--out-dir",
      outDir,
    ]);

    // Find the produced .proof file.
    const { readdirSync } = await import("fs");
    const proofFiles = readdirSync(outDir).filter((f) => f.endsWith(".proof"));
    expect(proofFiles).toHaveLength(1);
    const proofPath = join(outDir, proofFiles[0]!);
    const cidFromFilename = proofFiles[0]!.replace(/\.proof$/, "");

    // Verify the file's bytes hash to its filename CID (trust root).
    const { createHash } = await import("node:crypto");
    const bytes = readFileSync(proofPath);
    const derivedCid = createHash("sha256").update(bytes).digest("hex").slice(0, 32);
    expect(derivedCid).toBe(cidFromFilename);

    // Decode and verify catalog structure.
    const { decodeProofEnvelope } = await import("./proofEnvelope/index.js");
    const catalog = decodeProofEnvelope(new Uint8Array(bytes));
    expect(catalog.kind).toBe("catalog");
    expect(catalog.name).toBe("test-catalog");
    expect(catalog.version).toBe("0.0.1");
    expect(catalog.members.size).toBe(2);
    expect(catalog.members.has(m1.cid)).toBe(true);
    expect(catalog.members.has(m2.cid)).toBe(true);
  });

  it("'mint catalog' without <dir> exits 1", async () => {
    // No flags at all — leaves no positional that could be misread as a dir.
    let caught: Error | null = null;
    try {
      await runMint(["catalog"]);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught?.message).toBe("__test_exit__");
    expect(exit.code()).toBe(1);
    const err = stdio.stderr.read();
    expect(err).toContain("requires <dir>");
  });

  it("'mint catalog' parser quirk: --key VALUE leaks the VALUE as a positional <dir>", async () => {
    // SURFACED BUG: parseFlags strips `--key`/`--out`/etc into flags,
    // but the catalog dispatch separately scans `rest` for a non-`--`
    // positional. The flag-VALUE strings (which don't start with `--`)
    // are still in `rest`, so a `--key /path/to/key.pem` invocation
    // makes the parser try to scan the keyPath itself as the catalog
    // directory. The error here is ENOTDIR on the key.pem path,
    // NOT the clean "requires <dir>" message a user would expect.
    let caught: Error | null = null;
    try {
      await runMint(["catalog", "--key", keyPath]);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught).not.toBeNull();
    // The specific failure mode: ENOTDIR on the misread key file.
    expect(caught!.message).toMatch(/ENOTDIR|not a directory/);
    expect(caught!.message).toContain(keyPath);
  });

  it("ephemeral key fallback warns to stderr and emits the public key", async () => {
    const spec = {
      bindingHash: "ffff0000ffff0000",
      propertyHash: "0000ffff0000ffff",
      verdict: "holds",
      producedBy: "ephemeral-test@v1",
    };
    const specPath = join(tmpDir, "ephem-spec.json");
    writeFileSync(specPath, JSON.stringify(spec), "utf-8");

    const savedEnv = process.env.PROVEKIT_KEY;
    delete process.env.PROVEKIT_KEY;
    try {
      await runMint(["property", "--spec", specPath]);
      const err = stdio.stderr.read();
      expect(err).toContain("warning: no key supplied");
      expect(err).toContain("generating ephemeral keypair");
      // SPKI base64 line on stderr.
      expect(err).toMatch(/ephemeral public key \(SPKI base64\):/);

      const out = stdio.stdout.read();
      const memento = JSON.parse(out);
      expect(memento.cid).toMatch(HEX32);
    } finally {
      if (savedEnv !== undefined) process.env.PROVEKIT_KEY = savedEnv;
    }
  });

  it("'mint generic' with full evidence variant in spec is honored", async () => {
    const spec = {
      bindingHash: "9999888877776666",
      propertyHash: "5555444433332222",
      verdict: "holds",
      producedBy: "generic-test@v1",
      inputCids: [],
      evidence: {
        kind: "legacy-witness",
        schema: VARIANT_SCHEMA_CIDS["legacy-witness"],
        body: {
          rawWitness: '{"explicit":"override"}',
          legacyProducerId: "generic-test@v1",
        },
      },
    };
    const specPath = join(tmpDir, "generic-spec.json");
    writeFileSync(specPath, JSON.stringify(spec), "utf-8");

    await runMint(["generic", "--spec", specPath, "--key", keyPath]);
    const out = stdio.stdout.read();
    const memento = JSON.parse(out);
    expect(memento.evidence?.kind).toBe("legacy-witness");
    expect(memento.evidence?.body?.rawWitness).toContain("explicit");
  });
});
