/**
 * publish-principle action tests. Side-effecting; writes a JSON file
 * to a scratch project root's `.provekit/principles/` directory and
 * confirms the resulting file matches the LibraryPrinciple shape that
 * recognize.ts loads.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, readFileSync, existsSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import type { LibraryPrinciple } from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryActionRegistry } from "../registry.js";
import {
  makePublishPrincipleAction,
  PUBLISH_PRINCIPLE_CAPABILITY,
} from "./publishPrinciple.js";
import type { ShapeCluster } from "./clusterByShape.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeProjectAndDb() {
  const projectRoot = mkdtempSync(join(tmpdir(), "publish-principle-"));
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  const db = openDb(join(projectRoot, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return { projectRoot, db };
}

const wf = { name: "test-wf", cid: "wf-publish-test-v1" };

const sampleCluster: ShapeCluster = {
  fingerprint: "arithmetic|Int,Int|1",
  members: ["aa", "bb"],
  shape: { kind: "arithmetic", bindingSorts: ["Int", "Int"], declarationCount: 1 },
};

describe("publish-principle Action", () => {
  it("writes a LibraryPrinciple JSON to .provekit/principles/<name>.json", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makePublishPrincipleAction();
    const runner = new WorkflowRunner(db, wf);

    const { resource } = await runner.runAction(action, {
      projectRoot,
      principleName: "div-by-zero",
      bugClassId: "div-by-zero",
      cluster: sampleCluster,
    });

    expect(resource.outcome).toBe("created");
    expect(resource.principleId).toBe("div-by-zero");
    expect(resource.jsonPath).toBe(
      join(projectRoot, ".provekit", "principles", "div-by-zero.json"),
    );
    expect(existsSync(resource.jsonPath!)).toBe(true);

    const written = JSON.parse(readFileSync(resource.jsonPath!, "utf-8")) as LibraryPrinciple;
    expect(written.id).toBe("div-by-zero");
    expect(written.bug_class_id).toBe("div-by-zero");
    expect(written.name).toBe("div-by-zero");
    expect(written.confidence).toBe("medium");
    const provenance = Array.isArray(written.provenance)
      ? written.provenance
      : written.provenance
        ? [written.provenance]
        : [];
    expect(provenance).toHaveLength(1);
    expect(provenance[0].source).toBe("harvest");
    expect(provenance[0].bugId).toBe(sampleCluster.fingerprint);
  });

  it("merges into an existing principle file, preserving custom fields", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const principlesDir = join(projectRoot, ".provekit", "principles");
    mkdirSync(principlesDir, { recursive: true });
    const path = join(principlesDir, "div-by-zero.json");
    const existing: LibraryPrinciple = {
      id: "div-by-zero",
      bug_class_id: "div-by-zero",
      name: "div-by-zero",
      smt2Template: "(assert (= b 0))",
      confidence: "high",
      provenance: [
        {
          source: "seed",
          timestamp: "2025-01-01T00:00:00.000Z",
        },
      ],
    };
    writeFileSync(path, JSON.stringify(existing, null, 2));

    const action = makePublishPrincipleAction();
    const runner = new WorkflowRunner(db, wf);

    const { resource } = await runner.runAction(action, {
      projectRoot,
      principleName: "div-by-zero",
      bugClassId: "div-by-zero",
      cluster: sampleCluster,
      confidence: "medium",
    });

    expect(resource.outcome).toBe("merged");
    const merged = JSON.parse(readFileSync(path, "utf-8")) as LibraryPrinciple;
    expect(merged.smt2Template).toBe("(assert (= b 0))");
    expect(merged.confidence).toBe("medium"); // new entry's confidence wins
    const provenance = Array.isArray(merged.provenance)
      ? merged.provenance
      : merged.provenance
        ? [merged.provenance]
        : [];
    expect(provenance).toHaveLength(2);
    expect(provenance[0].source).toBe("seed");
    expect(provenance[1].source).toBe("harvest");
  });

  it("no-ops when cluster is null", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makePublishPrincipleAction();
    const runner = new WorkflowRunner(db, wf);

    const { resource, auditCid } = await runner.runAction(action, {
      projectRoot,
      principleName: "empty",
      bugClassId: "empty",
      cluster: null,
    });

    expect(resource.outcome).toBe("skipped");
    expect(resource.jsonPath).toBeNull();
    expect(auditCid).toBeTruthy(); // audit memento still recorded
    expect(
      existsSync(join(projectRoot, ".provekit", "principles", "empty.json")),
    ).toBe(false);
  });

  it("each invocation produces a fresh audit memento (no cache reuse)", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makePublishPrincipleAction();
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runAction(action, {
      projectRoot,
      principleName: "p",
      bugClassId: "p",
      cluster: sampleCluster,
    });
    const b = await runner.runAction(action, {
      projectRoot,
      principleName: "p",
      bugClassId: "p",
      cluster: sampleCluster,
    });

    expect(b.auditCid).not.toBe(a.auditCid);
    // First call created, second merged.
    expect(a.resource.outcome).toBe("created");
    expect(b.resource.outcome).toBe("merged");
  });

  it("dispatches via the registry as capability 'publish-principle'", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makePublishPrincipleAction();
    const registry = new InMemoryActionRegistry();
    registry.register(PUBLISH_PRINCIPLE_CAPABILITY, action);

    const resolved = registry.resolve(PUBLISH_PRINCIPLE_CAPABILITY);
    expect(resolved).not.toBeNull();
    const runner = new WorkflowRunner(db, wf);
    const { resource } = await runner.runAction(resolved!, {
      projectRoot,
      principleName: "x",
      bugClassId: "x",
      cluster: sampleCluster,
    } as unknown as Parameters<typeof runner.runAction>[1]);
    expect((resource as { outcome: string }).outcome).toBe("created");
  });
});
