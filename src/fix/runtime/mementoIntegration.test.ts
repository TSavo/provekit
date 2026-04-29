/**
 * Step 2 instrumentation smoke test: verifyAll writes mementos to the
 * memento store as it computes verdicts.
 *
 * Confirms the durable foundation hooks up to the verifier without
 * changing verifier behavior. Step 3 (cache-lookup short-circuit) is
 * a future commit; this test only validates the WRITE side.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { createHash } from "crypto";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { writeInvariant, type StoredInvariant } from "./invariantStore.js";
import { verifyAll } from "./verify.js";
import {
  findMemento,
  computeBindingHash,
  computePropertyHash,
  stats,
} from "./mementoStore.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

let projectRoot: string;
let mementoDb: ReturnType<typeof openDb>;

beforeEach(() => {
  projectRoot = mkdtempSync(join(tmpdir(), "memento-integration-"));
  mkdirSync(join(projectRoot, "src"), { recursive: true });
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  mementoDb = openDb(join(projectRoot, ".provekit", "memento.db"));
  migrate(mementoDb, { migrationsFolder: DRIZZLE_FOLDER });
});

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function makeLocalInvariant(
  filePath: string,
  startLine: number,
  endLine: number,
  bytesAtMintTime: string,
): StoredInvariant {
  return {
    id: "test-inv-1",
    createdAt: new Date().toISOString(),
    originatingBug: "memento integration smoke",
    smt: {
      kind: "arithmetic",
      declarations: ["(declare-const k Int)"],
      assertion: "(assert (> k 0))",
    },
    bindings: [
      {
        type: "local",
        smt_constant: "k",
        source_expr: "k",
        sort: "Int",
        node: {
          filePath,
          nodeHash: hash16(bytesAtMintTime),
          startLine,
          endLine,
        },
      },
    ],
    callsite: { filePath, function: null, startLine, endLine },
    scope: "callsite",
    regressionTest: null,
    patchSha: null,
    retired: null,
  };
}

describe("verifyAll → memento store integration (step 2)", () => {
  it("writes a memento per invariant when mementoDb is provided", async () => {
    const file = "src/example.ts";
    const content = "function f(k: number) {\n  return k > 0;\n}\n";
    writeFileSync(join(projectRoot, file), content);

    const lines = content.split("\n");
    const span = lines.slice(0, 3).join("\n");
    const inv = makeLocalInvariant(file, 1, 3, span);
    writeInvariant(projectRoot, inv);

    const before = stats(mementoDb);
    expect(before.totalRows).toBe(0);

    const report = await verifyAll(projectRoot, { mementoDb });
    expect(report.verdicts).toHaveLength(1);
    expect(report.verdicts[0].status).toBe("holds");

    const after = stats(mementoDb);
    expect(after.totalRows).toBe(1);
    expect(after.byVerdict.holds).toBe(1);
    expect(after.byProducer["path-checker@1.0"]).toBe(1);
  });

  it("the memento has the expected binding_hash and property_hash", async () => {
    const file = "src/example.ts";
    const content = "function f(k: number) {\n  return k > 0;\n}\n";
    writeFileSync(join(projectRoot, file), content);

    const span = content.split("\n").slice(0, 3).join("\n");
    const inv = makeLocalInvariant(file, 1, 3, span);
    writeInvariant(projectRoot, inv);

    await verifyAll(projectRoot, { mementoDb });

    const expectedBindingHash = computeBindingHash(inv);
    const expectedPropertyHash = computePropertyHash(inv);

    const memento = findMemento(mementoDb, {
      bindingHash: expectedBindingHash,
      propertyHash: expectedPropertyHash,
    });
    expect(memento).not.toBeNull();
    expect(memento?.verdict).toBe("holds");
    expect(memento?.producedBy).toBe("path-checker@1.0");
    // Witness JSON includes the pathCheck status from the verdict.
    expect(memento?.witness).toContain("pathCheck");
  });

  it("writes a 'decayed' memento when the bound source mutates", async () => {
    const file = "src/example.ts";
    const original = "function f(k: number) {\n  return k > 0;\n}\n";
    writeFileSync(join(projectRoot, file), original);

    const span = original.split("\n").slice(0, 3).join("\n");
    const inv = makeLocalInvariant(file, 1, 3, span);
    writeInvariant(projectRoot, inv);

    // Mutate the bound span so the binding decays.
    const mutated = "function f(k: number) {\n  return k >= 0;\n}\n";
    writeFileSync(join(projectRoot, file), mutated);

    await verifyAll(projectRoot, { mementoDb });

    const s = stats(mementoDb);
    expect(s.byVerdict.decayed).toBe(1);
    expect(s.byVerdict.holds).toBe(0);
  });

  it("does NOT write mementos when mementoDb is omitted", async () => {
    const file = "src/example.ts";
    const content = "function f(k: number) { return k > 0; }\n";
    writeFileSync(join(projectRoot, file), content);

    const inv = makeLocalInvariant(file, 1, 1, content.split("\n")[0]);
    writeInvariant(projectRoot, inv);

    await verifyAll(projectRoot); // no mementoDb option

    expect(stats(mementoDb).totalRows).toBe(0);
  });

  it("cache hit: second verifyAll uses the cached memento (step 3)", async () => {
    const file = "src/example.ts";
    const content = "function f(k: number) {\n  return k > 0;\n}\n";
    writeFileSync(join(projectRoot, file), content);

    const span = content.split("\n").slice(0, 3).join("\n");
    const inv = makeLocalInvariant(file, 1, 3, span);
    writeInvariant(projectRoot, inv);

    // First run: cache miss, path-checker runs, memento gets written.
    const first = await verifyAll(projectRoot, {
      mementoDb,
      mementoProducer: "first-producer@1.0",
    });
    expect(first.verdicts[0].status).toBe("holds");
    // First-run note (if any) is the substrate-not-built note; never
    // a cache-hit note (since the table was empty going in).
    expect(first.verdicts[0].note ?? "").not.toMatch(/cached verdict from/);

    // Second run: cache HIT. The verdict comes from the memento, not
    // the path-checker. The note documents the cached origin.
    const second = await verifyAll(projectRoot, {
      mementoDb,
      mementoProducer: "second-producer@1.0",
    });
    expect(second.verdicts[0].status).toBe("holds");
    expect(second.verdicts[0].note).toMatch(/cached verdict from first-producer@1\.0/);

    // Producer table should still show only the first run's producer
    // (cache hit means second run did NOT write).
    const s = stats(mementoDb);
    expect(s.byProducer["first-producer@1.0"]).toBe(1);
    // Cache hit DOES re-write under the second producer (step 2 always
    // writes); cross-validation works because both rows exist for the
    // same (binding_hash, property_hash, ...) tuple.
    expect(s.byProducer["second-producer@1.0"]).toBe(1);
  });

  it("respects a custom mementoProducer name", async () => {
    const file = "src/example.ts";
    const content = "const x = 1;\n";
    writeFileSync(join(projectRoot, file), content);

    const inv = makeLocalInvariant(file, 1, 1, content.split("\n").slice(0, 1).join("\n"));
    writeInvariant(projectRoot, inv);

    await verifyAll(projectRoot, {
      mementoDb,
      mementoProducer: "z3-symbolic@4.13",
    });

    const s = stats(mementoDb);
    expect(s.byProducer["z3-symbolic@4.13"]).toBe(1);
    expect(s.byProducer["path-checker@1.0"]).toBeUndefined();
  });
});
