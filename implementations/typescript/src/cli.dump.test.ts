/**
 * Tests for `provekit dump`. End-to-end via runMint(catalog) → runDump.
 */

import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import { mkdtempSync, writeFileSync, readdirSync, readFileSync, writeFileSync as fsWriteFileSync } from "fs";
import { generateKeyPairSync } from "crypto";
import { tmpdir } from "os";
import { join } from "path";

import { runMint } from "./cli.mint.js";
import { runDump } from "./cli.dump.js";
import { VARIANT_SCHEMA_CIDS } from "./claimEnvelope/index.js";

/** Pad a short suffix into a self-identifying BLAKE3-512 placeholder hash. */
function fakeCid(suffix: string): string {
  return `blake3-512:${suffix.padStart(128, "0")}`;
}

function captureStdio() {
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
    stdout: { read: () => out.join(""), reset: () => { out = []; } },
    stderr: { read: () => err.join(""), reset: () => { err = []; } },
    restore: () => {
      process.stdout.write = origStdoutWrite;
      process.stderr.write = origStderrWrite;
    },
  };
}

function captureExit() {
  let code: number | undefined;
  const orig = process.exit;
  process.exit = ((c?: number) => {
    code = c ?? 0;
    throw new Error("__test_exit__");
  }) as never;
  return { code: () => code, restore: () => { process.exit = orig; } };
}

let tmpDir: string;
let keyPath: string;

beforeAll(() => {
  const { privateKey } = generateKeyPairSync("ed25519");
  const pem = privateKey.export({ type: "pkcs8", format: "pem" }) as string;
  tmpDir = mkdtempSync(join(tmpdir(), "cli-dump-test-"));
  keyPath = join(tmpDir, "key.pem");
  writeFileSync(keyPath, pem, "utf-8");
});

async function buildSampleProof(): Promise<string> {
  // Mint two members + a catalog into a fresh dir.
  const dir = mkdtempSync(join(tmpDir, "src-"));
  const z3UnsatEvidence = {
    kind: "z3-unsat",
    schema: VARIANT_SCHEMA_CIDS["z3-unsat"],
    body: { smtLibInput: "(check-sat)\n", z3Verdict: "unsat", z3RunMs: 1 },
  };
  for (const name of ["m1", "m2"]) {
    const spec = {
      bindingHash: name === "m1"
        ? fakeCid("aaaa1111aaaa1111")
        : fakeCid("cccc3333cccc3333"),
      propertyHash: name === "m1"
        ? fakeCid("bbbb2222bbbb2222")
        : fakeCid("dddd4444dddd4444"),
      producedBy: `${name}@v1`,
      evidence: z3UnsatEvidence,
    };
    const specPath = join(dir, `${name}.spec.json`);
    fsWriteFileSync(specPath, JSON.stringify(spec));
    await runMint(["property", "--spec", specPath, "--key", keyPath, "--out", join(dir, `${name}.json`)]);
  }
  const outDir = mkdtempSync(join(tmpDir, "out-"));
  await runMint([
    "catalog",
    dir,
    "--name",
    "dump-test",
    "--version",
    "0.0.1",
    "--key",
    keyPath,
    "--out-dir",
    outDir,
  ]);
  const proofs = readdirSync(outDir).filter((f) => f.endsWith(".proof"));
  if (proofs.length !== 1) throw new Error(`expected 1 .proof, got ${proofs.length}`);
  return join(outDir, proofs[0]!);
}

describe("runDump", () => {
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

  it("with no args exits 1 with usage", async () => {
    let caught: Error | null = null;
    try {
      await runDump([]);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught?.message).toBe("__test_exit__");
    expect(exit.code()).toBe(1);
    expect(stdio.stderr.read()).toContain("requires <file>.proof");
  });

  it("dumps a freshly minted .proof, exits 0, all CIDs match", async () => {
    const proofPath = await buildSampleProof();
    stdio.stdout.reset();
    stdio.stderr.reset();

    let caught: Error | null = null;
    try {
      await runDump([proofPath]);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught?.message).toBe("__test_exit__");
    expect(exit.code()).toBe(0);
    const out = stdio.stdout.read();
    expect(out).toContain("dump-test");
    expect(out).toContain("members (2)");
    expect(out).toMatch(/✓ blake3-512:[0-9a-f]{128}/);
    expect(out).not.toContain("✗");
  });

  it("--json emits machine-readable output with ok=true on valid file", async () => {
    const proofPath = await buildSampleProof();
    stdio.stdout.reset();
    stdio.stderr.reset();

    let caught: Error | null = null;
    try {
      await runDump([proofPath, "--json"]);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught?.message).toBe("__test_exit__");
    expect(exit.code()).toBe(0);

    const result = JSON.parse(stdio.stdout.read());
    expect(result.ok).toBe(true);
    expect(result.errors).toEqual([]);
    expect(result.catalog.kind).toBe("catalog");
    expect(result.catalog.name).toBe("dump-test");
    expect(result.catalog.memberCount).toBe(2);
    expect(result.members).toHaveLength(2);
    expect(result.members.every((m: { cidMatch: boolean }) => m.cidMatch)).toBe(true);
  });

  it("rejects on tampered file (rule 1: filename CID mismatch), exits 1", async () => {
    const proofPath = await buildSampleProof();
    // Flip one byte in the file body (after the filename hash was set).
    const bytes = readFileSync(proofPath);
    bytes[Math.floor(bytes.length / 2)]! ^= 0xff;
    fsWriteFileSync(proofPath, bytes);
    stdio.stdout.reset();
    stdio.stderr.reset();

    let caught: Error | null = null;
    try {
      await runDump([proofPath, "--json"]);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught?.message).toBe("__test_exit__");
    expect(exit.code()).toBe(1);

    const result = JSON.parse(stdio.stdout.read());
    expect(result.ok).toBe(false);
    expect(result.errors.some((e: string) => /rule 1|trust root|decode/.test(e))).toBe(true);
  });
});
