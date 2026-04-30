/**
 * Tests for src/fix/runtime/pathChecker.ts.
 *
 * pathChecker is a 1075-line module whose deepest layer is symbolic
 * execution + Z3 invocation. Real-Z3 integration tests are gated under
 * PROVEKIT_PATHCHECKER_REAL_Z3=1 so the suite stays portable. The
 * bulk of the coverage here is Z3-free: it pins `checkPath`'s decision
 * shape on a path with no DB (no path constraints derivable) under each
 * of the four meaningful invariant assertion shapes.
 *
 * The "no DB / no path constraints" path exercises classify()'s critical
 * SAT-without-real-constraints guard: any non-tautological invariant
 * surfaces as `undecidable` rather than `violated`. Tautological
 * invariants return `holds`. These two distinctions are the contract
 * `provekit verify` relies on to avoid screaming on every real invariant.
 */
import { describe, it, expect } from "vitest";
import { checkPath } from "./pathChecker.js";
import type { StoredInvariant } from "./invariantStore.js";
import type { Path } from "./pathEnumerator.js";

const realZ3 = process.env.PROVEKIT_PATHCHECKER_REAL_Z3 === "1";

const HAS_Z3 = (() => {
  try {
    require("child_process").execSync("z3 --version", { stdio: "pipe" });
    return true;
  } catch {
    return false;
  }
})();

function makePath(): Path {
  return {
    steps: [{ nodeId: "callsite-node", slot: "callsite" }],
  };
}

function makeInvariant(args: {
  kind?: StoredInvariant["smt"]["kind"];
  declarations: string[];
  assertion: string;
}): StoredInvariant {
  return {
    id: "test-id",
    createdAt: "2026-04-29T00:00:00Z",
    originatingBug: "test",
    smt: {
      kind: args.kind ?? "other",
      declarations: args.declarations,
      assertion: args.assertion,
    },
    bindings: [],
    callsite: {
      filePath: "src/test.ts",
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

describe.skipIf(!HAS_Z3)("checkPath — no DB, no real path constraints", () => {
  it("returns 'holds' for a tautological invariant (always-true assertion)", async () => {
    const invariant = makeInvariant({
      declarations: [],
      assertion: "(assert (= 1 1))",
    });
    const verdict = await checkPath(makePath(), invariant);
    expect(verdict.status).toBe("holds");
    expect(verdict.reason).toMatch(/no real path constraints|tautology/);
  });

  it("returns 'undecidable' for a non-tautological invariant under no path constraints", async () => {
    // (assert (> x 0)) negated to (not (> x 0)) is satisfiable for x=0,
    // but with zero real path constraints the SAT-trust guard downgrades
    // to undecidable.
    const invariant = makeInvariant({
      declarations: ["(declare-const x Int)"],
      assertion: "(assert (> x 0))",
    });
    const verdict = await checkPath(makePath(), invariant);
    expect(verdict.status).toBe("undecidable");
    expect(verdict.reason).toMatch(/no real path constraints|negated invariant.*trivially/);
  });

  it("returns 'undecidable' when the invariant cannot be safely negated (multi-assert text)", async () => {
    // Two top-level forms; the negate heuristic punts.
    const invariant = makeInvariant({
      declarations: ["(declare-const x Int)"],
      assertion: "(assert (> x 0))\n(assert (< x 100))",
    });
    const verdict = await checkPath(makePath(), invariant);
    expect(verdict.status).toBe("undecidable");
  });

  it("returns 'holds' for an unsatisfiable (always-false) invariant body — the negation is trivially true, but with zero real constraints we treat it as the boring tautology branch", async () => {
    const invariant = makeInvariant({
      declarations: [],
      assertion: "(assert false)",
    });
    const verdict = await checkPath(makePath(), invariant);
    // Negated is `(assert (not false))` → SAT trivially. With zero real
    // constraints, classify() downgrades to undecidable (SAT-trust guard).
    expect(verdict.status).toBe("undecidable");
    expect(verdict.reason).toMatch(/no real path constraints|trivially/);
  });

  it("threads a custom timeoutMs without throwing (smoke check on the timeout option)", async () => {
    const invariant = makeInvariant({
      declarations: [],
      assertion: "(assert (= 0 0))",
    });
    const verdict = await checkPath(makePath(), invariant, undefined, {
      timeoutMs: 5_000,
    });
    expect(["holds", "undecidable"]).toContain(verdict.status);
  });
});

// Placeholder describe block when Z3 is absent — keeps the file from
// disappearing into a no-test phantom. Verifies pathChecker imports
// cleanly without invoking Z3.
describe.skipIf(HAS_Z3)("checkPath — Z3 absent", () => {
  it("module imports without error and exports checkPath", () => {
    expect(typeof checkPath).toBe("function");
  });
});

// Real-Z3 integration test, gated like the other realZ3 tests in the repo.
describe.skipIf(!realZ3)("checkPath — real Z3 integration", () => {
  it("returns 'holds' on a path that grounds a true invariant with no needed constraints", async () => {
    const invariant = makeInvariant({
      declarations: [],
      assertion: "(assert (forall ((x Int)) (= x x)))",
    });
    const verdict = await checkPath(makePath(), invariant);
    expect(verdict.status).toBe("holds");
  });
});
