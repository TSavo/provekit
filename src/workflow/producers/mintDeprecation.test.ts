/**
 * MintDeprecation stage tests. Cache, capability dispatch, and the
 * "no silent retires" guard.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { WorkflowRunner } from "../runner.js";
import {
  makeMintDeprecationStage,
  MINT_DEPRECATION_CAPABILITY,
} from "./mintDeprecation.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "mint-deprecation-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-mint-deprecation-test-v1" };

describe("mintDeprecation Stage", () => {
  it("produces a decayed verdict record with the supplied reason", async () => {
    const db = makeDb();
    const stage = makeMintDeprecationStage();
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      retiredPropertyHash: "abcdef0123456789",
      propertyName: "parseIntCanReturnZero",
      reason: "superseded by parseIntCanReturnZeroOrNaN",
      filePath: "/abs/path/parseInt.invariant.ts",
    });

    expect(result.output.verdict).toBe("decayed");
    expect(result.output.retiredPropertyHash).toBe("abcdef0123456789");
    expect(result.output.propertyName).toBe("parseIntCanReturnZero");
    expect(result.output.reason).toBe("superseded by parseIntCanReturnZeroOrNaN");
    expect(result.output.filePath).toBe("/abs/path/parseInt.invariant.ts");
    expect(result.output.text).toContain("Retired invariant");
    expect(result.output.text).toContain("superseded");
    expect(result.output.text).toContain("decayed");
  });

  it("rejects empty or whitespace-only reasons", async () => {
    const db = makeDb();
    const stage = makeMintDeprecationStage();
    const runner = new WorkflowRunner(db, wf);

    await expect(
      runner.runStage(stage, {
        retiredPropertyHash: "abcdef",
        propertyName: "x",
        reason: "",
      }),
    ).rejects.toThrow(/non-empty reason/);

    await expect(
      runner.runStage(stage, {
        retiredPropertyHash: "abcdef",
        propertyName: "x",
        reason: "   ",
      }),
    ).rejects.toThrow(/non-empty reason/);
  });

  it("caches identical input — second run is a hit", async () => {
    const db = makeDb();
    const stage = makeMintDeprecationStage();
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      retiredPropertyHash: "ph-cache",
      propertyName: "x",
      reason: "obsolete",
    });
    const b = await runner.runStage(stage, {
      retiredPropertyHash: "ph-cache",
      propertyName: "x",
      reason: "obsolete",
    });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("renders a sensible text without filePath", async () => {
    const db = makeDb();
    const stage = makeMintDeprecationStage();
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      retiredPropertyHash: "ph",
      propertyName: "foo",
      reason: "no longer applicable",
    });

    expect(result.output.filePath).toBeNull();
    expect(result.output.text).not.toContain("file:");
  });

  it("capability constant matches the conventional name", () => {
    expect(MINT_DEPRECATION_CAPABILITY).toBe("mint-deprecation");
  });
});
