/**
 * RenderProofChain stage tests. Covers:
 * 1. Local rendering of an existing memento + its inputs.
 * 2. Unresolved CIDs are surfaced, not silently chased — this is the
 *    scope-discipline check (no external walking).
 * 3. Cache: identical input + identical DB → same propertyHash + cache hit.
 * 4. Capability dispatch.
 * 5. Missing start CID renders gracefully.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { writeMemento } from "../../fix/runtime/mementoStore.js";
import { WorkflowRunner } from "../runner.js";
import {
  makeRenderProofChainStage,
  RENDER_PROOF_CHAIN_CAPABILITY,
} from "./renderProofChain.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "render-proof-chain-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-render-proof-chain-test-v1" };

describe("renderProofChain Stage", () => {
  it("renders a chain of locally-present mementos", async () => {
    const db = makeDb();

    // Plant two-deep DAG: leaf <- mid <- root.
    const leaf = writeMemento(db, {
      bindingHash: "bh-leaf",
      propertyHash: "ph-leaf",
      verdict: "holds",
      witness: "leaf-witness",
      producedBy: "test-producer-v1",
      inputCids: [],
    });
    const mid = writeMemento(db, {
      bindingHash: "bh-mid",
      propertyHash: "ph-mid",
      verdict: "holds",
      witness: "mid-witness",
      producedBy: "test-producer-v1",
      inputCids: [leaf.cid!],
    });
    const root = writeMemento(db, {
      bindingHash: "bh-root",
      propertyHash: "ph-root",
      verdict: "holds",
      witness: "root-witness",
      producedBy: "test-producer-v1",
      inputCids: [mid.cid!],
    });

    const stage = makeRenderProofChainStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, { startCid: root.cid! });

    expect(result.output.startResolved).toBe(true);
    expect(result.output.mementos).toHaveLength(3);
    expect(result.output.mementos[0].cid).toBe(root.cid);
    expect(result.output.mementos[0].depth).toBe(0);
    expect(result.output.mementos[1].cid).toBe(mid.cid);
    expect(result.output.mementos[1].depth).toBe(1);
    expect(result.output.mementos[2].cid).toBe(leaf.cid);
    expect(result.output.mementos[2].depth).toBe(2);
    expect(result.output.unresolvedInputCids).toEqual([]);
    expect(result.output.text).toContain(root.cid!);
    expect(result.output.text).toContain(mid.cid!);
    expect(result.output.text).toContain(leaf.cid!);
  });

  it("surfaces unresolved inputCids without walking externally", async () => {
    const db = makeDb();
    // Plant a memento that references a CID that is NOT in the local store.
    const m = writeMemento(db, {
      bindingHash: "bh-only",
      propertyHash: "ph-only",
      verdict: "holds",
      witness: "x",
      producedBy: "test-producer-v1",
      inputCids: ["cid-of-something-far-away"],
    });

    const stage = makeRenderProofChainStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, { startCid: m.cid! });

    expect(result.output.startResolved).toBe(true);
    expect(result.output.mementos).toHaveLength(1);
    expect(result.output.unresolvedInputCids).toEqual([
      "cid-of-something-far-away",
    ]);
    expect(result.output.text).toContain("not in local store");
    expect(result.output.text).toContain("cid-of-something-far-away");
  });

  it("renders gracefully when the start CID is not in the local store", async () => {
    const db = makeDb();
    const stage = makeRenderProofChainStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      startCid: "no-such-cid-anywhere",
    });

    expect(result.output.startResolved).toBe(false);
    expect(result.output.mementos).toEqual([]);
    expect(result.output.text).toContain("not in local store");
  });

  it("caches identical input — second run is a hit", async () => {
    const db = makeDb();
    const m = writeMemento(db, {
      bindingHash: "bh-cache",
      propertyHash: "ph-cache",
      verdict: "holds",
      witness: "x",
      producedBy: "test-producer-v1",
    });
    const stage = makeRenderProofChainStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { startCid: m.cid! });
    const b = await runner.runStage(stage, { startCid: m.cid! });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("respects maxDepth", async () => {
    const db = makeDb();
    const leaf = writeMemento(db, {
      bindingHash: "bh-leaf",
      propertyHash: "ph-leaf",
      verdict: "holds",
      witness: "w",
      producedBy: "test-producer-v1",
    });
    const mid = writeMemento(db, {
      bindingHash: "bh-mid",
      propertyHash: "ph-mid",
      verdict: "holds",
      witness: "w",
      producedBy: "test-producer-v1",
      inputCids: [leaf.cid!],
    });
    const root = writeMemento(db, {
      bindingHash: "bh-root",
      propertyHash: "ph-root",
      verdict: "holds",
      witness: "w",
      producedBy: "test-producer-v1",
      inputCids: [mid.cid!],
    });

    const stage = makeRenderProofChainStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      startCid: root.cid!,
      maxDepth: 1,
    });

    // Depth 1 reaches root and mid but not leaf.
    expect(result.output.mementos.map((m) => m.cid)).toEqual([
      root.cid,
      mid.cid,
    ]);
    // leaf.cid is referenced by mid but was beyond maxDepth, so it
    // appears as unresolved (the renderer doesn't chase past the cap).
    expect(result.output.unresolvedInputCids).toEqual([leaf.cid!]);
  });

  it("capability constant matches the conventional name", () => {
    expect(RENDER_PROOF_CHAIN_CAPABILITY).toBe("render-proof-chain");
  });
});
