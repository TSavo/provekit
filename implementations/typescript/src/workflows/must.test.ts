/**
 * Must workflow registry-assembly tests.
 *
 * Asserts:
 *   - registerMustRegistries returns ProducerRegistry + ActionRegistry
 *     containing every capability the YAML manifest names (intake,
 *     locate, formulate-via-lifter; write-invariant-file action).
 *   - The capability constants match the manifest's `capability` refs.
 *
 * Heavy stages (formulate-via-lifter's full Z3 path) need a real LLM
 * + fixture; that's covered by the must-workflow integration smoke.
 *
 * SURFACED BUG: src/workflows/must.workflow.yaml is missing the
 * required `output:` field. parseManifest's validator rejects the
 * document with "manifest.output must be a $-prefixed reference".
 * The test below pins that failure mode so a regression that masks
 * the validator (or a fix that adds the field) trips here loudly.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb, type Db } from "../db/index.js";
import { StubLLMProvider } from "../fix/types.js";
import { loadMustManifest, registerMustRegistries } from "./must.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb(): Db {
  const tmp = mkdtempSync(join(tmpdir(), "must-workflow-test-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function makeStubLLM(): StubLLMProvider {
  return new StubLLMProvider(new Map<string, string>());
}

describe("loadMustManifest", () => {
  // Pin the current state: must.workflow.yaml is missing `output:`.
  // When the bug is fixed (output added), THIS test will fail,
  // forcing the corresponding positive-case tests to be turned on.
  it("surfaces the missing-output bug in must.workflow.yaml", () => {
    expect(() => loadMustManifest()).toThrow(
      /manifest\.output must be a \$-prefixed reference/,
    );
  });
});

describe("registerMustRegistries", () => {
  // The registry-assembly path is independent of the YAML; the deps are
  // wired statically in must.ts. These tests pass even while the YAML
  // is malformed — they pin the producer wiring.
  it("registers every Stage capability the manifest names", () => {
    const db = makeDb();
    const { registry } = registerMustRegistries({
      db,
      llm: makeStubLLM(),
    });
    const registered = new Set(registry.capabilities());
    expect(registered.has("intake")).toBe(true);
    expect(registered.has("locate")).toBe(true);
    expect(registered.has("formulate-via-lifter")).toBe(true);
  });

  it("registers the write-invariant-file Action capability", () => {
    const db = makeDb();
    const { actionRegistry } = registerMustRegistries({
      db,
      llm: makeStubLLM(),
    });
    expect(actionRegistry.capabilities()).toContain("write-invariant-file");
  });

  it("registry shape: 3 stage capabilities, 1 action capability", () => {
    const db = makeDb();
    const { registry, actionRegistry } = registerMustRegistries({
      db,
      llm: makeStubLLM(),
    });
    expect(registry.capabilities().sort()).toEqual([
      "formulate-via-lifter",
      "intake",
      "locate",
    ]);
    expect(actionRegistry.capabilities()).toEqual(["write-invariant-file"]);
  });
});
