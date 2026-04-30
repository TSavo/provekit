/**
 * Mint workflow integration test. Verifies:
 * 1. Manifest loads cleanly with two Stages and one Action.
 * 2. End-to-end runManifest mints a property memento, writes it to disk.
 * 3. The Action runs AFTER the mint Stage; cid in file matches Stage cid.
 * 4. Capability declarations match what the manifest references.
 *
 * Tests use a deterministic ed25519 keypair (generated from a fixed seed)
 * so the produced memento bytes are reproducible across runs — this is
 * what makes the workflow cache hits cleanly.
 */

import { describe, it, expect } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  existsSync,
  writeFileSync,
} from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import { generateKeypair } from "../producerKeys/index.js";
import {
  loadMintManifest,
  registerMintRegistries,
  MINT_STAGE_CAPABILITIES,
  MINT_ACTION_CAPABILITIES,
  type MintWorkflowInput,
} from "./mint.js";
import type { MintMementoOutput } from "../workflow/producers/mintMemento.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "mint-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function fixedKeyPem(): string {
  // Deterministic seed → deterministic ed25519 key.
  const seed = Buffer.from(
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "hex",
  );
  const { privateKey } = generateKeypair({ seed });
  return privateKey.export({ type: "pkcs8", format: "pem" }).toString();
}

describe("mint workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadMintManifest();
    expect(manifest.name).toBe("mint");
    expect(manifest.nodes).toHaveLength(2);
    expect(manifest.nodes.map((n) => n.capability)).toEqual([
      "load-mint-spec",
      "mint-memento",
    ]);
    expect(manifest.actions).toHaveLength(1);
    expect(manifest.actions![0].action).toBe("write-memento-file");
    expect(manifest.actions![0].runAfter).toBe("$node.mint");
  });

  it("declares a CLI block including kind/spec/key/out", () => {
    const manifest = loadMintManifest();
    expect(manifest.cli).toBeDefined();
    const argNames = manifest.cli!.args!.map((a) => a.name);
    expect(argNames).toEqual(["kind", "spec", "key", "out"]);
  });

  it("mints a property memento and writes it to disk", async () => {
    const db = makeDb();
    const tmp = mkdtempSync(join(tmpdir(), "mint-out-"));
    const outPath = join(tmp, "memento.json");

    const manifest = loadMintManifest();
    const { registry, actionRegistry } = registerMintRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: MintWorkflowInput = {
      kind: "property",
      privateKeyPem: fixedKeyPem(),
      outPath,
      spec: {
        bindingHash: "abcdef0123456789",
        propertyHash: "fedcba9876543210",
        verdict: "holds",
        producedBy: "test@v1",
        producedAt: "2026-01-01T00:00:00.000Z",
        inputCids: [],
        rawWitness: '{"hello":"world"}',
      },
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);
    const out = result.output as MintMementoOutput;

    // Stage produced an envelope with the expected fields.
    expect(out.envelope.bindingHash).toBe("abcdef0123456789");
    expect(out.envelope.propertyHash).toBe("fedcba9876543210");
    expect(out.envelope.verdict).toBe("holds");
    expect(typeof out.envelope.cid).toBe("string");
    expect(out.publicKeyFingerprint).toMatch(/^[0-9a-f]{64}$/);

    // The Action wrote the envelope to disk; the file's CID matches.
    expect(existsSync(outPath)).toBe(true);
    const written = JSON.parse(readFileSync(outPath, "utf-8"));
    expect(written.cid).toBe(out.envelope.cid);
    expect(written.bindingHash).toBe(out.envelope.bindingHash);
  });

  it("supports loading the spec from a JSON file path", async () => {
    const db = makeDb();
    const tmp = mkdtempSync(join(tmpdir(), "mint-spec-"));
    const specPath = join(tmp, "spec.json");
    const outPath = join(tmp, "memento.json");
    writeFileSync(
      specPath,
      JSON.stringify({
        bindingHash: "0000000011111111",
        propertyHash: "2222222233333333",
        verdict: "holds",
        producedBy: "spec-loader@v1",
        producedAt: "2026-01-01T00:00:00.000Z",
        inputCids: [],
        rawWitness: '{}',
      }),
      "utf-8",
    );

    const manifest = loadMintManifest();
    const { registry, actionRegistry } = registerMintRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: MintWorkflowInput = {
      kind: "property",
      privateKeyPem: fixedKeyPem(),
      outPath,
      specPath,
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);
    const out = result.output as MintMementoOutput;
    expect(out.envelope.bindingHash).toBe("0000000011111111");
    expect(out.envelope.propertyHash).toBe("2222222233333333");
  });

  it("declares the expected capabilities", () => {
    expect(MINT_STAGE_CAPABILITIES).toEqual(["load-mint-spec", "mint-memento"]);
    expect(MINT_ACTION_CAPABILITIES).toEqual(["write-memento-file"]);
  });
});
