import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdirSync, writeFileSync, rmSync, existsSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { randomBytes } from "crypto";

import { GapDetectionPhase } from "./GapDetectionPhase.js";
import type { GapDetectionInput } from "./GapDetectionPhase.js";
import type { DerivationOutput } from "./DerivationPhase.js";
import type { Contract } from "../contracts.js";
import { openDb } from "../db/index.js";
import { gapReports } from "../db/schema/index.js";
import { PhaseOptions } from "./Phase.js";

// A self-contained fixture function — no imports, so loadModuleWithPrivates won't fail
const FIXTURE_SOURCE = `export function divide(a, b) {
  const q = a / b;
  return q;
}`;

// A Z3 witness using Real sort so ieeeSpecialsAgent fires.
// b is div_by_zero (Real), a is 1.0 (Real).
// synthesizeInputs materializes div_by_zero → NaN, so b=NaN at runtime.
// The runtime value of b will be NaN (kind="nan") while the witness sort is Real → agent fires.
const Z3_WITNESS = `(
  (define-fun b () Real (/ 1.0 0.0))
  (define-fun a () Real 1.0)
)`;

function makeTempRoot(): string {
  const dir = join(tmpdir(), `gdp-test-${randomBytes(6).toString("hex")}`);
  mkdirSync(dir, { recursive: true });
  return dir;
}

function makeContract(overrides: Partial<Contract> = {}): Contract {
  return {
    key: "src/fixture.ts/divide[2]",
    file: "src/fixture.ts",
    function: "divide",
    line: 2,
    signal_hash: "abc123",
    proven: [],
    violations: [],
    clause_history: [],
    depends_on: [],
    ...overrides,
  };
}

describe("GapDetectionPhase", () => {
  let tempRoot: string;
  let fixturePath: string;
  let options: PhaseOptions;
  const phase = new GapDetectionPhase();

  beforeEach(() => {
    tempRoot = makeTempRoot();
    // Create the fixture source file at the contract.file path
    const srcDir = join(tempRoot, "src");
    mkdirSync(srcDir, { recursive: true });
    fixturePath = join(srcDir, "fixture.ts");
    writeFileSync(fixturePath, FIXTURE_SOURCE);
    options = { projectRoot: tempRoot, verbose: false };
  });

  afterEach(() => {
    try {
      rmSync(tempRoot, { recursive: true, force: true });
    } catch {}
  });

  it("violation with bindings + witness produces a gap_reports row", async () => {
    const contract = makeContract({
      violations: [
        {
          principle: "division-by-zero",
          principle_hash: "",
          claim: "denominator can be zero",
          smt2: `; division by zero\n; PRINCIPLE: division-by-zero\n; LINE: 2\n(declare-const a Int)\n(declare-const b Int)\n(assert (= b 0))\n(check-sat)`,
          witness: Z3_WITNESS,
          smt_bindings: [
            { smt_constant: "b", source_line: 2, source_expr: "b", sort: "Real" },
            { smt_constant: "a", source_line: 2, source_expr: "a", sort: "Real" },
          ],
        },
      ],
    });

    const emptyDerivation: DerivationOutput = {
      contracts: [],
      newViolations: [],
      derivedAt: new Date().toISOString(),
    };

    const input: GapDetectionInput = {
      derivation: emptyDerivation,
      projectRoot: tempRoot,
      contracts: [contract],
    };

    const result = await phase.execute(input, options);

    expect(result.data.skipped.missingBindings).toBe(0);
    expect(result.data.skipped.missingWitness).toBe(0);
    expect(result.data.skipped.untestable).toBe(0);
    expect(result.data.reportsWritten).toBeGreaterThan(0);

    // Verify gap_reports row exists in the DB
    const dbPath = join(tempRoot, ".neurallog", "neurallog.db");
    expect(existsSync(dbPath)).toBe(true);
    const db = openDb(dbPath);
    const rows = db.select().from(gapReports).all();
    expect(rows.length).toBeGreaterThan(0);
  });

  it("violation without bindings increments skipped.missingBindings", async () => {
    const contract = makeContract({
      violations: [
        {
          principle: "division-by-zero",
          principle_hash: "",
          claim: "denominator can be zero",
          smt2: `; division by zero\n(check-sat)`,
          witness: Z3_WITNESS,
          // No smt_bindings
        },
      ],
    });

    const input: GapDetectionInput = {
      derivation: { contracts: [], newViolations: [], derivedAt: new Date().toISOString() },
      projectRoot: tempRoot,
      contracts: [contract],
    };

    const result = await phase.execute(input, options);

    expect(result.data.skipped.missingBindings).toBe(1);
    expect(result.data.reportsWritten).toBe(0);
  });

  it("violation with bindings but no witness increments skipped.missingWitness", async () => {
    const contract = makeContract({
      violations: [
        {
          principle: "division-by-zero",
          principle_hash: "",
          claim: "denominator can be zero",
          smt2: `; division by zero\n(check-sat)`,
          // No witness
          smt_bindings: [
            { smt_constant: "b", source_line: 2, source_expr: "b", sort: "Int" },
          ],
        },
      ],
    });

    const input: GapDetectionInput = {
      derivation: { contracts: [], newViolations: [], derivedAt: new Date().toISOString() },
      projectRoot: tempRoot,
      contracts: [contract],
    };

    const result = await phase.execute(input, options);

    expect(result.data.skipped.missingWitness).toBe(1);
    expect(result.data.reportsWritten).toBe(0);
  });

  it("DB file is created and migrations applied on first run", async () => {
    const input: GapDetectionInput = {
      derivation: { contracts: [], newViolations: [], derivedAt: new Date().toISOString() },
      projectRoot: tempRoot,
      contracts: [],
    };

    await phase.execute(input, options);

    const dbPath = join(tempRoot, ".neurallog", "neurallog.db");
    expect(existsSync(dbPath)).toBe(true);

    // Verify gap_reports table exists by querying it
    const db = openDb(dbPath);
    const rows = db.select().from(gapReports).all();
    expect(Array.isArray(rows)).toBe(true);
  });

  it("empty bindings array (length 0) increments skipped.missingBindings", async () => {
    const contract = makeContract({
      violations: [
        {
          principle: "division-by-zero",
          principle_hash: "",
          claim: "denominator can be zero",
          smt2: `; division by zero\n(check-sat)`,
          witness: Z3_WITNESS,
          smt_bindings: [], // explicitly empty
        },
      ],
    });

    const input: GapDetectionInput = {
      derivation: { contracts: [], newViolations: [], derivedAt: new Date().toISOString() },
      projectRoot: tempRoot,
      contracts: [contract],
    };

    const result = await phase.execute(input, options);

    expect(result.data.skipped.missingBindings).toBe(1);
    expect(result.data.reportsWritten).toBe(0);
  });
});
