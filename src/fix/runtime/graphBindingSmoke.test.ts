/**
 * Graph-binding kind: empirical smoke test.
 *
 * Validates that `resolveBindings` correctly walks the import graph
 * from a binding's root, applies the predicate, and reports
 * holds/decayed appropriately. This is the load-bearing shape for
 * ProvekIt's product constraints #1 (no LLM in verify path), #9 (one
 * pipeline fork), #13 (no LLM in pre-commit) — properties that don't
 * reduce to a single source span.
 *
 * No LLM cost. No corpus pollution. Pure mechanical proof of the new
 * binding kind.
 *
 * Test plan:
 *   1. Create a tmp project with a known import graph (a.ts → b.ts → c.ts).
 *   2. Hand-craft a graph-binding StoredInvariant with predicate
 *      "no_match" against pattern "src/forbidden/**".
 *   3. Run verifyAll → expect "holds" (no file reaches forbidden/).
 *   4. Add an import from c.ts to src/forbidden/leak.ts.
 *   5. Run verifyAll → expect "decayed" with predicate-violation reason.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, appendFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  writeInvariant,
  type StoredInvariant,
  type GraphBinding,
} from "./invariantStore.js";
import { verifyAll } from "./verify.js";

let tmpRoot: string;

beforeEach(() => {
  tmpRoot = mkdtempSync(join(tmpdir(), "graph-bind-smoke-"));
  mkdirSync(join(tmpRoot, "src"), { recursive: true });
});

function makeGraphInvariant(binding: GraphBinding): StoredInvariant {
  return {
    id: "test-graph-1",
    createdAt: new Date().toISOString(),
    originatingBug: "synthetic graph-binding smoke test",
    smt: {
      kind: "other",
      declarations: [],
      assertion: "(assert true)",
    },
    bindings: [binding],
    callsite: {
      filePath: binding.root.filePath,
      function: null,
      startLine: 1,
      endLine: 1,
    },
    scope: "callsite",
    regressionTest: null,
    patchSha: null,
    retired: null,
  };
}

describe("graph-binding kind (step 6 empirical)", () => {
  it("holds when imports_transitively reaches no forbidden node", async () => {
    // Build: src/a.ts → src/b.ts → src/c.ts; nothing reaches src/forbidden/
    writeFileSync(
      join(tmpRoot, "src/a.ts"),
      `import { b } from "./b";\nexport const a = b;\n`,
    );
    writeFileSync(
      join(tmpRoot, "src/b.ts"),
      `import { c } from "./c";\nexport const b = c;\n`,
    );
    writeFileSync(join(tmpRoot, "src/c.ts"), `export const c = 1;\n`);

    const inv = makeGraphInvariant({
      type: "graph",
      smt_constant: "no_llm_reach",
      relation: "imports_transitively",
      root: { filePath: "src/a.ts" },
      predicate: "no_match",
      predicateArg: "src/forbidden/**",
    });
    writeInvariant(tmpRoot, inv);

    const report = await verifyAll(tmpRoot);
    expect(report.verdicts).toHaveLength(1);
    const v = report.verdicts[0];
    expect(v.status).toBe("holds");
    expect(v.bindings[0].status).toBe("resolved");
  });

  it("decays when an import is added that reaches the forbidden region", async () => {
    writeFileSync(
      join(tmpRoot, "src/a.ts"),
      `import { b } from "./b";\nexport const a = b;\n`,
    );
    writeFileSync(
      join(tmpRoot, "src/b.ts"),
      `import { c } from "./c";\nexport const b = c;\n`,
    );
    writeFileSync(join(tmpRoot, "src/c.ts"), `export const c = 1;\n`);

    const inv = makeGraphInvariant({
      type: "graph",
      smt_constant: "no_llm_reach",
      relation: "imports_transitively",
      root: { filePath: "src/a.ts" },
      predicate: "no_match",
      predicateArg: "src/forbidden/**",
    });
    writeInvariant(tmpRoot, inv);

    // First verify: holds
    const before = await verifyAll(tmpRoot);
    expect(before.verdicts[0].status).toBe("holds");

    // Introduce a forbidden import. c.ts now reaches src/forbidden/leak.ts.
    mkdirSync(join(tmpRoot, "src/forbidden"), { recursive: true });
    writeFileSync(
      join(tmpRoot, "src/forbidden/leak.ts"),
      `export const leak = "secret";\n`,
    );
    appendFileSync(
      join(tmpRoot, "src/c.ts"),
      `import { leak } from "./forbidden/leak";\nconsole.log(leak);\n`,
    );

    const after = await verifyAll(tmpRoot);
    expect(after.verdicts[0].status).toBe("decayed");
    const b = after.verdicts[0].bindings[0];
    expect(b.status).toBe("decayed");
    expect(b.reason).toMatch(/graph predicate violated/);
    expect(b.reason).toMatch(/src\/forbidden\/leak\.ts/);
  });

  it("decays:deleted when the graph root file is removed", async () => {
    writeFileSync(join(tmpRoot, "src/a.ts"), `export const a = 1;\n`);
    const inv = makeGraphInvariant({
      type: "graph",
      smt_constant: "x",
      relation: "imports_transitively",
      root: { filePath: "src/a.ts" },
      predicate: "no_match",
      predicateArg: "src/anywhere/**",
    });
    writeInvariant(tmpRoot, inv);

    const { unlinkSync } = await import("fs");
    unlinkSync(join(tmpRoot, "src/a.ts"));

    const report = await verifyAll(tmpRoot);
    expect(report.verdicts[0].status).toBe("decayed");
    expect(report.verdicts[0].bindings[0].decayKind).toBe("deleted");
    expect(report.verdicts[0].bindings[0].reason).toMatch(/graph root file not found/);
  });
});
